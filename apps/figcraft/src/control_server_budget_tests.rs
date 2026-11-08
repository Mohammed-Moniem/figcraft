#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::control_budget::Budget;
use std::io::BufRead;
use std::net::Shutdown;
use std::thread::JoinHandle;
use std::time::Instant;

struct Connection {
    peer: BufReader<TcpStream>,
    worker: Option<JoinHandle<std::io::Result<()>>>,
}

impl Connection {
    fn new(budget: Arc<Budget>, pending: Queue, timeout: Duration) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        peer.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
        let accepted = listener.accept().unwrap().0;
        let worker = std::thread::spawn(move || {
            serve_budgeted(accepted, pending, Arc::new(Mutex::new(None)), Arc::new(EventLog::default()), timeout, None, budget)
        });
        Self { peer: BufReader::new(peer), worker: Some(worker) }
    }

    fn write(&mut self, bytes: &[u8]) {
        self.peer.get_mut().write_all(bytes).unwrap();
    }

    fn reply(&mut self) -> Value {
        let mut line = String::new();
        assert!(self.peer.read_line(&mut line).unwrap() > 0, "a complete protocol response is required");
        assert!(line.len() <= 8192, "fixtures receive bounded synthetic responses");
        assert!(line.ends_with('\n'));
        serde_json::from_str(&line).unwrap()
    }

    fn finish(&mut self) -> std::io::Result<()> {
        // The malformed/EOF fixture may already have shut this half, and a peer can close
        // after its complete error reply. Cleanup must still observe the worker result.
        let _ = self.peer.get_ref().shutdown(Shutdown::Write);
        self.worker.take().unwrap().join().unwrap()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.peer.get_ref().shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn pending() -> Queue {
    Arc::new(Mutex::new(Vec::new()))
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while !condition() {
        assert!(Instant::now() < deadline, "bounded fixture observation did not occur");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn take_request(queue: &Queue) -> ControlRequest {
    wait_until(|| !queue.lock().unwrap().is_empty());
    queue.lock().unwrap().remove(0)
}

fn assert_recovery(budget: &Arc<Budget>) {
    let queue = pending();
    let mut connection = Connection::new(budget.clone(), queue.clone(), Duration::from_secs(2));
    connection.write(b"{\"id\":1,\"method\":\"x\"}\n");
    let request = take_request(&queue);
    assert!(request.state.start());
    request.reply.send(ControlReply { id: request.id.clone(), result: Ok(json!({"recovered": true})) }).unwrap();
    drop(request);
    assert_eq!(connection.reply(), json!({"id":1,"ok":true,"result":{"recovered":true}}));
    connection.finish().unwrap();
    assert_eq!(budget.used(), 0);
}

#[test]
fn concurrent_partial_tcp_frames_share_raw_capacity_before_growth_and_recover() {
    let budget = Arc::new(Budget::new(32 * 1024));
    let queue = pending();
    let timeout = Duration::from_secs(2);
    let mut first = Connection::new(budget.clone(), queue.clone(), timeout);
    first.write(&vec![b' '; 8192]);
    wait_until(|| budget.used() == 8192);
    let mut second = Connection::new(budget.clone(), queue.clone(), timeout);
    second.write(&vec![b' '; 8192]);
    wait_until(|| budget.used() == 16 * 1024);
    let mut refused = Connection::new(budget.clone(), queue.clone(), timeout);
    refused.write(&vec![b' '; 8192]);
    wait_until(|| budget.used() == 24 * 1024);
    // Its old8KiB remains live while a12KiB replacement would be allocated. The pool must
    // refuse BEFORE that replacement, although the eventual retained capacities would fit.
    refused.write(b" ");
    let response = refused.reply();
    assert_eq!(response["ok"], false);
    assert!(response["error"].as_str().unwrap().contains("memory budget"));
    refused.finish().unwrap();
    assert_eq!(budget.used(), 16 * 1024);
    assert_eq!(budget.parses.load(Ordering::Relaxed), 0, "partial raw frames have never reached serde");
    assert!(queue.lock().unwrap().is_empty());
    first.finish().unwrap();
    second.finish().unwrap();
    assert_eq!(budget.used(), 0);
    assert_recovery(&budget);
}

#[test]
fn invalid_json_utf8_and_truncated_eof_drop_every_frame_owner() {
    let budget = Arc::new(Budget::new(128 * 1024));
    for bytes in [b"{\"params\":[true,}\n".as_slice(), b"\"\xff\"\n", b"{\"id\":"] {
        let queue = pending();
        let mut connection = Connection::new(budget.clone(), queue.clone(), Duration::from_secs(1));
        connection.write(bytes);
        if !bytes.ends_with(b"\n") {
            connection.peer.get_ref().shutdown(Shutdown::Write).unwrap();
        }
        let response = connection.reply();
        assert_eq!(response["ok"], false);
        assert!(response["error"].as_str().unwrap().starts_with("bad JSON:"));
        connection.finish().unwrap();
        assert!(queue.lock().unwrap().is_empty());
        assert_eq!(budget.used(), 0, "syntax/UTF8/EOF errors cannot pin raw or parse admission");
    }
    assert_recovery(&budget);
}

#[test]
fn partial_frame_deadline_and_empty_eof_return_raw_admission() {
    let budget = Arc::new(Budget::new(32 * 1024));
    let queue = pending();
    let mut connection = Connection::new(budget.clone(), queue.clone(), Duration::from_millis(80));
    connection.write(b"{\"id\":");
    wait_until(|| budget.used() > 0);
    let started = Instant::now();
    let result = connection.worker.take().unwrap().join().unwrap();
    if let Err(error) = result {
        assert!(matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut), "unexpected deadline error: {error}");
    }
    assert!(started.elapsed() < Duration::from_secs(1), "unfinished frame ownership must expire within the bounded read deadline");
    assert_eq!(budget.used(), 0);
    assert!(queue.lock().unwrap().is_empty());
    let mut empty = Connection::new(budget.clone(), queue, Duration::from_secs(1));
    empty.finish().unwrap();
    assert_eq!(budget.used(), 0);
    assert_recovery(&budget);
}

#[test]
fn trickling_tcp_input_cannot_renew_the_absolute_frame_deadline() {
    let budget = Arc::new(Budget::new(32 * 1024));
    let queue = pending();
    let mut connection = Connection::new(budget.clone(), queue.clone(), Duration::from_millis(100));
    let mut trickle = connection.peer.get_ref().try_clone().unwrap();
    let writer = std::thread::spawn(move || {
        for _ in 0..30 {
            let _ = trickle.write_all(b" ");
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    let started = Instant::now();
    let outcome = connection.worker.take().unwrap().join().unwrap();
    assert!(started.elapsed() < Duration::from_millis(700), "a continuously trickling peer must expire before its byte sequence finishes");
    if let Err(error) = outcome {
        assert!(matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut));
    }
    assert!(!writer.is_finished(), "bytes were still being offered when the frame deadline expired");
    writer.join().unwrap();
    assert_eq!(budget.used(), 0);
    assert_eq!(budget.parses.load(Ordering::Relaxed), 0);
    assert!(queue.lock().unwrap().is_empty());
    assert_recovery(&budget);
}

#[test]
fn full_queue_refusal_releases_its_reservation_without_disturbing_existing_entries() {
    let budget = Arc::new(Budget::new(32 * 1024));
    let queue = pending();
    for id in 0..MAX_QUEUED_REQUESTS {
        queue.lock().unwrap().push(ControlRequest {
            id: json!(id),
            method: "held".into(),
            params: json!({}),
            reply: std::sync::mpsc::channel().0,
            state: RequestGate::new(),
        });
    }
    let mut connection = Connection::new(budget.clone(), queue.clone(), Duration::from_secs(1));
    connection.write(b"{\"id\":1,\"method\":\"x\"}\n");
    assert_eq!(connection.reply()["error"], "control request queue is full");
    connection.finish().unwrap();
    assert_eq!(queue.lock().unwrap().len(), MAX_QUEUED_REQUESTS);
    assert_eq!(budget.used(), 0);
    queue.lock().unwrap().clear();
    assert_recovery(&budget);
}

#[test]
fn cancelled_deferred_request_keeps_budget_until_payload_is_pruned() {
    let budget = Arc::new(Budget::new(96 * 1024));
    let queue = pending();
    let mut connection = Connection::new(budget.clone(), queue.clone(), Duration::from_millis(300));
    connection.write(b"{\"id\":1,\"method\":\"x\",\"params\":{\"text\":\"held\"}}\n");
    let request = take_request(&queue);
    let mut deferred = vec![request];
    assert_eq!(connection.reply()["ok"], false);
    connection.finish().unwrap();
    assert!(deferred[0].is_cancelled());
    assert!(budget.used() > 0, "a cancelled request still owns its queued parameters");
    drain_queue(&queue, &mut deferred);
    assert!(deferred.is_empty());
    assert_eq!(budget.used(), 0);
    assert_recovery(&budget);
}

#[test]
fn cancelled_native_queue_cannot_block_future_memory_admission() {
    let budget = Arc::new(Budget::new(64 * 1024));
    let queue = pending();
    let timeout = Duration::from_millis(500);
    let mut first = Connection::new(budget.clone(), queue.clone(), timeout);
    let mut second = Connection::new(budget.clone(), queue.clone(), timeout);
    first.write(b"{\"id\":1,\"method\":\"x\"}\n");
    second.write(b"{\"id\":2,\"method\":\"x\"}\n");
    wait_until(|| queue.lock().unwrap().len() == 2);
    assert_eq!(first.reply()["ok"], false);
    assert_eq!(second.reply()["ok"], false);
    first.finish().unwrap();
    second.finish().unwrap();
    assert_eq!(
        budget.used(),
        0,
        "cancelled native queue must release admission without a UI tick or another successful parse; retained requests={}",
        queue.lock().unwrap().len()
    );
    assert!(queue.lock().unwrap().is_empty());
    assert_eq!(budget.parses.load(Ordering::Relaxed), 2);
    assert_recovery(&budget);
    assert_eq!(budget.parses.load(Ordering::Relaxed), 3, "fresh request recovers after both transport timeouts");
}

#[test]
fn running_timeout_and_peer_close_cannot_release_still_owned_parameters() {
    let budget = Arc::new(Budget::new(96 * 1024));
    let queue = pending();
    let mut connection = Connection::new(budget.clone(), queue.clone(), Duration::from_millis(300));
    connection.write(b"{\"id\":1,\"method\":\"x\",\"params\":{\"text\":\"held\"}}\n");
    let request = take_request(&queue);
    assert!(request.state.start());
    let response = connection.reply();
    assert!(response["error"].as_str().unwrap().contains("still running"));
    connection.finish().unwrap();
    assert_eq!(request.state.cancel_if_open(), RequestState::Running);
    assert!(budget.used() > 0, "worker EOF cannot release a request which the UI still owns");
    drop(request);
    assert_eq!(budget.used(), 0);
    assert_recovery(&budget);
}

#[test]
fn large_completed_frame_is_dropped_before_the_same_connection_waits_for_another() {
    let budget = Arc::new(Budget::new(256 * 1024));
    let queue = pending();
    let mut connection = Connection::new(budget.clone(), queue.clone(), Duration::from_secs(2));
    let frame = format!("{{\"id\":1,\"method\":\"x\",\"params\":{{\"text\":\"{}\"}}}}\n", "x".repeat(8192));
    connection.write(frame.as_bytes());
    let request = take_request(&queue);
    assert_eq!(request.params["text"].as_str().unwrap().len(), 8192);
    request.reply.send(ControlReply { id: request.id.clone(), result: Ok(Value::Null) }).unwrap();
    drop(request);
    assert_eq!(connection.reply()["ok"], true);
    wait_until(|| budget.used() == 0);
    connection.write(b"{\"id\":2,\"method\":\"x\"}\n");
    let request = take_request(&queue);
    request.reply.send(ControlReply { id: request.id.clone(), result: Ok(Value::Null) }).unwrap();
    drop(request);
    assert_eq!(connection.reply()["id"], 2);
    connection.finish().unwrap();
    assert_eq!(budget.used(), 0);
}

#[test]
fn events_poll_bypass_of_ui_queue_still_holds_global_parse_admission() {
    let budget = Arc::new(Budget::new(64 * 1024));
    let queue = pending();
    let mut poll = Connection::new(budget.clone(), queue.clone(), Duration::from_secs(2));
    poll.write(b"{\"id\":1,\"method\":\"events.poll\",\"params\":{\"timeoutMs\":300}}\n");
    wait_until(|| budget.parses.load(Ordering::Relaxed) == 1);
    assert!(budget.used() > 0);
    assert!(queue.lock().unwrap().is_empty(), "event polling has no UI request owner");
    let mut blocked = Connection::new(budget.clone(), queue.clone(), Duration::from_secs(1));
    blocked.write(b"{\"id\":2,\"method\":\"x\"}\n");
    let response = blocked.reply();
    assert_eq!(response["ok"], false);
    assert!(response["error"].as_str().unwrap().contains("memory budget"));
    blocked.finish().unwrap();
    assert_eq!(budget.parses.load(Ordering::Relaxed), 1, "refused concurrent request never reaches serde");
    assert_eq!(poll.reply()["ok"], true);
    poll.finish().unwrap();
    assert_eq!(budget.used(), 0);
    assert_recovery(&budget);
}
