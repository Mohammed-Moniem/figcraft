//! Admission accounting for untrusted control frames, shared across connections and rebinds.
//!
//! This is an ingress allocation envelope, not a process RSS or document/command-output limit.
//! Preflight charges both wire bytes and JSON structure BEFORE serde may expand the input.
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

const SHARED_BYTES: usize = 64 * 1024 * 1024;
const STRUCTURE_BYTES: usize = 4096;
const WIRE_COPIES: usize = 12;
const FRAME_OVERHEAD: usize = 4096;
const READ_CHUNK: usize = 4096;
const MAX_DEPTH: usize = 128;

pub(super) struct Budget {
    limit: usize,
    used: AtomicUsize,
    #[cfg(test)]
    pub(super) parses: AtomicUsize,
}

impl Budget {
    pub(super) fn shared() -> Arc<Self> {
        static BUDGET: OnceLock<Arc<Budget>> = OnceLock::new();
        BUDGET.get_or_init(|| Arc::new(Self::new(SHARED_BYTES))).clone()
    }

    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            used: AtomicUsize::new(0),
            #[cfg(test)]
            parses: AtomicUsize::new(0),
        }
    }

    fn reserve(self: &Arc<Self>, bytes: usize) -> Option<Reservation> {
        let mut reservation = Reservation { budget: self.clone(), bytes: 0 };
        reservation.grow(bytes).then_some(reservation)
    }

    /// Lexical preflight deliberately counts keys, unknown fields and duplicate keys as well as
    /// values. It allocates nothing and does not replace serde's syntax/UTF-8/number validation.
    /// A structure unit pays for tree slots/map nodes, their spare/transient capacity and the
    /// bounded ingress copies; wire pays for strings, escape scratch and converted UI text.
    pub(super) fn parsed(self: &Arc<Self>, line: &[u8]) -> Option<Arc<Reservation>> {
        // The envelope was reviewed for this bounded Value/String representation. Fail closed
        // if a future parser/toolchain changes it; re-audit Cargo features and copy lifetimes.
        if std::mem::size_of::<serde_json::Value>() > 64 || std::mem::size_of::<String>() > 24 {
            return None;
        }
        let mut quoted = false;
        let mut escaped = false;
        let mut scalar = false;
        let mut depth = 0usize;
        let mut units = 0usize;
        for &byte in line {
            if quoted {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quoted = false;
                }
                continue;
            }
            match byte {
                b'"' => {
                    quoted = true;
                    scalar = false;
                    units = units.checked_add(1)?;
                }
                b'{' | b'[' => {
                    depth = depth.checked_add(1)?;
                    if depth > MAX_DEPTH {
                        return None;
                    }
                    scalar = false;
                    units = units.checked_add(1)?;
                }
                b'}' | b']' => {
                    depth = depth.saturating_sub(1);
                    scalar = false;
                }
                b',' | b':' | b' ' | b'\r' | b'\n' | b'\t' => scalar = false,
                _ if !scalar => {
                    units = units.checked_add(1)?;
                    scalar = true;
                }
                _ => {}
            }
        }
        let bytes = FRAME_OVERHEAD.checked_add(units.checked_mul(STRUCTURE_BYTES)?)?.checked_add(line.len().checked_mul(WIRE_COPIES)?)?;
        self.reserve(bytes).map(Arc::new)
    }

    #[cfg(test)]
    pub(super) fn used(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
}

/// Release only when the last real payload owner drops, never merely when its gate settles.
pub(super) struct Reservation {
    budget: Arc<Budget>,
    bytes: usize,
}

impl Reservation {
    fn grow(&mut self, bytes: usize) -> bool {
        let Some(owned) = self.bytes.checked_add(bytes) else {
            return false;
        };
        if self
            .budget
            .used
            .try_update(Ordering::AcqRel, Ordering::Relaxed, |used| used.checked_add(bytes).filter(|next| *next <= self.budget.limit))
            .is_err()
        {
            return false;
        }
        self.bytes = owned;
        true
    }

    fn shrink(&mut self, bytes: usize) {
        self.bytes -= bytes;
        self.budget.used.fetch_sub(bytes, Ordering::AcqRel);
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

pub(super) enum Frame {
    Eof,
    Ready(Vec<u8>, Reservation),
    Refused(&'static str),
}

/// Raw Vec capacity is reserved before growth; a fresh Vec is dropped for every frame. The
/// fixed 8KiB socket reader per admitted worker remains separate, constant overhead.
/// Each frame has an absolute read deadline, so trickle input cannot retain admission forever.
pub(super) fn read_frame(reader: &mut BufReader<TcpStream>, budget: &Arc<Budget>, max_line: usize, timeout: Duration) -> std::io::Result<Frame> {
    let mut bytes = Vec::new();
    let mut reservation = Reservation { budget: budget.clone(), bytes: 0 };
    let started = Instant::now();
    loop {
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Ok(Frame::Refused("control frame read deadline exceeded"));
        }
        reader.get_ref().set_read_timeout(Some(remaining))?;
        let available = match reader.fill_buf() {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            other => other?,
        };
        if available.is_empty() {
            return if bytes.is_empty() { Ok(Frame::Eof) } else { Ok(Frame::Ready(bytes, reservation)) };
        }
        let take = available.iter().position(|byte| *byte == b'\n').map_or(available.len(), |n| n + 1);
        let end = bytes.len().checked_add(take).ok_or_else(|| std::io::Error::other("control frame length overflow"))?;
        let newline = available.get(take - 1) == Some(&b'\n');
        if end > max_line.saturating_add(usize::from(newline)) {
            return Ok(Frame::Refused("message too large"));
        }
        if end > bytes.capacity() {
            let capacity = end.checked_add(READ_CHUNK - 1).map(|n| n / READ_CHUNK * READ_CHUNK);
            let Some(capacity) = capacity else {
                return Ok(Frame::Refused("control memory budget exhausted"));
            };
            // Pay for the whole replacement while the old allocation is still live. Reserving
            // only the delta would miss the peak during allocator growth/copy.
            if !reservation.grow(capacity) {
                return Ok(Frame::Refused("control memory budget exhausted"));
            }
            let mut next = Vec::new();
            if next.try_reserve_exact(capacity).is_err() {
                return Ok(Frame::Refused("control memory allocation unavailable"));
            }
            next.extend_from_slice(&bytes);
            let old_capacity = bytes.capacity();
            drop(std::mem::replace(&mut bytes, next));
            reservation.shrink(old_capacity);
        }
        bytes.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline {
            bytes.pop();
            return Ok(Frame::Ready(bytes, reservation));
        }
    }
}

#[cfg(test)]
#[path = "control_budget_tests.rs"]
mod tests;
