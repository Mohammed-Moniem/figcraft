//! Localhost JSON-lines control server: each line `{"id", "method", "params"}` is queued for the UI
//! thread; the reply line `{"id", "ok", "result" | "error"}` is written when the frame handled it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use figcraft_engine::events::{self, EventLog};
use figcraft_ui_egui::{ControlReply, ControlRequest, RequestGate, RequestState};
use serde_json::{Value, json};

type Queue = Arc<Mutex<Vec<ControlRequest>>>;
type CtxSlot = Arc<Mutex<Option<egui::Context>>>;

const MAX_LINE: usize = 32 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 8;
const MAX_QUEUED_REQUESTS: usize = 8;
const TIMEOUT: Duration = Duration::from_secs(60);
/// The longest a request's `timeoutMs` can make it wait: a client can raise the wait, never lift it.
const MAX_TIMEOUT: Duration = Duration::from_secs(600);
/// How long the accepting thread waits after a failed accept. Accepting can keep failing, for
/// example while the process has no file descriptors left, and retrying at once would keep a
/// core busy until it recovers.
const ACCEPT_RETRY: Duration = Duration::from_millis(100);

/// Why a request's transport wait ended without a control reply.
#[derive(Debug, PartialEq, Eq)]
enum WaitFailure {
    Timeout,
    /// The UI side dropped its sender before replying (the app is closing).
    Disconnected,
    /// The UI started the request before the timeout and is still running it: it takes effect
    /// when it finishes, though nobody waits for its reply (#1673).
    StillRunning,
}

/// Wait for the UI's reply, and settle a timeout under the same gate the UI's final commit uses.
/// A commit that got the gate first has already queued its reply: consume that reply instead of
/// reporting a timeout. If the gate is still open, cancel it so the UI writes nothing.
fn wait_for_reply(rx: &std::sync::mpsc::Receiver<ControlReply>, gate: &RequestGate, timeout: Duration) -> Result<ControlReply, WaitFailure> {
    match rx.recv_timeout(timeout) {
        Ok(reply) => Ok(reply),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => after_timeout(rx, gate),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(WaitFailure::Disconnected),
    }
}

/// The timeout branch of [`wait_for_reply`]. Kept separate so it can be tested deterministically
/// against a gate whose commit already happened.
fn after_timeout(rx: &std::sync::mpsc::Receiver<ControlReply>, gate: &RequestGate) -> Result<ControlReply, WaitFailure> {
    match gate.cancel_if_open() {
        RequestState::Replied => rx.try_recv().map_err(|_| WaitFailure::Disconnected),
        // It may have finished just now: its reply is the truth then.
        RequestState::Running => rx.try_recv().map_err(|_| WaitFailure::StillRunning),
        RequestState::Open | RequestState::Cancelled => Err(WaitFailure::Timeout),
    }
}

/// How long to wait for the reply to request `v`: its `timeoutMs` (a whole number, at most
/// [`MAX_TIMEOUT`]), else `default` (#1696).
fn request_timeout(v: &Value, default: Duration) -> Duration {
    v.get("timeoutMs").and_then(Value::as_u64).map_or(default, |ms| Duration::from_millis(ms).min(MAX_TIMEOUT))
}

/// What a request that got no reply after `waited` is told. Only a request that needs a drawn frame
/// can be held up by the window being out of sight; any other was either never started (the app
/// was busy, so it was dropped and never applied) or is still running and will be applied (#1673).
fn timeout_message(failure: &WaitFailure, needs_frame: bool, waited: Duration) -> String {
    let secs = waited.as_secs_f64().ceil();
    match failure {
        WaitFailure::Disconnected => "the FigCraft window closed before answering".into(),
        WaitFailure::StillRunning => format!(
            "the app is still running this request after {secs} s: it will take effect when it finishes, but its reply is lost. Check the document before retrying"
        ),
        WaitFailure::Timeout if needs_frame => {
            "timed out waiting for the UI: screenshots and input need the FigCraft window on screen (is it minimised or covered?)".into()
        }
        WaitFailure::Timeout => {
            format!("timed out after {secs} s before the app started this request (it was busy, or its window isn't drawing): it was not applied")
        }
    }
}

/// Deferred requests for a hidden window must not make the UI's queue grow without a bound.
pub fn drain_queue(queue: &Queue, target: &mut Vec<ControlRequest>) {
    target.retain(|request| !request.is_cancelled());
    let mut queue = queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    queue.retain(|request| !request.is_cancelled());
    let count = queue.len().min(MAX_QUEUED_REQUESTS.saturating_sub(target.len()));
    target.extend(queue.drain(..count));
}

/// Bind `port`, waiting up to `busy` for it while another process holds it: an app started again on
/// other graphics ([`crate::graphics`]) starts beside the one ending, which still holds it.
pub fn bind(port: u16, busy: Duration) -> std::io::Result<TcpListener> {
    let since = std::time::Instant::now();
    loop {
        match TcpListener::bind(("127.0.0.1", port)) {
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && since.elapsed() < busy => std::thread::sleep(Duration::from_millis(50)),
            bound => return bound,
        }
    }
}

/// Shared handle to the local control server, used by the UI to inspect status and toggle/rebind.
#[derive(Clone, Default)]
pub struct ControlHandle {
    inner: Arc<ControlInner>,
}

#[derive(Default)]
struct ControlInner {
    listening_port: AtomicU16,
    active_connections: AtomicUsize,
    generation: AtomicU64,
    last_error: Mutex<Option<String>>,
    deps: Mutex<Option<(Queue, CtxSlot, Arc<EventLog>)>>,
    data_dir: Mutex<Option<std::path::PathBuf>>,
    auth: Mutex<Option<Arc<crate::control_auth::Lease>>>,
    apply_lock: Mutex<()>,
}

impl ControlHandle {
    /// Create an unstarted `ControlHandle`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach the shared request queue, egui context slot, and session event log before starting.
    pub fn bind_deps(&self, queue: Queue, ctx: CtxSlot, events: Arc<EventLog>) {
        if let Ok(mut d) = self.inner.deps.lock() {
            *d = Some((queue, ctx, events));
        }
    }

    /// Return `(listening_port, active_connections, last_error)`.
    pub fn set_data_dir(&self, dir: Option<std::path::PathBuf>) {
        *self.inner.data_dir.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = dir;
    }
    pub fn status(&self) -> (u16, usize, Option<String>) {
        let port = self.inner.listening_port.load(Ordering::Relaxed);
        let active = self.inner.active_connections.load(Ordering::Relaxed);
        let err = self.inner.last_error.lock().ok().and_then(|e| e.clone());
        (port, active, err)
    }

    /// Start, rebind, or stop the localhost TCP control listener.
    pub fn apply(&self, enabled: bool, port: u16, busy: Duration) {
        let _serial = self.inner.apply_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let old_port = self.inner.listening_port.load(Ordering::Acquire);
        if enabled && port != 0 && self.inner.deps.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_none() {
            *self.inner.last_error.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some("control request dependencies are not ready".into());
            return;
        }
        if enabled && old_port == port && old_port != 0 {
            return;
        }
        if (!enabled || port == 0)
            && let Some(auth) = self.inner.auth.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
        {
            auth.retire();
        }
        let next_listener = if enabled && port != 0 {
            match bind(port, busy) {
                Ok(listener) => Some(listener),
                Err(e) => {
                    *self.inner.last_error.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(format!("can't bind control port: {e}"));
                    return;
                }
            }
        } else {
            None
        };
        let next_auth = if next_listener.is_some() {
            let dir = self.inner.data_dir.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone().or_else(crate::services::config_dir);
            match dir
                .ok_or_else(|| "control requires a local data directory".to_string())
                .and_then(|dir| crate::control_auth::Lease::create(&dir, port))
            {
                Ok(auth) => Some(Arc::new(auth)),
                Err(e) => {
                    *self.inner.last_error.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(e);
                    return;
                }
            }
        } else {
            None
        };
        if let Some(old) = std::mem::replace(&mut *self.inner.auth.lock().unwrap_or_else(std::sync::PoisonError::into_inner), next_auth) {
            old.retire();
        }
        self.inner.listening_port.store(0, Ordering::Release);
        let next_gen = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if old_port != 0 {
            let _ = TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], old_port)), Duration::from_millis(50));
        }
        if let Ok(mut err) = self.inner.last_error.lock() {
            *err = None;
        }
        if !enabled || port == 0 {
            return;
        }
        let Some((queue, ctx, events)) = self.inner.deps.lock().ok().and_then(|d| d.clone()) else {
            return;
        };
        let listener = match next_listener.ok_or_else(|| std::io::Error::other("control disabled")) {
            Ok(l) => l,
            Err(e) => {
                let msg = format!("can't bind 127.0.0.1:{port}: {e}");
                log::error!("control server: {msg} (another app may own it; pick another port)");
                eprintln!("FigCraft control server failed to bind port {port}: {e}");
                if let Ok(mut err) = self.inner.last_error.lock() {
                    *err = Some(msg);
                }
                return;
            }
        };
        self.inner.listening_port.store(port, Ordering::SeqCst);
        eprintln!("FigCraft control server listening on 127.0.0.1:{port}");
        let inner = self.inner.clone();
        let spawned = std::thread::Builder::new().name("figcraft-control".into()).spawn(move || {
            let mut workers = Vec::new();
            for stream in accepted(listener.incoming(), || std::thread::sleep(ACCEPT_RETRY)) {
                if inner.generation.load(Ordering::SeqCst) != next_gen {
                    break;
                }
                spawn_connection_tracked(stream, queue.clone(), ctx.clone(), events.clone(), Some(inner.clone()), &mut workers, TIMEOUT);
            }
        });
        if let Err(e) = spawned {
            log::error!("control server thread: {e}");
            self.inner.listening_port.store(0, Ordering::SeqCst);
            if let Some(auth) = self.inner.auth.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
                auth.retire();
            }
            if let Ok(mut err) = self.inner.last_error.lock() {
                *err = Some(format!("can't start the server thread: {e}"));
            }
        }
    }
}

/// The connections `incoming` accepts, calling `wait` after each failed accept.
fn accepted<T>(incoming: impl Iterator<Item = std::io::Result<T>>, mut wait: impl FnMut()) -> impl Iterator<Item = T> {
    incoming.filter_map(move |stream| stream.map_err(|_| wait()).ok())
}

#[cfg(test)]
fn spawn_connection(
    stream: TcpStream,
    queue: Queue,
    ctx: CtxSlot,
    events: Arc<EventLog>,
    workers: &mut Vec<std::thread::JoinHandle<()>>,
    timeout: Duration,
) {
    spawn_connection_tracked(stream, queue, ctx, events, None, workers, timeout);
}

/// Spawn a worker thread for an accepted control connection while tracking active connection count.
fn spawn_connection_tracked(
    stream: TcpStream,
    queue: Queue,
    ctx: CtxSlot,
    events: Arc<EventLog>,
    inner: Option<Arc<ControlInner>>,
    workers: &mut Vec<std::thread::JoinHandle<()>>,
    timeout: Duration,
) {
    let auth = inner.as_ref().and_then(|state| state.auth.lock().ok().and_then(|a| a.clone()));
    if inner.is_some() && auth.is_none() {
        return;
    }
    workers.retain(|worker| !worker.is_finished());
    if workers.len() >= MAX_CONNECTIONS {
        return;
    }
    match std::thread::Builder::new().name("figcraft-control-conn".into()).spawn(move || {
        if let Some(ref inner) = inner {
            inner.active_connections.fetch_add(1, Ordering::Relaxed);
        }
        let _ = serve_session(stream, queue, ctx, events, timeout, auth);
        if let Some(ref inner) = inner {
            inner.active_connections.fetch_sub(1, Ordering::Relaxed);
        }
    }) {
        Ok(worker) => workers.push(worker),
        Err(e) => log::error!("control connection thread: {e}"),
    }
}

/// Serve one control connection: read bounded JSON lines, reject oversized frames and a
/// full queue, queue one request, wait under its gate, and write one reply. Non-JSON input
/// answers with an error and closes the connection so HTTP body lines cannot become commands.
/// Serve a connection with a private event log (the tests; the app shares its session's log).
#[cfg(test)]
fn serve(out: TcpStream, queue: Queue, ctx: CtxSlot, timeout: Duration) -> std::io::Result<()> {
    serve_with_events(out, queue, ctx, Arc::new(EventLog::default()), timeout)
}

#[cfg(test)]
fn serve_with_events(out: TcpStream, queue: Queue, ctx: CtxSlot, events: Arc<EventLog>, timeout: Duration) -> std::io::Result<()> {
    serve_session(out, queue, ctx, events, timeout, None)
}
fn serve_session(
    mut out: TcpStream,
    queue: Queue,
    ctx: CtxSlot,
    events: Arc<EventLog>,
    timeout: Duration,
    auth: Option<Arc<crate::control_auth::Lease>>,
) -> std::io::Result<()> {
    if let Some(auth) = auth.as_ref()
        && !auth.authenticate(&mut out)?
    {
        return Ok(());
    }
    out.set_read_timeout(Some(timeout))?;
    out.set_write_timeout(Some(timeout))?;
    let mut reader = BufReader::new(out.try_clone()?);
    let mut line = Vec::new();
    loop {
        if auth.as_ref().is_some_and(|a| !a.active()) {
            return Ok(());
        }
        line.clear();
        if (&mut reader).take((MAX_LINE + 1) as u64).read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if line.len() > MAX_LINE {
            writeln!(out, "{}", json!({"id": null, "ok": false, "error": "message too large"}))?;
            break;
        }
        let Ok(line) = std::str::from_utf8(&line) else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(v) => {
                let id = v.get("id").cloned().unwrap_or(Value::Null);
                let method = v.get("method").and_then(Value::as_str).unwrap_or("").to_string();
                let params = v.get("params").cloned().unwrap_or(json!({}));
                if auth.as_ref().is_some_and(|a| !a.active()) {
                    return Ok(());
                }
                // A poll waits on the shared log, not on a frame: it needs no UI thread and no
                // request slot, and it works while the window is minimised.
                if method == "events.poll" {
                    let reply = match event_poll(&events, &params) {
                        Ok(result) => json!({"id": id, "ok": true, "result": result}),
                        Err(error) => json!({"id": id, "ok": false, "error": error}),
                    };
                    writeln!(out, "{reply}")?;
                    continue;
                }
                let waits = request_timeout(&v, timeout);
                let (tx, rx) = std::sync::mpsc::channel();
                let gate = RequestGate::with_timeout(waits);
                let mut pending = queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                pending.retain(|request| !request.is_cancelled());
                if pending.len() >= MAX_QUEUED_REQUESTS {
                    drop(pending);
                    writeln!(out, "{}", json!({"id": id, "ok": false, "error": "control request queue is full"}))?;
                    continue;
                }
                let needs_frame = matches!(method.as_str(), "ui.screenshot" | "ui.input");
                pending.push(ControlRequest { id: id.clone(), method, params, reply: tx, state: gate.clone() });
                drop(pending);
                if let Some(c) = ctx.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
                    c.request_repaint();
                }
                match wait_for_reply(&rx, &gate, waits) {
                    Ok(r) => match r.result {
                        Ok(res) => json!({"id": r.id, "ok": true, "result": res}),
                        Err(e) => json!({"id": r.id, "ok": false, "error": e}),
                    },
                    Err(failure) => json!({"id": id, "ok": false, "error": timeout_message(&failure, needs_frame, waits)}),
                }
            }
            Err(e) => {
                // A line that isn't JSON ends the connection: a web page can make the browser send
                // an HTTP request here, and its header lines must not let the JSON lines of its body
                // through as commands.
                writeln!(out, "{}", json!({"id": null, "ok": false, "error": format!("bad JSON: {e}")}))?;
                break;
            }
        };
        writeln!(out, "{reply}")?;
    }
    Ok(())
}

/// `events.poll {after?, timeoutMs?, topics?, limit?}`: read the shared log, waiting up to 25 s.
fn event_poll(log: &EventLog, params: &Value) -> Result<Value, String> {
    let (after, topics, limit, timeout) = events::poll_options(params, 20_000, 25_000)?;
    let result = log.poll(after, &topics, limit, timeout);
    serde_json::to_value(result).map_err(|e| format!("serialize events: {e}"))
}

#[cfg(test)]
#[path = "control_server_tests.rs"]
mod tests;
