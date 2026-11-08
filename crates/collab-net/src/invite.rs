//! Transport-facing reexports of the shared, socket-free invite parser.

pub use figcraft_collab::invite::{Endpoint, Invite, Lookup, loopback_ip};

/// Ids and tokens are URL-safe base64 / hex characters.
pub(crate) fn valid_id(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
