//! Shared limits and arithmetic for the configurable canvas pixel grid.
use serde::Deserialize;

pub const DEFAULT_SIZE: f64 = 1.0;
pub const MIN_SIZE: f64 = 0.125;
pub const MAX_SIZE: f64 = 10_000.0;
/// A corrupt or enormous viewport must not allocate or paint an unbounded grid.
pub const MAX_LINES: usize = 4096;

pub fn valid_size(size: f64) -> Option<f64> {
    (size.is_finite() && (MIN_SIZE..=MAX_SIZE).contains(&size)).then_some(size)
}

pub fn size_or_default(size: f64) -> f64 {
    valid_size(size).unwrap_or(DEFAULT_SIZE)
}

/// Bad hand-edited settings must not discard the rest of the preferences.
pub(crate) fn read_size<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(serde_json::Value::deserialize(d)?.as_f64().and_then(valid_size).unwrap_or(DEFAULT_SIZE))
}

/// Quantize a finite coordinate without overflowing when dividing by a fractional step.
pub fn round(value: f64, size: f64) -> f64 {
    let Some(size) = valid_size(size) else { return value };
    let result = (value / size).round() * size;
    if result.is_finite() { result } else { value }
}

/// Visible page-origin grid multiples. Indexed work avoids x += step stalling at large origins.
pub fn positions(min: f64, max: f64, size: f64) -> Vec<f64> {
    let Some(size) = valid_size(size) else { return Vec::new() };
    if !min.is_finite() || !max.is_finite() || min > max {
        return Vec::new();
    }
    let first = (min / size).ceil();
    let last = (max / size).floor();
    let span = last - first;
    // Adjacent multiples must be representable; otherwise do not draw a misleading grid.
    if !span.is_finite() || !(0.0..MAX_LINES as f64).contains(&span) || first + 1.0 == first {
        return Vec::new();
    }
    let lines: Vec<f64> = (0..=(span as usize)).map(|i| (first + i as f64) * size).filter(|v| v.is_finite() && *v >= min && *v <= max).collect();
    // The range can cross a precision boundary even if its first adjacent index is distinct.
    if lines.windows(2).any(|pair| pair[0] >= pair[1]) { Vec::new() } else { lines }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grid_positions_are_origin_aligned_and_bounded_for_hostile_viewports() {
        assert_eq!(positions(-17.0, 17.0, 8.0), [-16.0, -8.0, 0.0, 8.0, 16.0]);
        assert_eq!(positions(-0.3, 0.3, 0.125), [-0.25, -0.125, 0.0, 0.125, 0.25]);
        for (a, b, step) in
            [(f64::NAN, 1.0, 1.0), (0.0, f64::INFINITY, 1.0), (1e30, 1e30 + 1e16, 1.0), (-1e9, 1e9, 0.125), (0.0, 10.0, 0.0), (1.0, 0.0, 1.0)]
        {
            assert!(positions(a, b, step).is_empty());
        }
        assert_eq!(positions(0.0, (MAX_LINES - 1) as f64, 1.0).len(), MAX_LINES);
        assert!(round(f64::MAX, 0.125).is_finite());
        assert_eq!(round(-12.0, 8.0), -16.0);
    }
}
