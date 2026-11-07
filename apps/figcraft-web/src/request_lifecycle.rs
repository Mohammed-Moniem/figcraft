//! Bounded page requests. The same gate checks the monotonic deadline when the UI starts work;
//! a delayed browser timer therefore cannot make an expired request run late.

use std::io::Write;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use figcraft_ui_egui::{ControlReply, ControlRequest, RequestGate, RequestState};
use serde_json::{Value, json};

pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub const MAX_TIMEOUT_MS: u64 = 600_000;
pub const MAX_PENDING: usize = 32;
// A native 64 MiB file expands to about 85.4 MiB of base64. Keep that supported direct-open path
// bounded without reducing the existing host's native-file limit. The outer bridge stays smaller.
pub const MAX_REQUEST_BYTES: usize = 96 * 1024 * 1024;
pub const MAX_PENDING_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_OPEN_BYTES: usize = 64 * 1024 * 1024;
const MAX_DATE_MS: u64 = 8_640_000_000_000_000;

pub fn conditional_capability() -> Value {
    figcraft_ui_egui::control::conditional_execute_capability()
}

pub fn capabilities() -> Value {
    json!({
        "conditionalExecute":conditional_capability(),
        "documentEvents":figcraft_ui_egui::control::document_events_capability(),
        "requestLifecycle":{
            "version":1,"options":["timeoutMs","deadlineEpochMs"],
            "defaultTimeoutMs":DEFAULT_TIMEOUT_MS,"maxTimeoutMs":MAX_TIMEOUT_MS,
            "maxPending":MAX_PENDING,"maxRequestBytes":MAX_REQUEST_BYTES,
            "maxPendingBytes":MAX_PENDING_BYTES,"maxOpenBytes":MAX_OPEN_BYTES,
            "cancelQueued":true,"cancelRunning":false
        }
    })
}

#[derive(Debug)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
    pub started: bool,
    pub may_have_applied: bool,
    pub rejected: bool,
}

impl Failure {
    pub fn admission(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), started: false, may_have_applied: false, rejected: true }
    }

    fn editor(message: String) -> Self {
        let code = ["CONTROL_INVALID_EXPECTED", "CONTROL_DOCUMENT_CONFLICT", "CONTROL_REVISION_CONFLICT"]
            .into_iter()
            .find(|code| message.strip_prefix(code).is_some_and(|tail| tail.starts_with(':')));
        Self { code: code.unwrap_or("CONTROL_EDITOR_ERROR"), message, started: true, may_have_applied: code.is_none(), rejected: code.is_some() }
    }

    fn timeout(state: RequestState) -> Self {
        if state == RequestState::Running {
            Self {
                code: "CONTROL_TIMEOUT_RUNNING",
                message: "The editor started this request before the deadline. Its effect is uncertain; inspect before retrying. No retry was made."
                    .into(),
                started: true,
                may_have_applied: true,
                rejected: false,
            }
        } else {
            Self::admission("CONTROL_TIMEOUT_BEFORE_START", "The request expired before the editor started it and will not be applied.")
        }
    }

    fn dropped(state: RequestState) -> Self {
        let started = matches!(state, RequestState::Running | RequestState::Replied);
        Self {
            code: "CONTROL_REQUEST_DROPPED",
            message: if started {
                "The editor stopped answering after starting the request. Inspect before retrying; its effect is uncertain."
            } else {
                "The editor dropped the queued request before starting it."
            }
            .into(),
            started,
            may_have_applied: started,
            rejected: !started,
        }
    }
}

pub fn timeout(options: &Value, now_epoch_ms: f64) -> Result<Duration, Failure> {
    let options = options.as_object().ok_or_else(|| Failure::admission("CONTROL_INVALID_OPTIONS", "Request options must be an object."))?;
    if options.keys().any(|key| !matches!(key.as_str(), "timeoutMs" | "deadlineEpochMs")) {
        return Err(Failure::admission("CONTROL_INVALID_OPTIONS", "Request options take only timeoutMs and deadlineEpochMs."));
    }
    let wait = match options.get("timeoutMs") {
        None => DEFAULT_TIMEOUT_MS,
        Some(value) => value
            .as_u64()
            .filter(|value| (1..=MAX_TIMEOUT_MS).contains(value))
            .ok_or_else(|| Failure::admission("CONTROL_INVALID_OPTIONS", "timeoutMs must be an integer from 1 to 600000."))?,
    };
    if !now_epoch_ms.is_finite() || !(0.0..=MAX_DATE_MS as f64).contains(&now_epoch_ms) {
        return Err(Failure::admission("CONTROL_INVALID_OPTIONS", "The current request clock is invalid."));
    }
    let remaining = match options.get("deadlineEpochMs") {
        None => wait as f64,
        Some(value) => {
            let deadline = value.as_u64().filter(|value| *value <= MAX_DATE_MS).ok_or_else(|| {
                Failure::admission("CONTROL_INVALID_OPTIONS", "deadlineEpochMs must be an integer within the JavaScript date range.")
            })?;
            let remaining = deadline as f64 - now_epoch_ms;
            if remaining <= 0.0 {
                return Err(Failure::admission("CONTROL_TIMEOUT_BEFORE_START", "The request deadline already passed; nothing was queued."));
            }
            remaining.min(wait as f64)
        }
    };
    Ok(Duration::from_secs_f64(remaining / 1000.0))
}

// Count serialized request bytes without first allocating another potentially large JSON string.
struct Counter(usize);

impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        if self.0 > MAX_REQUEST_BYTES {
            return Err(std::io::Error::other("request too large"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn request_bytes(method: &str, params: &Value) -> Result<usize, Failure> {
    let mut counter = Counter(64);
    // Count escaped method bytes too; a quote-heavy unknown method must not consume more than
    // its advertised envelope budget merely because its UTF-8 input was short.
    serde_json::to_writer(&mut counter, method)
        .map_err(|_| Failure::admission("CONTROL_PAYLOAD_TOO_LARGE", "The encoded request exceeds 96 MiB."))?;
    serde_json::to_writer(&mut counter, params)
        .map_err(|_| Failure::admission("CONTROL_PAYLOAD_TOO_LARGE", "The encoded request exceeds 96 MiB."))?;
    Ok(counter.0)
}

struct Pending<T> {
    receiver: Receiver<ControlReply>,
    gate: RequestGate,
    bytes: usize,
    deadline_epoch_ms: f64,
    context: T,
}

pub struct Completion<T> {
    pub context: T,
    pub result: Result<Value, Failure>,
}

pub struct Requests<T> {
    next_id: u64,
    pending: Vec<Pending<T>>,
    queued: Vec<ControlRequest>,
    bytes: usize,
}

impl<T> Default for Requests<T> {
    fn default() -> Self {
        Self { next_id: 0, pending: Vec::new(), queued: Vec::new(), bytes: 0 }
    }
}

impl<T> Requests<T> {
    pub fn enqueue(&mut self, method: &str, params: Value, options: &Value, now_epoch_ms: f64, context: T) -> Result<(), Failure> {
        if method.is_empty() || method.len() > 120 || method.chars().any(char::is_control) || !params.is_object() {
            return Err(Failure::admission("CONTROL_INVALID_REQUEST", "Use a bounded method name and an object of request parameters."));
        }
        let duration = timeout(options, now_epoch_ms)?;
        // Start the monotonic clock before counting a large encoded payload. Sizing work must
        // not extend the caller's absolute admission deadline.
        let gate = RequestGate::with_timeout(duration);
        if self.pending.len() >= MAX_PENDING {
            return Err(Failure::admission("CONTROL_QUEUE_FULL", "The page already has 32 unanswered requests."));
        }
        let bytes = request_bytes(method, &params)?;
        if self.bytes.saturating_add(bytes) > MAX_PENDING_BYTES {
            return Err(Failure::admission("CONTROL_QUEUE_FULL", "Unanswered requests exceed the 128 MiB pending-byte budget."));
        }
        let id = self.next_id.checked_add(1).filter(|id| *id <= 9_007_199_254_740_991).ok_or_else(|| {
            Failure::admission("CONTROL_REQUEST_ID_EXHAUSTED", "The page request identity budget is exhausted. Reload after saving your work.")
        })?;
        let (reply, receiver) = std::sync::mpsc::channel();
        self.queued.retain(|request| !request.is_cancelled());
        self.queued.push(ControlRequest { id: json!(id), method: method.into(), params, reply, state: gate.clone() });
        self.pending.push(Pending { receiver, gate, bytes, deadline_epoch_ms: now_epoch_ms + duration.as_secs_f64() * 1000.0, context });
        self.next_id = id;
        self.bytes += bytes;
        Ok(())
    }

    pub fn take_requests(&mut self) -> Vec<ControlRequest> {
        std::mem::take(&mut self.queued).into_iter().filter(|request| !request.is_cancelled()).collect()
    }

    /// Reap the current absolute clock before handing work to a frame. A wall-clock jump or a
    /// throttled timer must not leave a not-yet-expired monotonic gate eligible for late admission.
    pub fn take_for_frame(&mut self, now_epoch_ms: f64) -> (Vec<ControlRequest>, Vec<Completion<T>>) {
        let completed = self.poll(now_epoch_ms);
        (self.take_requests(), completed)
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn poll(&mut self, now_epoch_ms: f64) -> Vec<Completion<T>> {
        let mut completed = Vec::new();
        let mut waiting = Vec::new();
        for pending in self.pending.drain(..) {
            let result = match pending.receiver.try_recv() {
                Ok(reply) => Some(reply.result.map_err(Failure::editor)),
                Err(TryRecvError::Disconnected) => {
                    let state = pending.gate.cancel_if_open();
                    Some(Err(if pending.gate.deadline_elapsed() || now_epoch_ms >= pending.deadline_epoch_ms {
                        Failure::timeout(state)
                    } else {
                        Failure::dropped(state)
                    }))
                }
                Err(TryRecvError::Empty) if pending.gate.deadline_elapsed() || now_epoch_ms >= pending.deadline_epoch_ms => {
                    let state = pending.gate.cancel_if_open();
                    // A screenshot commits its reply under this same gate. Prefer that real reply
                    // if it won the race; do not turn a successful durable commit into a timeout.
                    match pending.receiver.try_recv() {
                        Ok(reply) => Some(reply.result.map_err(Failure::editor)),
                        _ => Some(Err(if state == RequestState::Replied { Failure::dropped(state) } else { Failure::timeout(state) })),
                    }
                }
                Err(TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                self.bytes = self.bytes.saturating_sub(pending.bytes);
                completed.push(Completion { context: pending.context, result });
            } else {
                waiting.push(pending);
            }
        }
        self.pending = waiting;
        self.queued.retain(|request| !request.is_cancelled());
        completed
    }
}

impl<T> Drop for Requests<T> {
    fn drop(&mut self) {
        for pending in &self.pending {
            pending.gate.cancel_if_open();
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn options_are_strict_and_deadlines_use_the_earlier_bound() {
        assert_eq!(timeout(&json!({}), 1000.0).unwrap(), Duration::from_secs(30));
        assert_eq!(timeout(&json!({"timeoutMs":20,"deadlineEpochMs":1010}), 1000.0).unwrap(), Duration::from_millis(10));
        for options in [
            json!(null),
            json!({"timeoutMs":0}),
            json!({"timeoutMs":600001}),
            json!({"timeoutMs":1.5}),
            json!({"deadlineEpochMs":-1}),
            json!({"deadlineEpochMs":9_007_199_254_740_992u64}),
            json!({"extra":true}),
        ] {
            assert_eq!(timeout(&options, 1000.0).unwrap_err().code, "CONTROL_INVALID_OPTIONS");
        }
        assert_eq!(timeout(&json!({"deadlineEpochMs":1000}), 1000.0).unwrap_err().code, "CONTROL_TIMEOUT_BEFORE_START");
        assert!(timeout(&json!({}), f64::NAN).is_err());
    }

    #[test]
    fn unanswered_count_is_bounded_and_completion_frees_capacity() {
        let mut requests = Requests::default();
        for id in 0..MAX_PENDING {
            requests.enqueue("engine.commands", json!({}), &json!({}), 1000.0, id).unwrap();
        }
        assert_eq!(requests.enqueue("engine.commands", json!({}), &json!({}), 1000.0, 99).unwrap_err().code, "CONTROL_QUEUE_FULL");
        let mut queued = requests.take_requests();
        let request = queued.pop().unwrap();
        request.reply.send(ControlReply { id: request.id, result: Ok(json!({"exact":true})) }).unwrap();
        let complete = requests.poll(1000.0);
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].context, MAX_PENDING - 1);
        assert_eq!(complete[0].result.as_ref().unwrap(), &json!({"exact":true}));
        requests.enqueue("engine.commands", json!({}), &json!({}), 1000.0, 99).unwrap();
    }

    #[test]
    fn queued_deadline_cancels_gate_and_cannot_be_taken_or_started_late() {
        let mut requests = Requests::default();
        requests.enqueue("engine.execute", json!({"command":"file.new"}), &json!({}), 1000.0, ()).unwrap();
        // Expiration is deterministic without sleeping: replace the pending gate with the exact
        // shared gate of an already-expired request, as a timer-delayed browser would encounter.
        let expired = RequestGate::with_timeout(Duration::ZERO);
        requests.pending[0].gate = expired.clone();
        requests.queued[0].state = expired.clone();
        let result = requests.poll(1000.0).pop().unwrap().result.unwrap_err();
        assert_eq!(result.code, "CONTROL_TIMEOUT_BEFORE_START");
        assert!(!result.started && !result.may_have_applied && result.rejected);
        assert!(!expired.start());
        assert!(requests.take_requests().is_empty());
        assert!(!requests.has_pending());
        assert_eq!(requests.bytes, 0);
    }

    #[test]
    fn running_timeout_never_claims_cancellation_or_retries() {
        let mut requests = Requests::default();
        requests.enqueue("engine.execute", json!({"command":"file.new"}), &json!({}), 1000.0, ()).unwrap();
        let mut queued = requests.take_requests();
        let request = queued.pop().unwrap();
        assert!(request.state.start());
        // Advance the caller's wall clock beyond its deadline without a sleep. The shared
        // monotonic gate remains Running: ending a wait cannot undo admitted work.
        let result = requests.poll(31_001.0).pop().unwrap().result.unwrap_err();
        assert_eq!(result.code, "CONTROL_TIMEOUT_RUNNING");
        assert!(result.message.contains("uncertain"));
        assert!(result.started && result.may_have_applied && !result.rejected);
        assert_eq!(request.state.state(), RequestState::Running);
        assert!(requests.take_requests().is_empty());
        request.reply.send(ControlReply { id: request.id, result: Ok(json!({"later":true})) }).ok();
        assert!(requests.poll(31_001.0).is_empty());
    }

    #[test]
    fn frame_admission_reaps_a_wall_clock_jump_without_waiting_for_a_timer() {
        let mut requests = Requests::default();
        requests.enqueue("engine.execute", json!({"command":"file.new"}), &json!({}), 1000.0, ()).unwrap();
        let gate = requests.pending[0].gate.clone();
        // The monotonic timeout is still thirty seconds away, but the next frame observes an
        // absolute deadline in the past. It must settle the promise before admitting any work.
        let (admitted, completed) = requests.take_for_frame(31_001.0);
        assert!(admitted.is_empty());
        assert_eq!(completed.len(), 1);
        let failure = completed.into_iter().next().unwrap().result.unwrap_err();
        assert_eq!(failure.code, "CONTROL_TIMEOUT_BEFORE_START");
        assert!(!failure.started && !failure.may_have_applied);
        assert!(!gate.start());
        assert!(!requests.has_pending());
        assert_eq!(requests.bytes, 0);
    }

    #[test]
    fn exact_committed_reply_wins_over_an_elapsed_deadline() {
        let mut requests = Requests::default();
        requests.enqueue("ui.screenshot", json!({}), &json!({}), 1000.0, ()).unwrap();
        let request = requests.take_requests().pop().unwrap();
        request.state.commit(|| request.reply.send(ControlReply { id: request.id.clone(), result: Ok(json!({"saved":true})) }).unwrap()).unwrap();
        assert_eq!(requests.poll(31_001.0).pop().unwrap().result.unwrap(), json!({"saved":true}));
    }

    #[test]
    fn guard_refusal_is_not_reported_as_a_possible_edit() {
        let mut requests = Requests::default();
        requests.enqueue("engine.execute", json!({}), &json!({}), 1000.0, ()).unwrap();
        let request = requests.take_requests().pop().unwrap();
        request.reply.send(ControlReply { id: request.id, result: Err("CONTROL_REVISION_CONFLICT: inspect again".into()) }).unwrap();
        let failure = requests.poll(1000.0).pop().unwrap().result.unwrap_err();
        assert_eq!(failure.code, "CONTROL_REVISION_CONFLICT");
        assert!(failure.rejected && !failure.may_have_applied);
        assert_eq!(capabilities()["conditionalExecute"], conditional_capability());
        let session = figcraft_engine::Session::new();
        assert!(
            figcraft_engine::inspect::document(&session, 0, false).get("capabilities").is_none(),
            "an unguarded headless transport must not inherit a UI control capability"
        );
        let mut app = figcraft_ui_egui::FigApp::new(session, Box::new(figcraft_ui_egui::NoServices));
        let inspected = figcraft_ui_egui::control::handle(&mut app, &egui::Context::default(), "document.inspect", &json!({})).unwrap();
        assert_eq!(inspected["capabilities"]["conditionalExecute"], conditional_capability());
    }

    #[test]
    fn byte_accounting_refuses_growth_without_another_serialized_copy() {
        let mut counter = Counter(MAX_REQUEST_BYTES);
        assert!(counter.write_all(b"x").is_err());
        let mut requests = Requests::default();
        requests.bytes = MAX_PENDING_BYTES;
        assert_eq!(requests.enqueue("engine.commands", json!({}), &json!({}), 1000.0, ()).unwrap_err().code, "CONTROL_QUEUE_FULL");
        assert!(!requests.has_pending());
    }
    #[test]
    fn advertised_document_events_match_supported_control_polling() {
        let expected = figcraft_ui_egui::control::document_events_capability();
        assert_eq!(capabilities()["documentEvents"], expected, "page registration must advertise the event method it supports");
        let session = figcraft_engine::Session::new();
        let mut app = figcraft_ui_egui::FigApp::new(session, Box::new(figcraft_ui_egui::NoServices));
        let ctx = egui::Context::default();
        let inspected = figcraft_ui_egui::control::handle(&mut app, &ctx, "document.inspect", &json!({})).unwrap();
        assert_eq!(inspected["capabilities"]["documentEvents"], expected);
        app.session.execute("page.rename", &json!({"name":"Unsaved event"})).unwrap();
        let events = figcraft_ui_egui::control::handle(&mut app, &ctx, "events.poll", &json!({"topics":["document"]})).unwrap();
        assert!(events["events"].as_array().unwrap().iter().any(|event| event["kind"] == "document.changed"));
    }
}
