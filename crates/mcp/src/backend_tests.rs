//! The attached backend reads the app's replies with a bound (#1113).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::json;

use super::{Attached, Backend};

/// A fake app that takes one connection per reply, reads one request on it and answers with the
/// reply. It keeps the connections open until [`finish`] (so an unterminated reply never reaches
/// EOF), then returns how many requests it read.
fn fake_app(replies: Vec<Vec<u8>>) -> (u16, mpsc::Sender<()>, std::thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (done, wait) = mpsc::channel::<()>();
    let app = std::thread::spawn(move || {
        let mut open = Vec::new();
        for reply in &replies {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).unwrap();
            out.write_all(reply).unwrap();
            out.flush().unwrap();
            open.push(out);
        }
        wait.recv_timeout(Duration::from_secs(30)).ok();
        open.len()
    });
    (port, done, app)
}

/// Let the fake app close its connections, and check it read `requests` requests.
fn finish(done: mpsc::Sender<()>, app: std::thread::JoinHandle<usize>, requests: usize) {
    drop(done);
    assert_eq!(app.join().unwrap(), requests);
}

fn ok_reply(text: &str) -> Vec<u8> {
    format!("{}\n", json!({"id": 1, "ok": true, "result": text})).into_bytes()
}

/// #1696: a raised timeout goes with every request; without one the line is as it always was.
#[test]
fn a_raised_timeout_is_asked_for_on_each_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // Reads the requests of two calls; `with_timeout`'s dropped connection brings none.
    let app = std::thread::spawn(move || {
        let mut lines = Vec::new();
        while lines.len() < 2 {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut line = String::new();
            if BufReader::new(stream).read_line(&mut line).unwrap() > 0 {
                out.write_all(&ok_reply("fine")).unwrap();
                lines.push(serde_json::from_str::<serde_json::Value>(&line).unwrap());
            }
        }
        lines
    });
    Attached::port(port).unwrap().call("engine.commands", json!({})).unwrap();
    let mut long = Attached::port(port).unwrap().with_timeout(Duration::from_secs(300)).unwrap();
    long.call("engine.commands", json!({})).unwrap();
    let lines = app.join().unwrap();
    assert_eq!(lines[0].get("timeoutMs"), None, "{:?}", lines[0]);
    assert_eq!(lines[1]["timeoutMs"], 300_000, "{:?}", lines[1]);
}

#[test]
fn an_oversized_reply_is_refused_and_the_next_call_reconnects() {
    let (port, done, fake) = fake_app(vec![vec![b'x'; 200], ok_reply("fine")]);
    let mut app = Attached::port(port).unwrap();
    app.max_reply = 64;
    let err = app.call("engine.commands", json!({})).unwrap_err();
    assert!(err.contains("reply larger than"), "{err}");
    assert!(err.contains("may have completed"), "{err}");
    assert!(app.conn.is_none());
    assert_eq!(app.call("engine.commands", json!({})).unwrap(), json!("fine"));
    finish(done, fake, 2);
}

#[test]
fn a_reply_at_the_limit_is_read() {
    let reply = ok_reply("at the limit");
    let (port, done, fake) = fake_app(vec![reply.clone()]);
    let mut app = Attached::port(port).unwrap();
    app.max_reply = reply.len() - 1; // the newline doesn't count
    assert_eq!(app.call("engine.commands", json!({})).unwrap(), json!("at the limit"));
    finish(done, fake, 1);
}

#[test]
fn a_reply_that_is_not_utf8_is_an_error() {
    let (port, done, fake) = fake_app(vec![b"{\"ok\": true, \"result\": \"\xff\"}\n".to_vec()]);
    let mut app = Attached::port(port).unwrap();
    let err = app.call("engine.commands", json!({})).unwrap_err();
    assert!(err.contains("failed"), "{err}");
    assert!(app.conn.is_none());
    finish(done, fake, 1);
}

#[test]
fn authenticated_backend_reauthenticates_reconnections_and_never_sends_before_acceptance() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let token = "x".repeat(44);
    let expected = token.clone();
    let worker = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for index in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut read = BufReader::new(stream.try_clone().unwrap());
            let mut out = stream;
            let mut line = String::new();
            read.read_line(&mut line).unwrap();
            let auth: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(auth["method"], "auth");
            assert_eq!(auth["token"], expected);
            writeln!(out, "{}", json!({"ok":true,"result":{"authenticated":true,"protocol":1}})).unwrap();
            line.clear();
            read.read_line(&mut line).unwrap();
            let command: serde_json::Value = serde_json::from_str(&line).unwrap();
            seen.push(command["method"].clone());
            writeln!(out, "{}", json!({"ok":true,"result":{"index":index}})).unwrap();
        }
        seen
    });
    let mut app = Attached::connect_authenticated(&format!("127.0.0.1:{port}"), token).unwrap();
    assert_eq!(app.call("document.inspect", json!({})).unwrap()["index"], 0);
    app.conn = None;
    assert_eq!(app.call("engine.commands", json!({})).unwrap()["index"], 1);
    assert_eq!(worker.join().unwrap(), vec![json!("document.inspect"), json!("engine.commands")]);
}
#[test]
fn malformed_or_oversized_authentication_reply_fails_before_a_design_command() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut read = BufReader::new(stream.try_clone().unwrap());
        let mut out = stream;
        let mut line = String::new();
        read.read_line(&mut line).unwrap();
        assert!(line.contains("auth"));
        out.write_all(&vec![b'x'; 4098]).unwrap();
        out.flush().unwrap();
        out.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        line.clear();
        read.read_line(&mut line).ok();
        assert!(line.is_empty(), "no design request follows a refused credential handshake");
    });
    assert!(Attached::connect_authenticated(&format!("127.0.0.1:{port}"), "x".repeat(44)).is_err());
    worker.join().unwrap();
}

/// A finite fake app: the first reply can announce a terminal refusal while its socket stays
/// open. The next RPC may arrive on that socket or a new one, and the trace distinguishes them.
type RecoveryTrace = Vec<(usize, serde_json::Value)>;

struct RecoveryApp {
    port: u16,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<std::io::Result<RecoveryTrace>>>,
}

fn recovery_request(reader: &mut BufReader<std::net::TcpStream>) -> std::io::Result<serde_json::Value> {
    let mut bytes = Vec::new();
    std::io::Read::take(&mut *reader, 4097).read_until(b'\n', &mut bytes)?;
    if bytes.len() > 4096 || bytes.last() != Some(&b'\n') {
        return Err(std::io::Error::other("fixture requires one bounded complete request"));
    }
    serde_json::from_slice(&bytes).map_err(std::io::Error::other)
}

fn recovery_accept(listener: &TcpListener, stop: &std::sync::atomic::AtomicBool) -> std::io::Result<std::net::TcpStream> {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !stop.load(std::sync::atomic::Ordering::Acquire) && std::time::Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_millis(500)))?;
                stream.set_write_timeout(Some(Duration::from_millis(500)))?;
                return Ok(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(1)),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "fixture accept deadline"))
}

fn recovery_reply(stream: &mut std::net::TcpStream, request: &serde_json::Value) -> std::io::Result<()> {
    writeln!(stream, "{}", json!({"id":request["id"],"ok":true,"result":{"recovered":true}}))?;
    stream.flush()
}

impl RecoveryApp {
    fn new(first_reply: serde_json::Value, terminal: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signal = stop.clone();
        let worker = std::thread::spawn(move || {
            let mut first = BufReader::new(recovery_accept(&listener, &signal)?);
            let mut trace = vec![(1, recovery_request(&mut first)?)];
            writeln!(first.get_mut(), "{first_reply}")?;
            first.get_mut().flush()?;
            first.get_mut().set_nonblocking(true)?;
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let mut old_bytes = Vec::new();
            let mut old_complete = false;
            while !signal.load(std::sync::atomic::Ordering::Acquire) && std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false)?;
                        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
                        stream.set_write_timeout(Some(Duration::from_millis(500)))?;
                        let request = recovery_request(&mut BufReader::new(stream.try_clone()?))?;
                        recovery_reply(&mut stream, &request)?;
                        trace.push((2, request));
                        return Ok(trace);
                    }
                    Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => {}
                    Err(error) => return Err(error),
                }
                if !old_complete {
                    let mut chunk = [0; 512];
                    match std::io::Read::read(&mut first, &mut chunk) {
                        Ok(0) => old_complete = true,
                        Ok(count) => {
                            old_bytes.extend_from_slice(&chunk[..count]);
                            if old_bytes.len() > 4096 {
                                return Err(std::io::Error::other("fixture request exceeds bound"));
                            }
                            if old_bytes.contains(&b'\n') {
                                let request = serde_json::from_slice(&old_bytes).map_err(std::io::Error::other)?;
                                if !terminal {
                                    first.get_mut().set_nonblocking(false)?;
                                    recovery_reply(first.get_mut(), &request)?;
                                }
                                trace.push((1, request));
                                old_complete = true;
                                if !terminal {
                                    return Ok(trace);
                                }
                                // A refused socket never dispatches another command. Keep it
                                // open so old clients fail by their short read timeout, not FIN.
                            }
                        }
                        Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => {}
                        Err(error) => return Err(error),
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(trace)
        });
        Self { port, stop, worker: Some(worker) }
    }

    fn finish(mut self) -> RecoveryTrace {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        self.worker.take().unwrap().join().unwrap().unwrap()
    }
}

impl Drop for RecoveryApp {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn refusal_recovery_case(first_reply: serde_json::Value, terminal: bool) {
    let fake = RecoveryApp::new(first_reply, terminal);
    let mut app = Attached::port(fake.port).unwrap();
    // Bound the old candidate's immediate second call while its first socket remains open.
    app.conn.as_mut().unwrap().0.get_mut().set_read_timeout(Some(Duration::from_millis(80))).unwrap();
    let error = app.call("engine.commands", json!({})).unwrap_err();
    let kept = app.conn.is_some();
    let next = app.call("document.inspect", json!({}));
    let trace = fake.finish();
    assert_eq!(error, if terminal { "control memory budget exhausted" } else { "synthetic UI error" });
    assert_eq!(kept, !terminal, "only an explicit terminal refusal invalidates the connection immediately; next={next:?}; trace={trace:?}");
    assert_eq!(next.unwrap(), json!({"recovered":true}));
    assert_eq!(trace.len(), 2, "neither the refused operation nor the next operation is replayed");
    assert_eq!(trace[0].0, 1);
    assert_eq!(trace[0].1["id"], 1);
    assert_eq!(trace[0].1["method"], "engine.commands");
    assert_eq!(trace[1].0, if terminal { 2 } else { 1 });
    assert_eq!(trace[1].1["id"], 2);
    assert_eq!(trace[1].1["method"], "document.inspect");
}

#[test]
fn terminal_null_id_refusal_reconnects_the_next_call_without_replaying() {
    refusal_recovery_case(json!({"id":null,"ok":false,"error":"control memory budget exhausted"}), true);
}

#[test]
fn numeric_id_ui_error_preserves_the_connection_for_the_next_call() {
    refusal_recovery_case(json!({"id":1,"ok":false,"error":"synthetic UI error"}), false);
}

#[test]
fn missing_id_error_is_not_classified_as_an_explicit_terminal_refusal() {
    refusal_recovery_case(json!({"ok":false,"error":"synthetic UI error"}), false);
}
