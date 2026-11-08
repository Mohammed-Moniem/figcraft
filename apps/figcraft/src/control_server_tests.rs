#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use std::io::{BufRead, Read};
use std::net::Shutdown;
use std::sync::mpsc;
use std::time::Instant;

/// Test-only scheduler: hold thread retirement after the real bounded session has finished.
/// It models delayed worker retirement, not a longer production authentication/drain deadline.
#[derive(Clone)]
pub(super) struct RetirementHook {
    entered: mpsc::Sender<()>,
    release: Arc<(Mutex<usize>, std::sync::Condvar)>,
}

pub(super) fn observe_worker_retirement(inner: &ControlInner) {
    let hook = inner.retirement_hook.lock().unwrap().clone();
    if let Some(hook) = hook {
        let _ = hook.entered.send(());
        // No test failure can leave a worker blocked indefinitely; fixture Drop also releases.
        let (permits, changed) = &*hook.release;
        let (mut permits, _) = changed.wait_timeout_while(permits.lock().unwrap(), Duration::from_secs(2), |count| *count == 0).unwrap();
        if *permits > 0 {
            *permits -= 1;
        }
    }
}

pub(super) fn observe_full_capacity(inner: Option<&ControlInner>, workers: usize) {
    if let Some(inner) = inner
        && let Some(observer) = inner.capacity_observer.lock().unwrap().as_ref()
    {
        let _ = observer.try_send(workers);
    }
}

mod connection_backpressure_tests {
    use super::*;

    const INVALID: &[u8] =
        b"{\"method\":\"auth\",\"token\":\"owned-wrong-token\"}\n{\"id\":\"never-admit\",\"method\":\"engine.commands\",\"params\":{}}\n";
    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct Server {
        handle: ControlHandle,
        pending: Queue,
        directory: std::path::PathBuf,
        release: Arc<(Mutex<usize>, std::sync::Condvar)>,
        entered: mpsc::Receiver<()>,
        full: mpsc::Receiver<usize>,
        port: u16,
    }
    impl Server {
        fn new() -> Self {
            let directory = loop {
                let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
                assert!(sequence < 1024, "bounded owned directory allocation");
                let path = std::env::temp_dir().join(format!("figcraft-control-backpressure-{}-{sequence}", std::process::id()));
                match std::fs::create_dir(&path) {
                    Ok(()) => break path,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("owned test directory creation failed: {error}"),
                }
            };
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            let handle = ControlHandle::new();
            let pending = queue();
            handle.bind_deps(pending.clone(), context(), Arc::new(EventLog::default()));
            handle.set_data_dir(Some(directory.clone()));
            let release = Arc::new((Mutex::new(0), std::sync::Condvar::new()));
            let (entered_tx, entered) = mpsc::channel();
            let (full_tx, full) = mpsc::sync_channel(1);
            *handle.inner.retirement_hook.lock().unwrap() = Some(RetirementHook { entered: entered_tx, release: release.clone() });
            *handle.inner.capacity_observer.lock().unwrap() = Some(full_tx);
            handle.apply(true, port, Duration::ZERO);
            assert_eq!(handle.status().0, port);
            Self { handle, pending, directory, release, entered, full, port }
        }

        fn client(&self) -> TcpStream {
            let client = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
            client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            client.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
            client
        }

        fn fill_retiring_workers(&self) -> Vec<TcpStream> {
            let mut peers = Vec::new();
            for _ in 0..MAX_CONNECTIONS {
                let mut peer = self.client();
                peer.write_all(INVALID).unwrap();
                let (bytes, error) = receive(&mut peer);
                assert_eq!(error, None);
                assert_refusal(&bytes);
                // Keep the client's write half open; real refusal cleanup runs its50ms cap.
                peers.push(peer);
            }
            let deadline = Instant::now() + Duration::from_secs(1);
            for _ in 0..MAX_CONNECTIONS {
                self.entered
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .expect("all8 real sessions reach the controlled retirement gate");
            }
            assert_eq!(MAX_CONNECTIONS, 8, "worker capacity is unchanged");
            assert_eq!(self.handle.status().1, 8, "all8 retained workers are observed, not inferred from sleep");
            // The first8 already cloned their gate. A later worker must not inherit this hold.
            self.handle.inner.retirement_hook.lock().unwrap().take();
            while self.full.try_recv().is_ok() {}
            peers
        }

        fn observe_full(&self) {
            assert_eq!(self.full.recv_timeout(Duration::from_secs(1)).unwrap(), 8, "listener observes the exact full-capacity boundary");
            assert_eq!(self.handle.status().1, 8);
        }

        fn release_one(&self) {
            let (permits, changed) = &*self.release;
            *permits.lock().unwrap() += 1;
            changed.notify_one();
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            // Always release every held worker, including during assertion unwinding.
            let (permits, changed) = &*self.release;
            *permits.lock().unwrap() += MAX_CONNECTIONS;
            changed.notify_all();
            self.handle.apply(false, 0, Duration::ZERO);
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    fn receive(client: &mut TcpStream) -> (Vec<u8>, Option<std::io::ErrorKind>) {
        let mut bytes = Vec::new();
        let mut buffer = [0; 256];
        loop {
            match client.read(&mut buffer) {
                Ok(0) => return (bytes, None),
                Ok(n) => {
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() <= 4096);
                }
                Err(error) => return (bytes, Some(error.kind())),
            }
        }
    }

    fn assert_refusal(bytes: &[u8]) {
        assert_eq!(bytes.len(), 65, "one complete bounded refusal, never a dropped connection");
        assert_eq!(bytes.last(), Some(&b'\n'));
        assert_eq!(serde_json::from_slice::<Value>(bytes).unwrap(), json!({"id": null, "ok": false, "error": "control authentication required"}));
    }

    #[test]
    fn ninth_invalid_pipeline_waits_for_worker_retirement_then_delivers_full_refusal() {
        let server = Server::new();
        let peers = server.fill_retiring_workers();
        let mut ninth = server.client();
        let written = ninth.write_all(INVALID);
        server.observe_full();
        server.release_one();
        let (bytes, error) = receive(&mut ninth);
        assert!(written.is_ok(), "ninth accepted client was dropped before its write");
        assert_eq!(error, None, "ninth bounded pipeline must finish with FIN");
        assert_refusal(&bytes);
        assert!(server.pending.lock().unwrap().is_empty());
        assert!(server.handle.status().1 <= MAX_CONNECTIONS);
        drop(peers);
    }

    #[test]
    fn ninth_authenticated_pipeline_is_admitted_only_after_a_worker_retires() {
        let server = Server::new();
        let peers = server.fill_retiring_workers();
        let token = std::fs::read_to_string(server.directory.join(format!("control-{}.token", server.port))).unwrap();
        let mut ninth = server.client();
        let mut input = serde_json::to_vec(&json!({"method": "auth", "token": token})).unwrap();
        input.extend_from_slice(b"\n{\"id\":\"ninth\",\"method\":\"engine.commands\",\"params\":{}}\n");
        let written = ninth.write_all(&input);
        server.observe_full();
        assert!(server.pending.lock().unwrap().is_empty(), "waiting sockets cannot enter the command queue");
        server.release_one();
        assert!(written.is_ok(), "capacity drop must not replace bounded pre-admission waiting");
        let mut reader = BufReader::new(ninth);
        assert_eq!(reply(&mut reader), json!({"id": null, "ok": true, "result": {"authenticated": true, "protocol": 1}}));
        let deadline = Instant::now() + Duration::from_secs(1);
        let request = loop {
            let mut queue = server.pending.lock().unwrap();
            assert!(queue.len() <= 1);
            if let Some(request) = queue.pop() {
                break request;
            }
            drop(queue);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(request.id, "ninth");
        assert_eq!(request.method, "engine.commands");
        assert!(server.handle.status().1 <= MAX_CONNECTIONS);
        request.reply.send(ControlReply { id: request.id, result: Ok(json!({"admitted": true})) }).unwrap();
        assert_eq!(reply(&mut reader), json!({"id": "ninth", "ok": true, "result": {"admitted": true}}));
        reader.get_ref().shutdown(Shutdown::Write).unwrap();
        drop(peers);
    }

    #[test]
    fn generation_retirement_stops_a_listener_while_all_workers_are_held() {
        let server = Server::new();
        let peers = server.fill_retiring_workers();
        let mut ninth = server.client();
        let _ = ninth.write_all(INVALID);
        server.observe_full();
        server.handle.apply(false, 0, Duration::ZERO);
        assert_eq!(server.handle.status().0, 0);
        let deadline = Instant::now() + Duration::from_secs(1);
        let address = std::net::SocketAddr::from(([127, 0, 0, 1], server.port));
        loop {
            match TcpStream::connect_timeout(&address, Duration::from_millis(10)) {
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => break,
                _ => assert!(Instant::now() < deadline, "retired full listener must close without awaiting worker release"),
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(server.pending.lock().unwrap().is_empty());
        assert_eq!(server.handle.status().1, MAX_CONNECTIONS, "listener shutdown did not depend on worker gate release");
        drop(peers);
    }
}

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    client.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    (client, listener.accept().unwrap().0)
}

fn queue() -> Queue {
    Arc::new(Mutex::new(Vec::new()))
}

fn context() -> CtxSlot {
    Arc::new(Mutex::new(None))
}

/// Regression for #1111. Eight actual sockets send compact, allocation-amplifying JSON. The
/// fixture is small enough to run on the original implementation without stressing host RAM.
#[test]
fn shared_budget_dense_tcp_frames_are_refused_before_dispatch() {
    let pending = queue();
    let mut workers = Vec::new();
    let mut clients = Vec::new();
    for id in 0..8 {
        let (mut client, server) = pair();
        let pending = pending.clone();
        workers.push(std::thread::spawn(move || serve(server, pending, context(), Duration::from_millis(100)).unwrap()));
        let mut frame = format!("{{\"id\":{id},\"method\":\"engine.commands\",\"params\":{{\"unused\":[");
        for _ in 0..25_000 {
            frame.push_str("null,");
        }
        frame.push_str("null]}}\n");
        client.write_all(frame.as_bytes()).unwrap();
        clients.push(BufReader::new(client));
    }
    let replies: Vec<_> = clients.iter_mut().map(reply).collect();
    drop(clients);
    for worker in workers {
        worker.join().unwrap();
    }
    assert!(
        replies.iter().all(|reply| reply["error"].as_str().is_some_and(|error| error.contains("memory budget"))),
        "all dense frames must fail the shared pre-parse memory admission: {replies:?}"
    );
    assert!(pending.lock().unwrap().is_empty(), "refused input must not reach UI dispatch");
}

fn reply(reader: &mut BufReader<TcpStream>) -> Value {
    let mut line = String::new();
    assert!(reader.read_line(&mut line).unwrap() > 0);
    serde_json::from_str(&line).unwrap()
}

/// Build a `ui.inspect` control request with a throwaway reply channel.
fn request() -> ControlRequest {
    ControlRequest { id: json!(0), method: "ui.inspect".into(), params: json!({}), reply: mpsc::channel().0, state: Default::default() }
}

/// Real TCP coverage for the pre-authentication boundary, including unread pipelined bytes.
/// These fixtures use only the existing server/Lease API and also compile against the old code.
mod auth_refusal_delivery_tests {
    use super::*;

    const PIPELINE: &[u8] =
        b"{\"method\":\"auth\",\"token\":\"owned-wrong-token\"}\n{\"id\":\"owned-read-only-probe\",\"method\":\"engine.commands\",\"params\":{}}\n";
    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct Credentials {
        directory: std::path::PathBuf,
        lease: Arc<crate::control_auth::Lease>,
    }
    impl Credentials {
        fn new() -> Self {
            for _ in 0..1024 {
                let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
                let directory = std::env::temp_dir().join(format!("figcraft-auth-delivery-{}-{sequence}", std::process::id()));
                match std::fs::create_dir(&directory) {
                    Ok(()) => {
                        let lease = Arc::new(crate::control_auth::Lease::create(&directory, 0).unwrap());
                        return Self { directory, lease };
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("cannot create owned auth fixture: {error}"),
                }
            }
            panic!("cannot allocate an unused owned auth fixture directory");
        }
    }
    impl Drop for Credentials {
        fn drop(&mut self) {
            self.lease.retire();
            std::fs::remove_dir_all(&self.directory).unwrap();
        }
    }

    fn spawn(credentials: &Credentials) -> (TcpStream, Queue, std::thread::JoinHandle<std::io::Result<()>>, mpsc::Receiver<()>) {
        let (client, server) = pair();
        let pending = queue();
        let queue = pending.clone();
        let lease = credentials.lease.clone();
        let (finished, completion) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = serve_session(server, queue, context(), Arc::new(EventLog::default()), Duration::from_secs(2), Some(lease));
            let _ = finished.send(());
            result
        });
        (client, pending, worker, completion)
    }

    fn collect(client: &mut TcpStream) -> (Vec<u8>, Option<std::io::ErrorKind>) {
        let mut bytes = Vec::new();
        let mut buffer = [0; 256];
        loop {
            match client.read(&mut buffer) {
                Ok(0) => return (bytes, None),
                Ok(n) => {
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() <= 4096, "authentication emitted an unbounded response");
                }
                Err(error) => return (bytes, Some(error.kind())),
            }
        }
    }

    fn assert_refused(bytes: &[u8], error: Option<std::io::ErrorKind>) {
        assert_eq!(error, None, "a bounded refused pipeline must end with FIN, not reset");
        assert_eq!(bytes.last(), Some(&b'\n'), "the complete JSON line must arrive before EOF");
        assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 1, "no command reply may follow refusal");
        let value: Value = serde_json::from_slice(bytes).expect("refusal is a complete JSON frame");
        assert_eq!(value, json!({"id": null, "ok": false, "error": "control authentication required"}));
    }

    #[test]
    fn bounded_invalid_auth_pipeline_delivers_one_complete_refusal_on_real_tcp_32_times() {
        let credentials = Credentials::new();
        assert_eq!(PIPELINE.len(), 116, "same bounded wire input as the observed failure");
        for attempt in 0..32 {
            let (mut client, pending, worker, _) = spawn(&credentials);
            client.write_all(PIPELINE).unwrap();
            let (bytes, error) = collect(&mut client);
            drop(client); // Let graceful cleanup observe peer EOF without extending this fixture.
            let served = worker.join().unwrap();
            assert!(pending.lock().unwrap().is_empty(), "attempt {attempt} admitted a command");
            assert!(served.is_ok(), "attempt {attempt} failed in the transport");
            assert_refused(&bytes, error);
        }
    }

    #[test]
    fn malformed_auth_discards_bounded_non_json_tail_without_parsing_or_dispatch() {
        for attempt in 1..=64 {
            eprintln!("malformed-tail attempt {attempt}/64");
            let credentials = Credentials::new();
            let (mut client, pending, worker, _) = spawn(&credentials);
            let mut input = b"not-auth-json\n".to_vec();
            input.extend_from_slice(&[0xff; 3000]);
            input.extend_from_slice(b"\n{\"id\":1,\"method\":\"engine.commands\"}\n");
            client.write_all(&input).unwrap();
            let (bytes, error) = collect(&mut client);
            drop(client);
            worker.join().unwrap().unwrap_or_else(|error| panic!("malformed-tail attempt {attempt}/64: {error}"));
            assert!(pending.lock().unwrap().is_empty());
            assert_refused(&bytes, error);
        }
    }

    #[test]
    fn refused_peer_that_keeps_its_write_half_open_cannot_hold_the_worker() {
        let credentials = Credentials::new();
        let (mut client, pending, worker, completion) = spawn(&credentials);
        client.write_all(PIPELINE).unwrap();
        let (bytes, error) = collect(&mut client);
        // Deliberately keep client alive: worker completion must not depend on client EOF.
        let completed = completion.recv_timeout(Duration::from_millis(750));
        drop(client);
        worker.join().unwrap().unwrap();
        assert!(completed.is_ok(), "refusal cleanup waits beyond its bounded grace");
        assert!(pending.lock().unwrap().is_empty());
        assert_refused(&bytes, error);
    }

    #[test]
    fn over_budget_refused_input_still_closes_without_admission_or_unbounded_drain() {
        let credentials = Credentials::new();
        let (mut client, pending, worker, completion) = spawn(&credentials);
        let mut input = b"{\"method\":\"auth\",\"token\":\"owned-wrong-token\"}\n".to_vec();
        input.extend_from_slice(&[b' '; 8192]);
        let start = Instant::now();
        client.write_all(&input).unwrap();
        let completed = completion.recv_timeout(Duration::from_millis(750));
        drop(client);
        worker.join().unwrap().unwrap();
        assert!(completed.is_ok() && start.elapsed() < Duration::from_secs(2));
        assert!(pending.lock().unwrap().is_empty());
        // Reset is permitted beyond the explicit drain budget; no completeness claim here.
    }

    #[test]
    fn oversized_unterminated_auth_keeps_the_existing_1024_byte_limit() {
        let credentials = Credentials::new();
        let (mut client, pending, worker, completion) = spawn(&credentials);
        client.write_all(&[b' '; 1025]).unwrap();
        let completed = completion.recv_timeout(Duration::from_millis(750));
        drop(client);
        worker.join().unwrap().unwrap();
        assert!(completed.is_ok());
        assert!(pending.lock().unwrap().is_empty());
    }

    #[test]
    fn idle_authentication_keeps_its_absolute_two_second_deadline() {
        let credentials = Credentials::new();
        let (client, pending, worker, completion) = spawn(&credentials);
        let start = Instant::now();
        let completed = completion.recv_timeout(Duration::from_millis(3500));
        let elapsed = start.elapsed();
        drop(client);
        let served = worker.join().unwrap();
        assert!(completed.is_ok());
        assert!(elapsed >= Duration::from_millis(1500) && elapsed < Duration::from_millis(3500));
        assert!(served.is_err(), "an idle read still times out without a protocol response");
        assert!(pending.lock().unwrap().is_empty());
    }

    #[test]
    fn successful_auth_preserves_a_pipelined_command_and_normal_connection_lifetime() {
        let credentials = Credentials::new();
        let token = std::fs::read_to_string(credentials.directory.join("control-0.token")).unwrap();
        let (mut client, pending, worker, _) = spawn(&credentials);
        let mut input = serde_json::to_vec(&json!({"method": "auth", "token": token})).unwrap();
        input.extend_from_slice(b"\n{\"id\":\"admitted\",\"method\":\"engine.commands\",\"params\":{}}\n");
        client.write_all(&input).unwrap();
        let mut reader = BufReader::new(client);
        assert_eq!(reply(&mut reader), json!({"id": null, "ok": true, "result": {"authenticated": true, "protocol": 1}}));
        let start = Instant::now();
        let request = loop {
            if let Some(request) = pending.lock().unwrap().pop() {
                break request;
            }
            assert!(start.elapsed() < Duration::from_secs(1));
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(request.id, "admitted");
        assert_eq!(request.method, "engine.commands");
        request.reply.send(ControlReply { id: request.id, result: Ok(json!({"accepted": true})) }).unwrap();
        assert_eq!(reply(&mut reader), json!({"id": "admitted", "ok": true, "result": {"accepted": true}}));
        reader.get_ref().shutdown(Shutdown::Write).unwrap();
        worker.join().unwrap().unwrap();
        assert!(pending.lock().unwrap().is_empty());
    }
}

#[test]
fn rejects_an_oversized_unterminated_frame_without_dispatch() {
    let (mut client, server) = pair();
    let pending = queue();
    let q = pending.clone();
    let worker = std::thread::spawn(move || serve(server, q, context(), Duration::from_secs(5)));
    // Generate bytes as they are written; no giant input string and no terminating newline.
    std::io::copy(&mut std::io::repeat(b' ').take((MAX_LINE + 1) as u64), &mut client).unwrap();
    let mut reader = BufReader::new(client);
    assert_eq!(reply(&mut reader), json!({"id": null, "ok": false, "error": "message too large"}));
    assert_eq!(reader.read(&mut [0]).unwrap(), 0);
    worker.join().unwrap().unwrap();
    assert!(pending.lock().unwrap().is_empty());
}

#[test]
fn valid_frames_keep_their_order_ids_and_parameters() {
    let (mut client, server) = pair();
    let pending = queue();
    let q = pending.clone();
    let worker = std::thread::spawn(move || serve(server, q, context(), Duration::from_secs(5)));
    client.write_all(b"\r\n{\"id\":\"test\",\"method\":\"engine.execute\",\"params\":{\"command\":\"shape.rectangle\"}}\r\n").unwrap();
    let mut reader = BufReader::new(client);
    let start = Instant::now();
    let r = loop {
        if let Some(r) = pending.lock().unwrap().pop() {
            break r;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(r.id, "test");
    assert_eq!(r.method, "engine.execute");
    assert_eq!(r.params, json!({"command": "shape.rectangle"}));
    r.reply.send(figcraft_ui_egui::ControlReply { id: r.id, result: Ok(json!({"id": 42})) }).unwrap();
    assert_eq!(reply(&mut reader), json!({"id": "test", "ok": true, "result": {"id": 42}}));
    reader.get_ref().shutdown(Shutdown::Write).unwrap();
    worker.join().unwrap().unwrap();
}

#[test]
fn a_line_that_is_not_json_ends_the_connection_before_anything_after_it_runs() {
    // What a browser sends when a web page posts to the port: the JSON line in its body never runs.
    let (mut client, server) = pair();
    let pending = queue();
    let q = pending.clone();
    let worker = std::thread::spawn(move || serve(server, q, context(), Duration::from_secs(5)));
    let body = "{\"id\":1,\"method\":\"engine.execute\",\"params\":{\"command\":\"file.save\"}}\n";
    let http = format!("POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    client.write_all(http.as_bytes()).unwrap();
    let mut reader = BufReader::new(client);
    assert_eq!(reply(&mut reader)["ok"], false);
    assert_eq!(reader.read(&mut [0]).unwrap(), 0, "the connection is closed");
    worker.join().unwrap().unwrap();
    assert!(pending.lock().unwrap().is_empty(), "nothing was dispatched");
}

#[test]
fn full_queue_rejects_before_dispatch_and_ui_queue_stays_bounded() {
    let (mut client, server) = pair();
    let pending = queue();
    pending.lock().unwrap().extend((0..MAX_QUEUED_REQUESTS).map(|_| request()));
    let mut ui_queue = (0..MAX_QUEUED_REQUESTS - 1).map(|_| request()).collect();
    drain_queue(&pending, &mut ui_queue);
    assert_eq!(ui_queue.len(), MAX_QUEUED_REQUESTS);
    assert_eq!(pending.lock().unwrap().len(), MAX_QUEUED_REQUESTS - 1);
    pending.lock().unwrap().push(request());
    drain_queue(&pending, &mut ui_queue);
    assert_eq!(pending.lock().unwrap().len(), MAX_QUEUED_REQUESTS);
    let q = pending.clone();
    let worker = std::thread::spawn(move || serve(server, q, context(), Duration::from_secs(5)));
    client.write_all(b"{\"id\":1,\"method\":\"engine.execute\"}\n").unwrap();
    let mut reader = BufReader::new(client);
    assert_eq!(reply(&mut reader), json!({"id": 1, "ok": false, "error": "control request queue is full"}));
    assert_eq!(pending.lock().unwrap().len(), MAX_QUEUED_REQUESTS);
    ui_queue.clear();
    drain_queue(&pending, &mut ui_queue);
    assert_eq!(ui_queue.len(), MAX_QUEUED_REQUESTS);
    assert!(pending.lock().unwrap().is_empty());
    reader.get_ref().shutdown(Shutdown::Write).unwrap();
    worker.join().unwrap().unwrap();
}

/// Asserts a commit that writes the file and queues its reply wins the gate: the transport's
/// timeout branch consumes that reply and leaves the state `Replied`.
#[test]
fn a_committed_reply_wins_over_the_transport_timeout() {
    let dir = std::env::temp_dir().join(format!("figcraft-control-commit-{}", std::process::id()));
    let path = dir.join("shot.png");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let gate = RequestGate::new();
    let (tx, rx) = mpsc::channel();
    // The UI's final commit gets the gate first: the file and reply are both committed before the
    // transport's timeout branch can look at the gate.
    let committed = gate.commit(|| {
        std::fs::write(&path, b"png").unwrap();
        tx.send(ControlReply { id: json!(7), result: Ok(json!({"path": path.to_string_lossy()})) }).unwrap();
    });
    assert!(committed.is_some());
    assert!(path.is_file());
    let reply = after_timeout(&rx, &gate).expect("the queued commit is consumed");
    assert_eq!(reply.id, json!(7));
    assert!(reply.result.is_ok());
    assert_eq!(gate.state(), RequestState::Replied, "the timeout must not overwrite a commit");
    assert!(path.is_file(), "a winning commit keeps its file");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Asserts a zero-duration wait reports `Timeout` and cancels the open gate.
#[test]
fn a_timeout_cancels_an_open_request() {
    let gate = RequestGate::new();
    let (_tx, rx) = mpsc::channel::<ControlReply>();
    assert!(matches!(wait_for_reply(&rx, &gate, Duration::ZERO), Err(WaitFailure::Timeout)));
    assert_eq!(gate.state(), RequestState::Cancelled, "nothing may write after the timeout");
}

/// Asserts a closed reply channel reports `Disconnected` and leaves the gate `Open`, not
/// cancelled.
#[test]
fn a_disconnected_reply_channel_is_not_reported_as_a_timeout() {
    let gate = RequestGate::new();
    let (tx, rx) = mpsc::channel::<ControlReply>();
    drop(tx);
    assert!(matches!(wait_for_reply(&rx, &gate, Duration::from_secs(1)), Err(WaitFailure::Disconnected)));
    assert_eq!(gate.state(), RequestState::Open, "a closed app is not a cancel decision");
}

#[test]
fn pruning_an_expired_sender_reports_deadline_not_a_closed_window() {
    let gate = RequestGate::with_timeout(Duration::ZERO);
    let (tx, rx) = mpsc::channel::<ControlReply>();
    assert!(gate.is_cancelled());
    drop(tx);
    assert_eq!(wait_for_reply(&rx, &gate, Duration::from_secs(1)).err().unwrap(), WaitFailure::Timeout);
}

#[test]
fn pruning_an_explicitly_cancelled_sender_reports_no_application() {
    let gate = RequestGate::with_timeout(Duration::from_secs(60));
    gate.cancel_if_open();
    let (tx, rx) = mpsc::channel::<ControlReply>();
    drop(tx);
    let failure = wait_for_reply(&rx, &gate, Duration::from_secs(1)).err().unwrap();
    assert_eq!(failure, WaitFailure::Cancelled);
    let message = timeout_message(&failure, false, Duration::from_secs(60));
    assert!(message.contains("cancelled") && message.contains("not applied"));
    assert!(!message.contains("window closed") && !message.contains("timed out"));
}

/// #1673: a request the UI already started can't be cancelled any more. Its timeout says it is
/// still running and will take effect, not that the window is hidden; one that finished just as
/// the timeout fired gets its real reply.
#[test]
fn a_started_request_times_out_as_still_running() {
    let gate = RequestGate::new();
    assert!(gate.start(), "an open request starts");
    let (tx, rx) = mpsc::channel::<ControlReply>();
    let Err(failure) = wait_for_reply(&rx, &gate, Duration::ZERO) else { panic!("no reply was sent") };
    assert_eq!(failure, WaitFailure::StillRunning);
    assert_eq!(gate.state(), RequestState::Running, "the timeout doesn't cancel a running request");
    let message = timeout_message(&failure, false, Duration::from_secs(60));
    assert!(message.contains("will take effect") && !message.contains("window"), "{message}");
    tx.send(ControlReply { id: json!(3), result: Ok(json!({"done": true})) }).unwrap();
    assert_eq!(wait_for_reply(&rx, &gate, Duration::ZERO).unwrap().id, json!(3), "a reply that made it is the truth");
}

/// #1673: a request the transport gave up on before the UI started it never starts, and its
/// timeout says it wasn't applied; only requests that need a drawn frame blame the window.
#[test]
fn a_request_cancelled_before_it_started_never_runs() {
    let gate = RequestGate::new();
    let (_tx, rx) = mpsc::channel::<ControlReply>();
    let Err(failure) = wait_for_reply(&rx, &gate, Duration::ZERO) else { panic!("no reply was sent") };
    assert_eq!(failure, WaitFailure::Timeout);
    assert!(!gate.start(), "the UI must not run a cancelled request");
    assert_eq!(gate.state(), RequestState::Cancelled);
    let message = |needs_frame| timeout_message(&failure, needs_frame, Duration::from_secs(60));
    assert!(message(false).contains("not applied"));
    assert!(!message(false).contains("window on screen"));
    assert!(message(true).contains("window on screen"), "screenshots and input still need the window");
}

/// #1696: a request's `timeoutMs` sets its wait, capped; anything but a whole number leaves the
/// default.
#[test]
fn a_request_sets_its_own_timeout_within_the_cap() {
    let default = Duration::from_secs(60);
    let wait = |v: Value| request_timeout(&v, default);
    assert_eq!(wait(json!({"id": 1, "method": "engine.execute"})), default, "older clients keep the default");
    assert_eq!(wait(json!({"timeoutMs": 300_000})), Duration::from_secs(300));
    assert_eq!(wait(json!({"timeoutMs": 500})), Duration::from_millis(500), "a shorter wait too");
    assert_eq!(wait(json!({"timeoutMs": u64::MAX})), MAX_TIMEOUT, "never unbounded");
    for junk in [json!({"timeoutMs": "600000"}), json!({"timeoutMs": -1}), json!({"timeoutMs": 1.5}), json!({"timeoutMs": null})] {
        assert_eq!(wait(junk), default);
    }
}

/// #1696: over a connection, a raised `timeoutMs` outlasts the connection's default.
#[test]
fn a_raised_timeout_outlasts_the_default() {
    let (mut client, server) = pair();
    let pending = queue();
    let q = pending.clone();
    // Written first: the connection's 50 ms is also how long it waits for a line.
    client.write_all(b"{\"id\":1,\"method\":\"ui.inspect\",\"timeoutMs\":5000}\n").unwrap();
    let worker = std::thread::spawn(move || serve(server, q, context(), Duration::from_millis(50)));
    let mut reader = BufReader::new(client);
    let start = Instant::now();
    let r = loop {
        if let Some(r) = pending.lock().unwrap().pop() {
            break r;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    };
    // Well past the connection's 50 ms: the request's own wait still holds.
    std::thread::sleep(Duration::from_millis(200));
    r.reply.send(ControlReply { id: r.id, result: Ok(json!("late")) }).unwrap();
    assert_eq!(reply(&mut reader), json!({"id": 1, "ok": true, "result": "late"}));
    drop(reader);
    worker.join().unwrap().ok();
}

/// #1696: the timeout's message says how long the request waited, not always 60 s.
#[test]
fn a_timeout_names_the_wait_the_request_asked_for() {
    let (mut client, server) = pair();
    let worker = std::thread::spawn(move || serve(server, queue(), context(), Duration::from_secs(5)));
    client.write_all(b"{\"id\":2,\"method\":\"ui.inspect\",\"timeoutMs\":100}\n").unwrap();
    let mut reader = BufReader::new(client);
    let error = reply(&mut reader)["error"].as_str().unwrap().to_string();
    assert!(error.contains("after 1 s") && error.contains("not applied"), "{error}");
    reader.get_ref().shutdown(Shutdown::Write).unwrap();
    worker.join().unwrap().unwrap();
}

#[test]
fn idle_and_partial_frame_connections_expire() {
    for bytes in [b"".as_slice(), b"{\"id\":"] {
        let (mut client, server) = pair();
        let (done, finished) = mpsc::channel();
        let pending = queue();
        let q = pending.clone();
        let worker = std::thread::spawn(move || done.send(serve(server, q, context(), Duration::from_millis(50))));
        client.write_all(bytes).unwrap();
        let err = finished.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err();
        assert!(matches!(err.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut));
        assert!(pending.lock().unwrap().is_empty());
        worker.join().unwrap().unwrap();
    }
}

/// A failed accept is followed by a wait before the next one, so a listener that keeps failing
/// does not spin, and the connections around the failures still come through.
#[test]
fn a_failed_accept_waits_before_the_next() {
    let failed = || Err(std::io::Error::other("no file descriptors left"));
    let incoming = vec![failed(), Ok(1), failed(), failed(), Ok(2)];
    let mut waits = 0;
    let connections: Vec<i32> = accepted(incoming.into_iter(), || waits += 1).collect();
    assert_eq!(connections, [1, 2]);
    assert_eq!(waits, 3, "one wait per failed accept");
}

#[test]
fn connection_limit_rejects_excess_clients_and_reuses_finished_slots() {
    let pending = queue();
    let ctx = context();
    let mut workers = Vec::new();
    let mut clients = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        let (client, server) = pair();
        spawn_connection(
            server,
            pending.clone(),
            ctx.clone(),
            Arc::new(figcraft_engine::events::EventLog::default()),
            &mut workers,
            Duration::from_secs(5),
        );
        clients.push(client);
    }
    assert_eq!(workers.len(), MAX_CONNECTIONS);
    let (mut extra, server) = pair();
    spawn_connection(
        server,
        pending.clone(),
        ctx.clone(),
        Arc::new(figcraft_engine::events::EventLog::default()),
        &mut workers,
        Duration::from_secs(5),
    );
    assert_eq!(extra.read(&mut [0]).unwrap(), 0);
    assert_eq!(workers.len(), MAX_CONNECTIONS);
    for client in clients {
        client.shutdown(Shutdown::Both).unwrap();
    }
    let start = Instant::now();
    while workers.iter().any(|worker| !worker.is_finished()) {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    }
    let (mut client, server) = pair();
    spawn_connection(server, pending, ctx, Arc::new(figcraft_engine::events::EventLog::default()), &mut workers, Duration::from_secs(5));
    assert_eq!(workers.len(), 1);
    client.write_all(b"invalid\n").unwrap();
    assert_eq!(reply(&mut BufReader::new(client.try_clone().unwrap()))["ok"], false);
    // The server closes this connection after answering an invalid request, and shutting down
    // a socket whose peer has already closed is ENOTCONN on macOS and the BSDs where Linux
    // returns Ok. Closing the client is what the worker is waiting for, so the result is moot:
    // the other shutdowns in this file run while the peer is still open and stay checked.
    let _ = client.shutdown(Shutdown::Both);
    workers.pop().unwrap().join().unwrap();
}

#[test]
fn a_client_that_stops_reading_cannot_hold_a_worker() {
    let (mut client, server) = pair();
    let pending = queue();
    let q = pending.clone();
    let (done, finished) = mpsc::channel();
    let worker = std::thread::spawn(move || done.send(serve(server, q, context(), Duration::from_millis(100))));
    client.write_all(b"{\"id\":1,\"method\":\"document.inspect\"}\n").unwrap();
    let start = Instant::now();
    let r = loop {
        if let Some(r) = pending.lock().unwrap().pop() {
            break r;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    };
    r.reply.send(figcraft_ui_egui::ControlReply { id: r.id, result: Ok(json!("x".repeat(8 * 1024 * 1024))) }).unwrap();
    let err = finished.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err();
    assert!(matches!(err.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut));
    worker.join().unwrap().unwrap();
}

#[test]
fn events_poll_is_answered_without_the_ui_queue() {
    let (mut client, server) = pair();
    let pending = queue();
    let q = pending.clone();
    let log = Arc::new(figcraft_engine::events::EventLog::default());
    log.push("comment.added", "Doc", None, json!({"commentId": 7}));
    let l = log.clone();
    let worker = std::thread::spawn(move || serve_with_events(server, q, context(), l, Duration::from_secs(5)));
    client.write_all(b"{\"id\":1,\"method\":\"events.poll\",\"params\":{\"after\":0,\"timeoutMs\":0}}\n").unwrap();
    let mut reader = BufReader::new(client);
    let r = reply(&mut reader);
    assert_eq!(r["id"], 1);
    assert_eq!(r["result"]["events"].as_array().unwrap().len(), 1);
    assert_eq!(r["result"]["events"][0]["kind"], "comment.added");
    assert_eq!(r["result"]["events"][0]["data"]["commentId"], 7);
    assert_eq!(r["result"]["cursor"], 1);
    assert!(pending.lock().unwrap().is_empty());
    reader.get_ref().shutdown(Shutdown::Write).unwrap();
    worker.join().unwrap().unwrap();
}

#[test]
fn a_comment_added_in_the_session_is_pollable() {
    let mut session = figcraft_engine::Session::new();
    let log = session.events.clone();
    let id = session.execute("comment.add", &json!({"x": 10, "y": 20, "text": "Fix the padding"})).unwrap()["id"].clone();
    let r = event_poll(&log, &json!({"after": 0, "timeoutMs": 0})).unwrap();
    assert_eq!(r["events"].as_array().unwrap().len(), 1);
    assert_eq!(r["events"][0]["kind"], "comment.added");
    assert_eq!(r["events"][0]["data"]["commentId"], id);
}

#[test]
fn a_limit_sized_frame_is_accepted_at_eof() {
    let (mut client, server) = pair();
    let pending = queue();
    let q = pending.clone();
    let worker = std::thread::spawn(move || serve(server, q, context(), Duration::from_secs(5)));
    let message = b"{\"id\":1,\"method\":\"ui.inspect\"}";
    client.write_all(message).unwrap();
    std::io::copy(&mut std::io::repeat(b' ').take((MAX_LINE - message.len()) as u64), &mut client).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let start = Instant::now();
    let r = loop {
        if let Some(r) = pending.lock().unwrap().pop() {
            break r;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(r.id, 1);
    assert_eq!(r.method, "ui.inspect");
    r.reply.send(figcraft_ui_egui::ControlReply { id: r.id, result: Ok(json!({})) }).unwrap();
    assert_eq!(reply(&mut BufReader::new(client)), json!({"id": 1, "ok": true, "result": {}}));
    worker.join().unwrap().unwrap();
}

/// Asserts expired requests stop occupying the UI queue cap, a later request is admitted
/// and answered, and a window that finally draws discards its expired command before
/// dispatch.
#[test]
fn timed_out_requests_release_a_full_deferred_ui_queue() {
    use figcraft_ui_egui::{FigApp, NoServices, control, windows};

    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    let mut app = FigApp::new(figcraft_engine::Session::new(), Box::new(NoServices));
    let window = windows::open(&mut app, &ctx, Some(0), None, None).unwrap();
    let pending = queue();
    let mut clients = Vec::new();
    let mut workers = Vec::new();
    for id in 0..MAX_QUEUED_REQUESTS {
        let (mut client, server) = pair();
        let q = pending.clone();
        workers.push(std::thread::spawn(move || serve(server, q, context(), Duration::from_secs(1))));
        writeln!(client, "{}", json!({"id": id, "method": "engine.execute", "params": {"window": window, "command": "file.new"}})).unwrap();
        clients.push(BufReader::new(client));
    }
    let start = Instant::now();
    while pending.lock().unwrap().len() < MAX_QUEUED_REQUESTS {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    }
    drain_queue(&pending, &mut app.control_queue);
    control::drain(&mut app, &ctx);
    assert_eq!(app.control_queue.len(), MAX_QUEUED_REQUESTS, "the other window has not drawn");
    for client in &mut clients {
        assert!(reply(client)["error"].as_str().unwrap().contains("timed out"));
        client.get_ref().shutdown(Shutdown::Write).unwrap();
    }
    assert!(app.control_queue.iter().all(ControlRequest::is_cancelled));
    let expired = app.control_queue.pop().unwrap();
    // Include expired requests in the socket queue, too: admission must reclaim their slots.
    pending.lock().unwrap().extend((0..MAX_QUEUED_REQUESTS).map(|_| {
        let r = request();
        r.state.cancel_if_open();
        r
    }));
    let (mut client, server) = pair();
    let q = pending.clone();
    workers.push(std::thread::spawn(move || serve(server, q, context(), Duration::from_secs(5))));
    client.write_all(b"{\"id\":100,\"method\":\"window.list\",\"params\":{\"window\":0}}\n").unwrap();
    let start = Instant::now();
    while !pending.lock().unwrap().iter().any(|r| r.id == 100) {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    }
    drain_queue(&pending, &mut app.control_queue);
    assert_eq!(app.control_queue.len(), 1, "expired UI requests no longer occupy the cap");
    control::drain(&mut app, &ctx);
    let mut client = BufReader::new(client);
    assert_eq!(reply(&mut client)["ok"], true);
    client.get_ref().shutdown(Shutdown::Write).unwrap();
    assert!(app.control_queue.is_empty());
    // A window that finally draws must discard its expired command before dispatch, too.
    let documents = app.session.docs.len();
    app.windows.current = window;
    app.control_queue.push(expired);
    control::drain(&mut app, &ctx);
    assert!(app.control_queue.is_empty());
    assert_eq!(app.session.docs.len(), documents);
    for worker in workers {
        worker.join().unwrap().unwrap();
    }
}

#[test]
fn events_poll_rejects_an_unknown_topic_without_the_ui_queue() {
    let (client, server) = pair();
    let pending = queue();
    let q = pending.clone();
    let log = Arc::new(figcraft_engine::events::EventLog::default());
    let l = log.clone();
    let worker = std::thread::spawn(move || serve_with_events(server, q, context(), l, Duration::from_secs(5)));
    let mut reader = BufReader::new(client);
    reader.get_mut().write_all(b"{\"id\":1,\"method\":\"events.poll\",\"params\":{\"topics\":[\"nope\"],\"timeoutMs\":0}}\n").unwrap();
    let r = reply(&mut reader);
    assert_eq!(r["id"], 1);
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().is_some_and(|e| e.contains("unknown topic")), "{r}");
    assert!(pending.lock().unwrap().is_empty());
    reader.get_ref().shutdown(Shutdown::Write).unwrap();
    worker.join().unwrap().unwrap();
}

#[test]
fn events_poll_reports_reset_for_a_cursor_ahead_of_the_log() {
    let (client, server) = pair();
    let log = Arc::new(figcraft_engine::events::EventLog::default());
    log.push("comment.added", "Doc", None, json!({"commentId": 1}));
    let q = queue();
    let l = log.clone();
    let worker = std::thread::spawn(move || serve_with_events(server, q, context(), l, Duration::from_secs(5)));
    let mut reader = BufReader::new(client);
    reader.get_mut().write_all(b"{\"id\":1,\"method\":\"events.poll\",\"params\":{\"after\":2,\"timeoutMs\":0}}\n").unwrap();
    let r = reply(&mut reader);
    assert_eq!(r["result"]["reset"], true, "{r}");
    assert!(r["result"]["events"].as_array().unwrap().is_empty(), "{r}");
    assert_eq!(r["result"]["cursor"], 1);
    reader.get_ref().shutdown(Shutdown::Write).unwrap();
    worker.join().unwrap().unwrap();
}

#[test]
fn native_auth_admission_is_required_before_commands_and_rebind_is_transactional() {
    let dir = std::env::temp_dir().join(format!("figcraft-auth-server-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let reserved = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = reserved.local_addr().unwrap().port();
    drop(reserved);
    let queue: Queue = Default::default();
    let handle = ControlHandle::new();
    handle.set_data_dir(Some(dir.clone()));
    handle.bind_deps(queue.clone(), Default::default(), Arc::new(EventLog::default()));
    handle.apply(true, port, Duration::ZERO);
    assert_eq!(handle.status().0, port);
    let token_path = dir.join(format!("control-{port}.token"));
    let token = std::fs::read_to_string(&token_path).unwrap();
    for first in [json!({"method":"engine.execute","params":{"command":"file.new"}}), json!({"method":"auth","token":"wrong"})] {
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        writeln!(client, "{first}").unwrap();
        let mut response = String::new();
        BufReader::new(client).read_line(&mut response).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&response).unwrap()["ok"], false);
        assert!(queue.lock().unwrap().is_empty());
    }
    let busy = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let busy_port = busy.local_addr().unwrap().port();
    handle.apply(true, busy_port, Duration::ZERO);
    assert_eq!(handle.status().0, port);
    assert_eq!(std::fs::read_to_string(&token_path).unwrap(), token);
    assert!(!dir.join(format!("control-{busy_port}.token")).exists());
    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    writeln!(client, "{}", json!({"method":"auth","token":token})).unwrap();
    let mut reader = BufReader::new(client.try_clone().unwrap());
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&response).unwrap()["result"]["authenticated"], true);
    writeln!(client, "{}", json!({"id":71,"method":"engine.commands","params":{}})).unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(2);
    let request = loop {
        if let Some(r) = queue.lock().unwrap().pop() {
            break r;
        }
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(request.method, "engine.commands");
    request.reply.send(ControlReply { id: request.id, result: Ok(json!({"accepted":true})) }).unwrap();
    response.clear();
    reader.read_line(&mut response).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&response).unwrap()["result"]["accepted"], true);
    handle.apply(false, 0, Duration::ZERO);
    assert_eq!(handle.status().0, 0);
    assert!(!token_path.exists());
    assert!(queue.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

// Review-owned append-only fixtures for apps/figcraft/src/control_server_tests.rs.
// Actual production ControlHandle + TCP + RequestGate; no GUI or secret output.
mod final_auth_lifecycle_tests {
    use super::*;

    struct Owned {
        handle: ControlHandle,
        queue: Queue,
        dir: std::path::PathBuf,
        port: u16,
    }
    impl Owned {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = (0..1024)
                .find_map(|_| {
                    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
                    let dir = std::env::temp_dir().join(format!("figcraft-final-control-{}-{sequence}", std::process::id()));
                    std::fs::create_dir(&dir).ok().map(|()| dir)
                })
                .expect("a fresh owned fixture directory");
            let queue = queue();
            let handle = ControlHandle::new();
            handle.set_data_dir(Some(dir.clone()));
            handle.bind_deps(queue.clone(), context(), Arc::new(EventLog::default()));
            let port = unused_port();
            handle.apply(true, port, Duration::ZERO);
            assert_eq!(handle.status().0, port);
            Self { handle, queue, dir, port }
        }
        fn token_path(&self, port: u16) -> std::path::PathBuf {
            self.dir.join(format!("control-{port}.token"))
        }
        fn token(&self, port: u16) -> String {
            let value = std::fs::read_to_string(self.token_path(port)).unwrap();
            assert!(value.len() == 44, "private credential has the bounded protocol shape");
            value
        }
        fn authenticated(&self) -> BufReader<TcpStream> {
            let mut reader = connection(self.port);
            let token = self.token(self.port);
            writeln!(reader.get_mut(), "{}", json!({"method":"auth","token":token})).unwrap();
            let result = reply(&mut reader);
            assert_eq!(result["result"]["authenticated"], true);
            reader
        }
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            self.handle.apply(false, 0, Duration::ZERO);
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    fn unused_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port()
    }
    fn connection(port: u16) -> BufReader<TcpStream> {
        let socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        socket.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
        BufReader::new(socket)
    }
    fn closed_without_dispatch(reader: &mut BufReader<TcpStream>, queue: &Queue) {
        let _ = writeln!(reader.get_mut(), "{}", json!({"id":987,"method":"engine.commands","params":{}}));
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {}
            Err(error) if matches!(error.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe) => {}
            other => panic!("retired connection remained admitted: {:?}", other.map(|_| line.len())),
        }
        assert!(queue.lock().unwrap().is_empty(), "retired connection may not admit a new command");
    }
    fn pending(fixture: &Owned, deferred: bool) -> (BufReader<TcpStream>, RequestGate, Vec<ControlRequest>) {
        let mut reader = fixture.authenticated();
        writeln!(reader.get_mut(), "{}", json!({"id":991,"method":"engine.execute","params":{"command":"file.new","params":{}},"timeoutMs":5000}))
            .unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        while fixture.queue.lock().unwrap().is_empty() {
            assert!(Instant::now() < until, "the actual authenticated request reaches the production queue");
            std::thread::sleep(Duration::from_millis(1));
        }
        let gate = fixture.queue.lock().unwrap()[0].state.clone();
        let mut held = Vec::new();
        if deferred {
            drain_queue(&fixture.queue, &mut held);
            assert_eq!(held.len(), 1, "exercise a request already transferred to the deferred UI queue");
        }
        assert_eq!(gate.state(), RequestState::Open);
        (reader, gate, held)
    }
    fn retire_pending(rebind: bool, deferred: bool) {
        let fixture = Owned::new();
        let (reader, gate, _held) = pending(&fixture, deferred);
        let began = Instant::now();
        if rebind {
            let port = unused_port();
            fixture.handle.apply(true, port, Duration::ZERO);
            assert_eq!(fixture.handle.status().0, port);
        } else {
            fixture.handle.apply(false, 0, Duration::ZERO);
            assert_eq!(fixture.handle.status().0, 0);
        }
        assert!(began.elapsed() < Duration::from_secs(2), "the fixture must not pass through the five-second request timeout");
        assert_eq!(gate.state(), RequestState::Cancelled, "retiring the listener lease cancels admitted work that has not started");
        assert!(!gate.start(), "a deferred file.new cannot execute after its control lease was retired");
        drop(reader);
    }

    #[test]
    fn disabling_cancels_authenticated_work_still_waiting_in_native_queue() {
        retire_pending(false, false);
    }
    #[test]
    fn disabling_cancels_authenticated_work_already_deferred_to_the_ui() {
        retire_pending(false, true);
    }
    #[test]
    fn successful_rebind_cancels_authenticated_work_still_waiting_in_native_queue() {
        retire_pending(true, false);
    }
    #[test]
    fn successful_rebind_cancels_authenticated_work_already_deferred_to_the_ui() {
        retire_pending(true, true);
    }

    #[test]
    fn retirement_does_not_cancel_a_request_the_ui_already_started() {
        let fixture = Owned::new();
        let (mut reader, gate, _held) = pending(&fixture, false);
        let request = fixture.queue.lock().unwrap().pop().unwrap();
        assert!(gate.start());
        fixture.handle.apply(false, 0, Duration::ZERO);
        assert_eq!(gate.state(), RequestState::Running, "already-started work retains the existing truthful completion contract");
        // Started engine commands deliver their reply directly; commit() is the separate
        // Open-gate path used for deferred screenshot/file effects.
        request.reply.send(ControlReply { id: request.id, result: Ok(json!({"completed":true})) }).unwrap();
        assert_eq!(reply(&mut reader)["result"]["completed"], true);
    }

    #[test]
    fn bind_and_publication_failures_preserve_old_credentials_then_success_retires_old_connections() {
        let fixture = Owned::new();
        let old_token = fixture.token(fixture.port);
        let mut old_authenticated = fixture.authenticated();
        let mut old_unauthenticated = connection(fixture.port);
        let busy = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let busy_port = busy.local_addr().unwrap().port();
        fixture.handle.apply(true, busy_port, Duration::ZERO);
        assert_eq!(fixture.handle.status().0, fixture.port);
        assert!(fixture.token(fixture.port) == old_token, "failed bind preserves the existing private credential");
        assert!(!fixture.token_path(busy_port).exists());

        let next_port = unused_port();
        let forbidden_token = fixture.token_path(next_port);
        std::fs::create_dir(&forbidden_token).unwrap();
        fixture.handle.apply(true, next_port, Duration::ZERO);
        assert_eq!(fixture.handle.status().0, fixture.port);
        assert!(fixture.handle.status().2.is_some());
        assert!(fixture.token(fixture.port) == old_token, "publication refusal preserves old private credential");
        assert!(forbidden_token.is_dir(), "publication failure leaves the owned sentinel intact");
        let released = TcpListener::bind(("127.0.0.1", next_port)).expect("failed credential publication releases the provisional listener");
        drop(released);
        std::fs::remove_dir(&forbidden_token).unwrap();

        fixture.handle.apply(true, next_port, Duration::ZERO);
        assert_eq!(fixture.handle.status().0, next_port);
        assert!(!fixture.token_path(fixture.port).exists());
        let fresh_token = fixture.token(next_port);
        assert!(fresh_token != old_token, "successful rebind publishes a fresh private credential");
        let mut stale = connection(next_port);
        writeln!(stale.get_mut(), "{}", json!({"method":"auth","token":old_token})).unwrap();
        assert_eq!(reply(&mut stale)["ok"], false, "old credential is refused at the replacement listener");
        closed_without_dispatch(&mut old_authenticated, &fixture.queue);
        // Wake a first-line read that was pending when its lease retired.
        let _ = writeln!(old_unauthenticated.get_mut(), "{}", json!({"method":"auth","token":fresh_token}));
        let mut refused = String::new();
        match old_unauthenticated.read_line(&mut refused) {
            Ok(0) => {}
            Err(error) if matches!(error.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe) => {}
            other => panic!("retired first-line connection remained open: {:?}", other.map(|_| refused.len())),
        }
        assert!(fixture.queue.lock().unwrap().is_empty());
        let mut fresh = connection(next_port);
        writeln!(fresh.get_mut(), "{}", json!({"method":"auth","token":fixture.token(next_port)})).unwrap();
        assert_eq!(reply(&mut fresh)["result"]["authenticated"], true);
        fixture.handle.apply(false, 0, Duration::ZERO);
        assert_eq!(fixture.handle.status().0, 0);
        assert!(!fixture.token_path(next_port).exists());
        closed_without_dispatch(&mut fresh, &fixture.queue);
    }
}
