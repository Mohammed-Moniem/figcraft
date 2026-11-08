//! Actual authenticated TCP sessions with a manually advanced monotonic lease clock.
//! Frame I/O and request completion remain real; no sleep extends or expires the lease.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use std::io::{BufRead, Read};
use std::net::Shutdown;
use std::sync::mpsc;

const EXPIRED: &str = "control session lifetime exceeded; reconnect to continue";

struct Credentials {
    lease: Arc<crate::control_auth::Lease>,
    directory: std::path::PathBuf,
}

impl Credentials {
    fn new() -> Self {
        static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
        let directory = loop {
            let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            assert!(sequence < 1024, "bounded owned directory allocation");
            let path = std::env::temp_dir().join(format!("figcraft-control-session-{}-{sequence}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => break path,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("owned fixture directory creation failed: {error}"),
            }
        };
        let lease = Arc::new(crate::control_auth::Lease::create(&directory, 0).unwrap());
        Self { lease, directory }
    }

    fn authenticate(&self, reader: &mut BufReader<TcpStream>) {
        let token = std::fs::read_to_string(self.directory.join("control-0.token")).unwrap();
        writeln!(reader.get_mut(), "{}", json!({"method":"auth","token":token})).unwrap();
        assert_eq!(reply(reader), json!({"id":null,"ok":true,"result":{"authenticated":true,"protocol":1}}));
    }
}

impl Drop for Credentials {
    fn drop(&mut self) {
        self.lease.retire();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn policy(clock: &Arc<AtomicU64>) -> ConnectionPolicy {
    let mut policy = ConnectionPolicy::new(Duration::from_secs(2));
    policy.clock = Some((clock.clone(), clock.load(Ordering::Acquire)));
    policy
}

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    client.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
    (client, listener.accept().unwrap().0)
}

fn reply(reader: &mut BufReader<TcpStream>) -> Value {
    let mut line = String::new();
    assert!(reader.read_line(&mut line).unwrap() > 0);
    serde_json::from_str(&line).unwrap()
}

fn poll(reader: &mut BufReader<TcpStream>, id: u64) {
    writeln!(reader.get_mut(), "{}", json!({"id":id,"method":"events.poll","params":{"timeoutMs":0}})).unwrap();
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(2);
    while !condition() {
        assert!(Instant::now() < end, "bounded fixture observation did not arrive");
        std::thread::yield_now();
    }
}

struct Connection {
    reader: BufReader<TcpStream>,
    pending: Queue,
    budget: Arc<crate::control_budget::Budget>,
    worker: Option<std::thread::JoinHandle<std::io::Result<()>>>,
}

impl Connection {
    fn new(credentials: &Credentials, clock: &Arc<AtomicU64>) -> Self {
        let (client, server) = pair();
        let pending = Arc::new(Mutex::new(Vec::new()));
        let budget = Arc::new(crate::control_budget::Budget::new(64 * 1024 * 1024));
        let (queue, budget_copy, lease, policy) = (pending.clone(), budget.clone(), credentials.lease.clone(), policy(clock));
        let worker = std::thread::spawn(move || {
            serve_with_policy(server, queue, Arc::new(Mutex::new(None)), Arc::new(EventLog::default()), policy, Some(lease), budget_copy)
        });
        let mut connection = Self { reader: BufReader::new(client), pending, budget, worker: Some(worker) };
        credentials.authenticate(&mut connection.reader);
        connection
    }

    fn assert_expired(&mut self) {
        assert_eq!(reply(&mut self.reader), json!({"id":null,"ok":false,"error":EXPIRED}));
        let mut byte = [0];
        assert_eq!(self.reader.read(&mut byte).unwrap(), 0, "one complete bounded refusal precedes FIN");
        assert!(self.pending.lock().unwrap().is_empty());
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.reader.get_ref().shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Accepting {
    worker: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    sockets: Vec<TcpStream>,
}

impl Drop for Accepting {
    fn drop(&mut self) {
        for socket in &self.sockets {
            let _ = socket.shutdown(Shutdown::Both);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[test]
fn authenticated_keepalive_and_whitespace_cannot_renew_the_session_lease() {
    let credentials = Credentials::new();
    let clock = Arc::new(AtomicU64::new(0));
    let mut connection = Connection::new(&credentials, &clock);
    for (id, millis) in [0, 100_000, 300_000, 500_000].into_iter().enumerate() {
        clock.store(millis, Ordering::Release);
        connection.reader.get_mut().write_all(b" \t\n").unwrap();
        poll(&mut connection.reader, id as u64);
        let result = reply(&mut connection.reader);
        assert_eq!(result["id"], id);
        assert_eq!(result["ok"], true, "fresh requests within the lease still work");
    }
    clock.store(600_000, Ordering::Release);
    connection.reader.get_mut().write_all(b" \t\n").unwrap();
    poll(&mut connection.reader, 99);
    connection.assert_expired();
}

#[test]
fn a_frame_started_before_expiry_is_not_parsed_or_admitted_after_expiry() {
    let credentials = Credentials::new();
    let clock = Arc::new(AtomicU64::new(0));
    let mut connection = Connection::new(&credentials, &clock);
    connection.reader.get_mut().write_all(b"{\"id\":1,\"method\":\"events.poll\",").unwrap();
    wait_until(|| connection.budget.used() > 0);
    let parses = connection.budget.parses.load(Ordering::Acquire);
    clock.store(600_000, Ordering::Release);
    connection.reader.get_mut().write_all(b"\"params\":{\"timeoutMs\":0}}\n").unwrap();
    connection.assert_expired();
    assert_eq!(connection.budget.parses.load(Ordering::Acquire), parses, "expired bytes never reach serde");
    wait_until(|| connection.budget.used() == 0);
}

#[test]
fn an_admitted_request_keeps_its_existing_timeout_and_delivers_its_result() {
    let credentials = Credentials::new();
    let clock = Arc::new(AtomicU64::new(0));
    let mut connection = Connection::new(&credentials, &clock);
    writeln!(connection.reader.get_mut(), "{}", json!({"id":7,"method":"engine.commands","timeoutMs":5000})).unwrap();
    wait_until(|| !connection.pending.lock().unwrap().is_empty());
    let request = connection.pending.lock().unwrap().pop().unwrap();
    clock.store(600_000, Ordering::Release);
    request.reply.send(ControlReply { id: request.id, result: Ok(json!({"completed":true})) }).unwrap();
    assert_eq!(reply(&mut connection.reader), json!({"id":7,"ok":true,"result":{"completed":true}}));
    // Wake an implementation already waiting for another frame as well as a boundary check.
    let _ = connection.reader.get_mut().write_all(b" \n");
    connection.assert_expired();
}

#[test]
fn remaining_lease_caps_the_next_read_without_refreshing_the_clock() {
    let clock = Arc::new(AtomicU64::new(100));
    let policy = policy(&clock);
    assert_eq!(policy.timeout.min(policy.remaining()), Duration::from_secs(2));
    clock.store(599_000, Ordering::Release);
    assert_eq!(policy.timeout.min(policy.remaining()), Duration::from_millis(1100));
    clock.store(600_099, Ordering::Release);
    assert_eq!(policy.timeout.min(policy.remaining()), Duration::from_millis(1));
    clock.store(600_100, Ordering::Release);
    assert_eq!(policy.remaining(), Duration::ZERO);
    clock.store(u64::MAX, Ordering::Release);
    assert_eq!(policy.remaining(), Duration::ZERO);
}

#[test]
fn eight_expired_authenticated_workers_release_a_queued_legitimate_client() {
    let credentials = Credentials::new();
    let clock = Arc::new(AtomicU64::new(0));
    let mut connections: Vec<_> = (0..MAX_CONNECTIONS).map(|_| Connection::new(&credentials, &clock)).collect();
    let mut workers: Vec<_> = connections
        .iter_mut()
        .map(|connection| {
            let worker = connection.worker.take().unwrap();
            std::thread::spawn(move || {
                let _ = worker.join();
            })
        })
        .collect();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let mut ninth = BufReader::new(TcpStream::connect(listener.local_addr().unwrap()).unwrap());
    ninth.get_ref().set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    ninth.get_ref().set_write_timeout(Some(Duration::from_secs(2))).unwrap();
    let (observed, full) = mpsc::sync_channel(1);
    let inner = Arc::new(ControlInner::default());
    *inner.capacity_observer.lock().unwrap() = Some(observed);
    let (lease, next_clock) = (credentials.lease.clone(), clock.clone());
    let mut sockets: Vec<_> = connections.iter().map(|connection| connection.reader.get_ref().try_clone().unwrap()).collect();
    sockets.push(ninth.get_ref().try_clone().unwrap());
    let worker = std::thread::spawn(move || {
        assert!(wait_for_connection_capacity(&mut workers, &inner, 0));
        let (socket, _) = listener.accept().unwrap();
        let result = serve_with_policy(
            socket,
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(None)),
            Arc::new(EventLog::default()),
            policy(&next_clock),
            Some(lease),
            Arc::new(crate::control_budget::Budget::new(64 * 1024 * 1024)),
        );
        for worker in workers {
            worker.join().unwrap();
        }
        result
    });
    let mut accepting = Accepting { worker: Some(worker), sockets };
    assert_eq!(full.recv_timeout(Duration::from_secs(2)).unwrap(), MAX_CONNECTIONS, "actual listener observes all eight occupied slots");
    clock.store(600_000, Ordering::Release);
    for connection in &mut connections {
        connection.reader.get_mut().write_all(b" \n").unwrap();
    }
    credentials.authenticate(&mut ninth);
    poll(&mut ninth, 42);
    let admitted = reply(&mut ninth);
    assert_eq!(admitted["id"], 42);
    assert_eq!(admitted["ok"], true, "newly admitted connection receives a fresh lease");
    for connection in &mut connections {
        connection.assert_expired();
    }
    ninth.get_ref().shutdown(Shutdown::Both).unwrap();
    assert!(accepting.worker.take().unwrap().join().unwrap().is_ok());
}
