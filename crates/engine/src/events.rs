//! In-memory events for agents watching a live document.
//!
//! Comments are the feedback queue: `comment.add`, `reply`, `resolve` and the other mutating
//! `comment.*` commands append an event. The control channel's `events.poll` long-polls this log,
//! and the MCP server exposes it as `wait_for_changes`. Nothing here touches the file system, the
//! document format or the undo history.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;
use web_time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

/// Most events kept; the oldest are dropped.
pub const MAX_EVENTS: usize = 1024;

/// One change an agent can react to.
#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub seq: u64,
    pub time: f64,
    pub kind: String,
    pub document: String,
    pub path: Option<String>,
    pub data: Value,
}

/// What `events.poll` returns.
#[derive(Clone, Debug, Serialize)]
pub struct PollResult {
    pub events: Vec<Event>,
    pub cursor: u64,
    pub reset: bool,
    pub dropped: u64,
}

#[derive(Default)]
struct Log {
    next: u64,
    events: VecDeque<Event>,
}

/// A bounded, thread-safe log. The UI thread appends; control-channel workers wait.
#[derive(Default)]
pub struct EventLog {
    log: Mutex<Log>,
    wake: Condvar,
}

impl EventLog {
    /// Append one event and wake every waiter.
    pub fn push(&self, kind: &str, document: &str, path: Option<&str>, data: Value) {
        let mut log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
        let seq = log.next.saturating_add(1);
        log.next = seq;
        log.events.push_back(Event {
            seq,
            time: now(),
            kind: bounded_text(kind, 128),
            document: bounded_text(document, 512),
            path: path.map(|s| bounded_text(s, 4096)),
            data: bounded_data(data),
        });
        while log.events.len() > MAX_EVENTS {
            log.events.pop_front();
        }
        drop(log);
        self.wake.notify_all();
    }

    /// The newest sequence number (0 before the first event).
    pub fn latest(&self) -> u64 {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).next
    }

    /// Events after `after` that match `topics`, or wait up to `timeout` for one. `limit` is clamped
    /// to 1..=1000. A zero timeout returns at once. Browser polling always returns immediately:
    /// blocking its event loop would prevent the next edit from ever waking this log.
    ///
    /// `reset` means `after` is ahead of the log (the app restarted, or the cursor is stale): the
    /// caller should re-read the comments instead of trusting the cursor. `dropped` counts events
    /// that were lost because `after` is older than the retained window (0 for a fresh cursor).
    pub fn poll(&self, after: u64, topics: &[String], limit: usize, timeout: Duration) -> PollResult {
        let limit = limit.clamp(1, 1000);
        let deadline = Instant::now() + timeout;
        let mut log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            let latest = log.next;
            let ahead = after > latest;
            let first = log.events.front().map_or(latest.saturating_add(1), |e| e.seq);
            let dropped = if after > 0 && after < first.saturating_sub(1) { first.saturating_sub(1) - after } else { 0 };
            let reset = ahead || dropped > 0;
            let events: Vec<Event> = log.events.iter().filter(|e| e.seq > after && keeps(topics, &e.kind)).take(limit).cloned().collect();
            if !events.is_empty() || reset || timeout.is_zero() || cfg!(target_arch = "wasm32") {
                let cursor = events.last().map_or(latest, |e| e.seq);
                return PollResult { events, cursor, reset, dropped };
            }
            let now = Instant::now();
            if now >= deadline {
                return PollResult { events: Vec::new(), cursor: after.min(latest), reset, dropped };
            }
            let (guard, _) = self.wake.wait_timeout(log, deadline - now).unwrap_or_else(|e| e.into_inner());
            log = guard;
        }
    }
}

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// Whether an event kind matches the requested topics. An empty list (or `all`) keeps everything.
fn keeps(topics: &[String], kind: &str) -> bool {
    topics.is_empty()
        || topics.iter().any(|t| match t.as_str() {
            "all" => true,
            "comments" => kind.starts_with("comment."),
            "document" => kind.starts_with("document."),
            _ => false,
        })
}

/// `events.poll`'s `topics`: default `["comments"]`, `[]` means `all`, and unknown names are errors.
pub fn topics(v: Option<&Value>) -> Result<Vec<String>, String> {
    let Some(v) = v.filter(|v| !v.is_null()) else { return Ok(vec!["comments".into()]) };
    let Some(list) = v.as_array() else { return Err("`topics` must be an array of strings".into()) };
    let mut out = Vec::with_capacity(list.len());
    for t in list {
        let Some(s) = t.as_str() else { return Err("`topics` must be an array of strings".into()) };
        match s {
            // Each once: the list is scanned for every retained event under the log's lock, so
            // a long list of repeats would multiply that work.
            "comments" | "document" | "all" if out.iter().any(|t| t == s) => {}
            "comments" | "document" | "all" => out.push(s.to_string()),
            other => return Err(format!("unknown topic `{other}` (comments, document, all)")),
        }
    }
    if out.is_empty() || out.iter().any(|t| t == "all") {
        out = vec!["all".into()];
    }
    Ok(out)
}

/// The event a successful mutating `comment.*` command records, if it records one.
pub fn comment_event_kind(command: &str) -> Option<&'static str> {
    Some(match command {
        "comment.add" => "comment.added",
        "comment.reply" => "comment.replied",
        "comment.edit" => "comment.edited",
        "comment.react" => "comment.reacted",
        "comment.resolve" => "comment.resolved",
        "comment.move" => "comment.moved",
        "comment.delete" => "comment.deleted",
        _ => return None,
    })
}

fn bounded_text(text: &str, max: usize) -> String {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1)
    }
    text[..end].to_string()
}
fn bounded_data(data: Value) -> Value {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            if self.0 > 8192 {
                return Err(std::io::Error::other("event exceeds payload limit"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    if serde_json::to_writer(Count(0), &data).is_ok() { data } else { serde_json::json!({"reset":true,"reason":"event_payload_limit"}) }
}

/// Strict transport parameters; browser callers pass a zero maximum wait to avoid blocking UI.
pub fn poll_options(p: &Value, default_ms: u64, max_ms: u64) -> Result<(u64, Vec<String>, usize, Duration), String> {
    let object = p.as_object().ok_or("event params must be an object")?;
    if object.keys().any(|k| !matches!(k.as_str(), "after" | "topics" | "limit" | "timeoutMs")) {
        return Err("unknown event parameter".into());
    }
    let integer = |key: &str, default: u64, max: u64| -> Result<u64, String> {
        let value = match p.get(key) {
            None => default,
            Some(v) => v.as_u64().ok_or_else(|| format!("{key} must be a whole nonnegative integer"))?,
        };
        if value > max {
            return Err(format!("{key} exceeds its limit"));
        }
        Ok(value)
    };
    let after = integer("after", 0, u64::MAX)?;
    let limit = integer("limit", 100, 1000)?;
    if limit == 0 {
        return Err("limit must be 1–1000".into());
    }
    let timeout = integer("timeoutMs", default_ms, max_ms)?;
    let selected = topics(p.get("topics"))?;
    Ok((after, selected, limit as usize, Duration::from_millis(timeout)))
}
