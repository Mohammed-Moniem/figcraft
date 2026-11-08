//! Actual relay subprocesses against an authenticated bounded fake native app.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::disallowed_methods)]
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAAC";
struct Private {
    directory: PathBuf,
    token: PathBuf,
}
impl Private {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!("figcraft-attach-it-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&directory).unwrap();
        let token = directory.join("control.token");
        write_private(&token, TOKEN);
        Self { directory, token }
    }
}
impl Drop for Private {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn write_private(path: &Path, value: &str) {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).unwrap().write_all(value.as_bytes()).unwrap();
}
fn accept_fixture(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < end, "fake app accept deadline");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("{error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    stream
}

fn app(connections: usize, mut answer: impl FnMut(usize, &Value) -> Option<Value> + Send + 'static) -> (u16, std::thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let worker = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for connection in 0..connections {
            let stream = accept_fixture(&listener);
            let mut writer = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let auth: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(auth["method"], "auth");
            assert_eq!(auth["token"], TOKEN);
            writeln!(writer, "{}", json!({"id":null,"ok":true,"result":{"authenticated":true,"protocol":1}})).unwrap();
            loop {
                line.clear();
                let n = reader.read_line(&mut line).unwrap();
                if n == 0 {
                    break;
                }
                assert!(line.len() < 1024 * 1024, "fake app request bound");
                let request: Value = serde_json::from_str(&line).unwrap();
                seen.push(request.clone());
                let Some(result) = answer(connection, &request) else { break };
                writeln!(writer, "{}", json!({"id":request["id"],"ok":true,"result":result})).unwrap();
            }
        }
        seen
    });
    (port, worker)
}
fn invoke(port: u16, token: Option<&Path>, input: &[u8], extra: &[&str], environment: &[(&str, &Path)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_figcraft-attach"));
    cmd.args(["mcp", "--attach", &port.to_string()])
        .args(extra)
        .env_remove("FIGCRAFT_CONTROL_TOKEN_FILE")
        .env_remove("FIGCRAFT_LOCAL_DATA_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(token) = token {
        cmd.arg("--control-token-file").arg(token);
    }
    for (key, value) in environment {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    // A frame rejected early may close its pipe before all hostile bytes are written.
    let _ = stdin.write_all(input);
    drop(stdin);
    child.wait_with_output().unwrap()
}
fn transcript(messages: Vec<Value>) -> Vec<u8> {
    messages.into_iter().map(|v| v.to_string() + "\n").collect::<String>().into_bytes()
}
fn init() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"ScopedFixture","version":"1"}}})
}
fn tool(id: u64, name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
}
fn replies(output: &Output) -> Vec<Value> {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout.clone()).unwrap().lines().map(|line| serde_json::from_str(line).expect("stdout is protocol JSON only")).collect()
}
#[test]
fn actual_stdio_preserves_lifecycle_tools_resources_prompts_and_agent_document_target() {
    let private = Private::new();
    let (port, worker) = app(1, |_, r| {
        Some(match r["method"].as_str().unwrap() {
            "engine.execute" => json!({"id":7}),
            "node.get" => json!({"id":7,"name":"Native node"}),
            other => panic!("unexpected method {other}"),
        })
    });
    let input = transcript(vec![
        init(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"prompts/list"}),
        tool(4, "run_command", json!({"command":"shape.rectangle","params":{"width":"12px"},"document":"back-file"})),
        json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":"figcraft://node/0:7"}}),
    ]);
    let output = invoke(port, Some(&private.token), &input, &[], &[]);
    let r = replies(&output);
    assert_eq!(r.len(), 5);
    assert_eq!(r[0]["result"]["protocolVersion"], "2025-06-18");
    assert!(r[1]["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "screenshot"));
    assert!(!r[2]["result"]["prompts"].as_array().unwrap().is_empty());
    assert_eq!(r[3]["result"]["isError"], false);
    let calls = worker.join().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["params"]["agent"], "ScopedFixture");
    assert_eq!(calls[0]["params"]["document"], "back-file");
    assert_eq!(calls[0]["params"]["params"]["width"], "12px");
    assert_eq!(calls[1]["params"]["id"], 7);
}
#[test]
fn exports_and_window_screenshots_are_app_data_without_local_rendering() {
    let private = Private::new();
    let (port, worker) = app(1, |_, r| {
        Some(match r["method"].as_str().unwrap() {
            "engine.execute" => {
                assert_eq!(r["params"]["command"], "file.export");
                json!({"format":"png","data":PNG})
            }
            "ui.screenshot" => {
                assert_eq!(r["params"]["window"], 3);
                json!({"format":"png","data":PNG})
            }
            other => panic!("unexpected method {other}"),
        })
    });
    let output = invoke(
        port,
        Some(&private.token),
        &transcript(vec![init(), tool(2, "export", json!({"ids":[7],"format":"png"})), tool(3, "screenshot", json!({"window":3}))]),
        &[],
        &[],
    );
    let r = replies(&output);
    for result in &r[1..] {
        assert_eq!(result["result"]["isError"], false);
        assert_eq!(result["result"]["content"][0]["type"], "image");
        assert_eq!(result["result"]["content"][0]["data"], PNG);
    }
    assert_eq!(worker.join().unwrap().len(), 2);
}
#[test]
fn lost_mutation_reply_is_never_replayed_and_explicit_next_call_reauthenticates() {
    let private = Private::new();
    let (port, worker) = app(2, |connection, r| {
        assert_eq!(r["method"], "engine.execute");
        if connection == 0 { None } else { Some(json!({"applied":1})) }
    });
    let output = invoke(
        port,
        Some(&private.token),
        &transcript(vec![
            init(),
            tool(2, "run_command", json!({"command":"shape.rectangle"})),
            tool(3, "run_command", json!({"command":"doc.list"})),
        ]),
        &[],
        &[],
    );
    let r = replies(&output);
    assert_eq!(r[1]["result"]["isError"], true);
    assert!(r[1]["result"]["content"][0]["text"].as_str().unwrap().contains("may have completed"));
    assert_eq!(r[2]["result"]["isError"], false);
    let calls = worker.join().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["params"]["command"], "shape.rectangle");
    assert_eq!(calls[1]["params"]["command"], "doc.list");
}
#[test]
fn raised_timeout_survives_auth_reconnect_and_is_sent_on_requests() {
    let private = Private::new();
    let (port, worker) = app(2, |_, r| {
        assert_eq!(r["timeoutMs"], 300000);
        Some(json!({}))
    });
    let output = invoke(
        port,
        Some(&private.token),
        &transcript(vec![init(), tool(2, "run_command", json!({"command":"doc.list"}))]),
        &["--timeout", "300"],
        &[],
    );
    replies(&output);
    assert_eq!(worker.join().unwrap().len(), 1);
}
#[test]
fn oversized_unterminated_stdio_frame_stops_before_dispatching_a_following_command() {
    let private = Private::new();
    let (port, worker) = app(1, |_, _| panic!("oversized input must not call the app"));
    let mut input = vec![b' '; 32 * 1024 * 1024 + 1];
    input.push(b'\n');
    input.extend(transcript(vec![tool(2, "run_command", json!({"command":"shape.rectangle"}))]));
    let output = invoke(port, Some(&private.token), &input, &[], &[]);
    let r = replies(&output);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0]["error"]["code"], -32600);
    assert!(worker.join().unwrap().is_empty());
}
#[test]
fn explicit_token_precedes_environment_and_local_data_derivation_works() {
    let private = Private::new();
    let bad = private.directory.join("absent.token");
    let (port, worker) = app(1, |_, _| Some(json!({})));
    let output = invoke(port, Some(&private.token), &transcript(vec![init()]), &[], &[("FIGCRAFT_CONTROL_TOKEN_FILE", &bad)]);
    replies(&output);
    assert!(worker.join().unwrap().is_empty());
    let (port, worker) = app(1, |_, _| Some(json!({})));
    let derived = private.directory.join(format!("control-{port}.token"));
    write_private(&derived, TOKEN);
    let output = invoke(port, None, &transcript(vec![init()]), &[], &[("FIGCRAFT_LOCAL_DATA_DIR", &private.directory)]);
    replies(&output);
    assert!(worker.join().unwrap().is_empty());
}
#[test]
fn missing_or_malformed_private_credentials_fail_without_exposing_token() {
    let private = Private::new();
    let malformed = private.directory.join("malformed");
    write_private(&malformed, "private-do-not-log");
    for path in [&malformed, &private.directory, &private.directory.join("missing")] {
        let output = invoke(9, Some(path), b"", &[], &[]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-do-not-log"));
    }
}
#[cfg(unix)]
#[test]
fn symlink_and_nonprivate_token_are_refused_before_connecting() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let private = Private::new();
    let link = private.directory.join("link");
    symlink(&private.token, &link).unwrap();
    let output = invoke(9, Some(&link), b"", &[], &[]);
    assert_eq!(output.status.code(), Some(1));
    std::fs::set_permissions(&private.token, std::fs::Permissions::from_mode(0o644)).unwrap();
    let output = invoke(9, Some(&private.token), b"", &[], &[]);
    assert_eq!(output.status.code(), Some(1));
}
#[test]
fn unrecognized_headless_command_and_attach_without_argument_exit_with_usage() {
    for args in [vec!["mcp"], vec!["render", "design.figcraft"], vec!["mcp", "--attach", "bad-port"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_figcraft-attach")).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_figcraft-attach")).arg("--help").output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("mcp"));
}

#[test]
fn absent_implicit_credential_is_a_usage_error_matching_the_full_cli() {
    let output = invoke(9, None, b"", &[], &[]);
    assert_eq!(output.status.code(), Some(2), "no credential source is a usage error, not an unreachable app");
    assert!(output.stdout.is_empty());
}
#[test]
fn refused_or_wrong_protocol_authentication_sends_no_ordinary_request() {
    for auth_result in [json!({"ok":false}), json!({"ok":true,"result":{"authenticated":true,"protocol":2}})] {
        let private = Private::new();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let stream = accept_fixture(&listener);
            let mut writer = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let auth: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(auth["method"], "auth");
            writeln!(writer, "{auth_result}").unwrap();
            line.clear();
            assert_eq!(reader.read_line(&mut line).unwrap(), 0, "failed auth cannot dispatch a command");
        });
        let output = invoke(port, Some(&private.token), &transcript(vec![tool(2, "run_command", json!({"command":"shape.rectangle"}))]), &[], &[]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains(TOKEN));
        worker.join().unwrap();
    }
}
#[test]
fn oversized_token_file_and_unreachable_app_fail_closed() {
    let private = Private::new();
    let oversized = private.directory.join("oversized");
    write_private(&oversized, &"A".repeat(1025));
    let output = invoke(9, Some(&oversized), b"", &[], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let output = invoke(port, Some(&private.token), b"", &[], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}
