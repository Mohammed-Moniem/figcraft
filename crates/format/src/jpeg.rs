//! JPEG sampling-header inspection. This remains available for diagnostics; it is not a decode
//! denylist. The PR374 backport lets valid progressive CMYK frames reach the bounded decoder.

/// Whether `bytes` are a progressive JPEG whose frame samples some component more finely (across
/// or down) than its first. Unpatched zune-jpeg 0.5.15 asserted on some such frames. The local
/// package backports both sampling-stride and scratch-space fixes from upstream PR374, so this
/// property alone no longer rejects an image. Header inspection does not establish pixel validity.
pub fn finer_than_first(bytes: &[u8]) -> bool {
    let Some((marker, frame)) = frame(bytes) else { return false };
    if !matches!(marker, 0xC2 | 0xC6 | 0xCA | 0xCE) {
        return false;
    }
    // Length (2), precision (1), height (2), width (2), components (1), then id, sampling and
    // table for each component.
    let n = usize::from(frame.get(7).copied().unwrap_or(0));
    let sampling = |k: usize| frame.get(9 + 3 * k).map(|b| (b >> 4, b & 0x0F));
    let Some((h0, v0)) = sampling(0) else { return false };
    (1..n).any(|k| sampling(k).is_some_and(|(h, v)| h > h0 || v > v0))
}

/// A JPEG's start-of-frame marker and the segment after it (from its length bytes), found as the
/// decoders find it: fill bytes are skipped and markers without a length stepped over. `None` for
/// anything but a JPEG, or one whose scan, end or bytes come before a frame.
fn frame(bytes: &[u8]) -> Option<(u8, &[u8])> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut at = 2;
    loop {
        if *bytes.get(at)? != 0xFF {
            return None;
        }
        let marker = *bytes.get(at + 1)?;
        match marker {
            0xFF => at += 1,
            0x01 | 0xD0..=0xD8 => at += 2,
            0xD9 | 0xDA => return None,
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return Some((marker, bytes.get(at + 2..)?)),
            _ => {
                let len = usize::from(u16::from_be_bytes([*bytes.get(at + 2)?, *bytes.get(at + 3)?]));
                at = at.checked_add(2)?.checked_add(len)?;
            }
        }
    }
}
