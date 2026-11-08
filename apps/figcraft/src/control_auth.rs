//! Opt-in native control authentication. Tokens never enter logs, preferences or command journals.
use figcraft_ui_egui::{RequestGate, RequestState};
use serde_json::{Value, json};
use std::sync::Mutex;

// A lease retains only gate state, never request parameters. The explicit cap also bounds
// deferred requests that survive transport workers; settled/running entries are pruned.
const MAX_REGISTERED_REQUESTS: usize = 32;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const REFUSAL_DRAIN_BYTES: usize = 4096;
const REFUSAL_DRAIN_TIME: Duration = Duration::from_millis(50);

fn auth_io_error(operation: &'static str, error: std::io::Error) -> std::io::Error {
    std::io::Error::new(error.kind(), format!("control authentication {operation}: raw_os_error={:?}: {error}", error.raw_os_error()))
}

/// Refresh the remaining absolute authentication budget before every short write.
struct AuthReplyWriter<'a> {
    stream: &'a mut TcpStream,
    deadline: Instant,
}
impl Write for AuthReplyWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "control authentication deadline"));
        }
        self.stream.set_write_timeout(Some(remaining)).map_err(|error| auth_io_error("reply write-timeout", error))?;
        self.stream.write(bytes).map_err(|error| auth_io_error("reply write", error))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

fn write_auth_reply(writer: &mut impl Write, reply: &Value) -> std::io::Result<()> {
    // Formatting Value directly into the socket makes even the opening brace a separate write.
    // Prepare the complete frame before any socket I/O, including the protocol newline.
    let mut frame = serde_json::to_vec(reply).map_err(std::io::Error::other)?;
    frame.push(b'\n');
    writer.write_all(&frame)
}

fn discard_refused_input(
    mut read: impl FnMut(&mut [u8], Duration) -> std::io::Result<usize>,
    mut now: impl FnMut() -> Instant,
    mut wait: impl FnMut(Duration),
    deadline: Instant,
) -> std::io::Result<usize> {
    let mut discarded = 0;
    let mut buffer = [0; 256];
    while discarded < REFUSAL_DRAIN_BYTES {
        let remaining = deadline.saturating_duration_since(now());
        if remaining.is_zero() {
            break;
        }
        let limit = (REFUSAL_DRAIN_BYTES - discarded).min(buffer.len());
        match read(&mut buffer[..limit], remaining) {
            Ok(0) => break,
            Ok(n) => discarded += n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                // A nonblocking peer may remain open without sending anything. Back off only
                // within the current absolute budget, then recheck it before another read.
                let remaining = deadline.saturating_duration_since(now());
                if !remaining.is_zero() {
                    wait(remaining.min(Duration::from_millis(1)));
                }
            }
            Err(error)
                if matches!(error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe) =>
            {
                break;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(discarded)
}

fn close_refused_auth(stream: &mut TcpStream, auth_deadline: Instant) -> std::io::Result<()> {
    // Send FIN after the complete refusal. Closing a socket with unread pipelined bytes can
    // otherwise send RST and discard the response. Cleanup neither parses nor admits commands.
    // Set nonblocking before FIN: resetting a read timeout after half-close can fail on macOS
    // as the peer concurrently closes. No socket timeout is changed during the drain.
    stream.set_nonblocking(true).map_err(|error| auth_io_error("refusal nonblocking", error))?;
    stream.shutdown(Shutdown::Write).map_err(|error| auth_io_error("refusal shutdown-write", error))?;
    let deadline = auth_deadline.min(Instant::now() + REFUSAL_DRAIN_TIME);
    discard_refused_input(
        |buffer, _remaining| stream.read(buffer).map_err(|error| auth_io_error("refusal drain read", error)),
        Instant::now,
        std::thread::sleep,
        deadline,
    )?;
    Ok(())
}
pub struct Lease {
    path: PathBuf,
    token: String,
    active: AtomicBool,
    requests: Mutex<Vec<RequestGate>>,
}
impl Lease {
    pub fn create(dir: &Path, port: u16) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("control credentials: {e}"))?;
        let token = format!("{}{}", figcraft_collab_net::token::secret()?, figcraft_collab_net::token::secret()?);
        let path = dir.join(format!("control-{port}.token"));
        if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.file_type().is_file()) {
            return Err("control credential path is not a regular file".into());
        }
        let temp = dir.join(format!(".control-{port}-{}.tmp", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&temp)?;
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temp, &path)
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&temp);
            return Err(format!("cannot publish control credential: {e}"));
        }
        Ok(Self { path, token, active: AtomicBool::new(true), requests: Mutex::new(Vec::new()) })
    }
    /// Bind an admitted request to this credential lease before it enters either UI queue.
    /// Registration and the active recheck share a lock with retirement's registry drain.
    pub(crate) fn register_request(&self, gate: &RequestGate) -> bool {
        let mut requests = self.requests.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        requests.retain(|request| request.state() == RequestState::Open);
        if !self.active() || requests.len() >= MAX_REGISTERED_REQUESTS {
            drop(requests);
            gate.cancel_if_open();
            return false;
        }
        requests.push(gate.clone());
        true
    }

    pub fn active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    pub fn retire(&self) {
        self.active.store(false, Ordering::Release);
        // A request may already have moved out of the native queue into a hidden window's
        // deferred queue. Cancel its shared gate, not just the entries still visible here.
        // Drop the registry lock before acquiring gate locks or doing credential-file I/O.
        let requests = std::mem::take(&mut *self.requests.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        for request in requests {
            request.cancel_if_open();
        }
        if std::fs::read_to_string(&self.path).is_ok_and(|s| s == self.token) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    pub fn authenticate(&self, stream: &mut TcpStream) -> std::io::Result<bool> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut line = Vec::with_capacity(256);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() || line.len() >= 1024 || !self.active() {
                return Ok(false);
            }
            stream.set_read_timeout(Some(remaining)).map_err(|error| auth_io_error("initial auth read-timeout", error))?;
            let mut b = [0];
            if stream.read(&mut b).map_err(|error| auth_io_error("initial auth read", error))? == 0 {
                return Ok(false);
            }
            if b[0] == b'\n' {
                break;
            }
            line.push(b[0]);
        }
        let valid = serde_json::from_slice::<Value>(&line).ok().is_some_and(|v| {
            v.as_object().is_some_and(|o| o.keys().all(|k| matches!(k.as_str(), "id" | "method" | "token")))
                && v["method"] == "auth"
                && v["token"].as_str().is_some_and(|t| figcraft_engine::figcraft_collab::same_secret(t, &self.token))
        });
        let reply = if valid {
            json!({"id":null,"ok":true,"result":{"authenticated":true,"protocol":1}})
        } else {
            json!({"id":null,"ok":false,"error":"control authentication required"})
        };
        write_auth_reply(&mut AuthReplyWriter { stream, deadline }, &reply)?;
        if !valid {
            close_refused_auth(stream, deadline)?;
        }
        Ok(valid)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.retire()
    }
}

/// New helper controls run only with the production proposal, never as an old-code red claim.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod refusal_io_tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct ShortWriter {
        bytes: Vec<u8>,
        offered: Vec<usize>,
        interrupt_first: bool,
        stop_after: Option<std::io::ErrorKind>,
    }
    impl Write for ShortWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.offered.push(bytes.len());
            if self.interrupt_first && self.offered.len() == 1 {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            if self.bytes.len() >= 3
                && let Some(kind) = self.stop_after
            {
                return if kind == std::io::ErrorKind::WriteZero { Ok(0) } else { Err(kind.into()) };
            }
            let n = bytes.len().min(3);
            self.bytes.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn refusal() -> Value {
        json!({"id": null, "ok": false, "error": "control authentication required"})
    }

    #[test]
    fn auth_frame_is_complete_before_first_write_and_survives_short_writes() {
        let mut writer = ShortWriter::default();
        write_auth_reply(&mut writer, &refusal()).unwrap();
        assert_eq!(writer.offered.first(), Some(&writer.bytes.len()), "first write receives the entire serialized frame");
        assert!(writer.offered.len() > 1, "the fake transport actually makes short writes");
        assert_eq!(writer.bytes.last(), Some(&b'\n'));
        assert_eq!(writer.bytes.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert_eq!(serde_json::from_slice::<Value>(&writer.bytes).unwrap(), refusal());
    }

    #[test]
    fn interrupted_auth_write_retries_without_duplicating_the_frame() {
        let mut writer = ShortWriter { interrupt_first: true, ..Default::default() };
        write_auth_reply(&mut writer, &refusal()).unwrap();
        let mut expected = serde_json::to_vec(&refusal()).unwrap();
        expected.push(b'\n');
        assert_eq!(writer.bytes, expected);
        assert_eq!(writer.offered[0], writer.offered[1]);
    }

    #[test]
    fn zero_and_failed_short_writes_never_report_complete_authentication_output() {
        for kind in [std::io::ErrorKind::WriteZero, std::io::ErrorKind::BrokenPipe] {
            let mut writer = ShortWriter { stop_after: Some(kind), ..Default::default() };
            let error = write_auth_reply(&mut writer, &refusal()).unwrap_err();
            assert_eq!(error.kind(), kind);
            assert_eq!(writer.bytes.len(), 3);
            assert!(!writer.bytes.ends_with(b"\n"));
        }
    }

    #[test]
    fn expired_auth_output_deadline_writes_no_bytes_to_a_real_socket() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let mut server = listener.accept().unwrap().0;
        let error = write_auth_reply(&mut AuthReplyWriter { stream: &mut server, deadline: Instant::now() }, &refusal()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        drop(server);
        let mut output = Vec::new();
        client.read_to_end(&mut output).unwrap();
        assert!(output.is_empty());
    }

    #[test]
    fn discard_caps_bytes_even_when_the_peer_always_has_more_input() {
        let start = Instant::now();
        let calls = Cell::new(0);
        let discarded = discard_refused_input(
            |buffer, remaining| {
                assert!(remaining > Duration::ZERO && buffer.len() <= 256);
                calls.set(calls.get() + 1);
                buffer.fill(b' ');
                Ok(buffer.len())
            },
            || start,
            |_| panic!("byte-capped continuous input must not wait"),
            start + REFUSAL_DRAIN_TIME,
        )
        .unwrap();
        assert_eq!(discarded, 4096);
        assert_eq!(calls.get(), 16);
    }

    #[test]
    fn discard_stops_at_absolute_deadline_for_partial_reads_and_would_block() {
        let start = Instant::now();
        let elapsed = Cell::new(Duration::ZERO);
        let calls = Cell::new(0);
        let discarded = discard_refused_input(
            |_, remaining| {
                assert_eq!(remaining, Duration::from_millis(20) - elapsed.get());
                calls.set(calls.get() + 1);
                elapsed.set(elapsed.get() + Duration::from_millis(7));
                Ok(32)
            },
            || start + elapsed.get(),
            |_| panic!("successful partial reads must not wait"),
            start + Duration::from_millis(20),
        )
        .unwrap();
        assert_eq!(discarded, 96);
        assert_eq!(calls.get(), 3);

        let elapsed = Cell::new(Duration::ZERO);
        let calls = Cell::new(0);
        let waits = RefCell::new(Vec::new());
        let discarded = discard_refused_input(
            |_, _| {
                calls.set(calls.get() + 1);
                elapsed.set(elapsed.get() + Duration::from_micros(250));
                Err(std::io::ErrorKind::WouldBlock.into())
            },
            || start + elapsed.get(),
            |delay| {
                assert!(delay > Duration::ZERO && delay <= Duration::from_millis(1));
                assert!(elapsed.get() + delay <= Duration::from_millis(3), "read time counts against the same deadline");
                waits.borrow_mut().push(delay);
                elapsed.set(elapsed.get() + delay);
            },
            start + Duration::from_millis(3),
        )
        .unwrap();
        assert_eq!(discarded, 0);
        assert_eq!(calls.get(), 3, "WouldBlock backs off instead of spinning or ending cleanup immediately");
        assert_eq!(*waits.borrow(), [Duration::from_millis(1), Duration::from_millis(1), Duration::from_micros(250)]);
        assert_eq!(elapsed.get(), Duration::from_millis(3));
    }

    #[test]
    fn interrupted_discard_cannot_refresh_the_absolute_deadline() {
        let start = Instant::now();
        let elapsed = Cell::new(Duration::ZERO);
        let calls = Cell::new(0);
        let discarded = discard_refused_input(
            |_, _| {
                calls.set(calls.get() + 1);
                elapsed.set(elapsed.get() + Duration::from_millis(10));
                Err(std::io::ErrorKind::Interrupted.into())
            },
            || start + elapsed.get(),
            |_| panic!("interrupted reads must recheck their deadline"),
            start + Duration::from_millis(20),
        )
        .unwrap();
        assert_eq!(discarded, 0);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn expired_discard_never_reads_and_peer_endings_finish_without_a_retry() {
        let start = Instant::now();
        assert_eq!(
            discard_refused_input(|_, _| panic!("expired cleanup must not read"), || start, |_| panic!("expired cleanup must not wait"), start)
                .unwrap(),
            0
        );
        let endings = [None, Some(std::io::ErrorKind::TimedOut), Some(std::io::ErrorKind::ConnectionReset), Some(std::io::ErrorKind::BrokenPipe)];
        for ending in endings {
            let calls = Cell::new(0);
            let discarded = discard_refused_input(
                |_, _| {
                    calls.set(calls.get() + 1);
                    ending.map_or(Ok(0), |kind| Err(kind.into()))
                },
                || start,
                |_| panic!("peer termination must not wait"),
                start + REFUSAL_DRAIN_TIME,
            )
            .unwrap();
            assert_eq!(discarded, 0);
            assert_eq!(calls.get(), 1);
        }

        let elapsed = Cell::new(Duration::ZERO);
        let calls = Cell::new(0);
        let waits = Cell::new(0);
        let discarded = discard_refused_input(
            |_, _| {
                calls.set(calls.get() + 1);
                match calls.get() {
                    1 => Err(std::io::ErrorKind::WouldBlock.into()),
                    2 => Ok(7),
                    _ => Ok(0),
                }
            },
            || start + elapsed.get(),
            |delay| {
                waits.set(waits.get() + 1);
                elapsed.set(elapsed.get() + delay);
            },
            start + REFUSAL_DRAIN_TIME,
        )
        .unwrap();
        assert_eq!(discarded, 7, "input arriving after WouldBlock is discarded before EOF");
        assert_eq!(calls.get(), 3);
        assert_eq!(waits.get(), 1);
    }

    #[test]
    fn unexpected_discard_error_propagates_without_command_admission_or_spin() {
        let start = Instant::now();
        let calls = Cell::new(0);
        let error = discard_refused_input(
            |_, _| {
                calls.set(calls.get() + 1);
                Err(std::io::ErrorKind::PermissionDenied.into())
            },
            || start,
            |_| panic!("unexpected read errors must propagate immediately"),
            start + REFUSAL_DRAIN_TIME,
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(calls.get(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("figcraft-auth-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn private_token_lease_never_erases_a_replacement() {
        let dir = dir("lease");
        let lease = Lease::create(&dir, 39791).unwrap();
        let path = dir.join("control-39791.token");
        let first = std::fs::read_to_string(&path).unwrap();
        assert_eq!(first.len(), 44);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let newer = Lease::create(&dir, 39791).unwrap();
        assert_ne!(std::fs::read_to_string(&path).unwrap(), first);
        lease.retire();
        assert!(path.exists());
        newer.retire();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
    #[cfg(unix)]
    #[test]
    fn linked_credential_target_is_rejected_without_touching_its_target() {
        use std::os::unix::fs::symlink;
        let dir = dir("linked");
        let target = dir.join("other");
        std::fs::write(&target, b"private").unwrap();
        symlink(&target, dir.join("control-39792.token")).unwrap();
        assert!(Lease::create(&dir, 39792).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"private");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn request_registry_is_bounded_and_prunes_settled_or_started_entries() {
        let dir = dir("request-bound");
        let lease = Lease::create(&dir, 39793).unwrap();
        let held: Vec<RequestGate> = (0..MAX_REGISTERED_REQUESTS)
            .map(|_| {
                let gate = RequestGate::new();
                assert!(lease.register_request(&gate));
                gate
            })
            .collect();
        let refused = RequestGate::new();
        assert!(!lease.register_request(&refused));
        assert_eq!(refused.state(), RequestState::Cancelled);
        assert!(held.iter().all(|gate| gate.state() == RequestState::Open), "capacity refusal changes no earlier admission");
        assert!(held[0].start());
        let replacement = RequestGate::new();
        assert!(lease.register_request(&replacement), "a Running gate needs no cancellation slot");
        assert_eq!(lease.requests.lock().unwrap().len(), MAX_REGISTERED_REQUESTS);
        lease.retire();
        assert_eq!(held[0].state(), RequestState::Running, "retirement never falsely cancels started work");
        assert!(held[1..].iter().all(|gate| gate.state() == RequestState::Cancelled));
        assert_eq!(replacement.state(), RequestState::Cancelled);
        assert!(lease.requests.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_retired_lease_refuses_late_request_registration_without_an_effect() {
        let dir = dir("request-retired");
        let lease = Lease::create(&dir, 39794).unwrap();
        lease.retire();
        let gate = RequestGate::new();
        assert!(!lease.register_request(&gate));
        assert_eq!(gate.state(), RequestState::Cancelled);
        assert!(!gate.start());
        assert!(lease.requests.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
