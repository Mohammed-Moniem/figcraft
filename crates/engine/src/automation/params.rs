//! Parameter parsing helpers shared by commands (lenient: agents send numbers as strings, ids as
//! "12" or "0:12", colours as hex or CSS).

use figcraft_color::{BlendMode, Color, Paint};
use figcraft_doc::NodeId;
use serde_json::Value;

use crate::{EngineError, Result, Session, bad};

pub fn get<'a>(p: &'a Value, key: &str) -> Option<&'a Value> {
    p.get(key).filter(|v| !v.is_null())
}

pub use figcraft_automation_json::f64_of;

/// A 0-1 fraction: a `%` suffix or a number above 1 is a percentage ("1%" is 0.01, 50 is 0.5).
pub fn fraction_of(v: &Value) -> Option<f64> {
    let f = f64_of(v)?;
    let percent = v.as_str().is_some_and(|s| s.trim().ends_with('%'));
    Some(if percent || f > 1.0 { f / 100.0 } else { f })
}

pub fn num(p: &Value, key: &str) -> Option<f64> {
    get(p, key).and_then(f64_of)
}

pub fn num_or(p: &Value, key: &str, d: f64) -> f64 {
    num(p, key).unwrap_or(d)
}

pub fn req_num(cmd: &str, p: &Value, key: &str) -> Result<f64> {
    num(p, key).ok_or_else(|| bad(cmd, format!("`{key}` must be a number")))
}

/// A list index from params: a whole number, never negative (no casting -1 to 0).
/// Boolean numeric coercion is useful for other parameters, but cannot choose a list entry.
pub fn req_index(cmd: &str, p: &Value, key: &str) -> Result<usize> {
    if get(p, key).is_some_and(Value::is_boolean) {
        return Err(bad(cmd, format!("`{key}` must be a whole number")));
    }
    let k = req_num(cmd, p, key)?;
    if !(0.0..1e9).contains(&k) || k.fract() != 0.0 {
        return Err(bad(cmd, format!("`{key}` out of range")));
    }
    Ok(k as usize)
}

pub fn string(p: &Value, key: &str) -> Option<String> {
    match get(p, key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub fn req_string(cmd: &str, p: &Value, key: &str) -> Result<String> {
    string(p, key).ok_or_else(|| bad(cmd, format!("`{key}` must be a string")))
}

pub fn boolean(p: &Value, key: &str) -> Option<bool> {
    bool_of(get(p, key)?)
}

/// A lenient boolean: `true`, `1`, `"yes"`, `"on"`…
pub fn bool_of(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_f64().map(|f| f != 0.0),
        Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Some(true),
            "false" | "no" | "off" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

pub fn id_of(v: &Value) -> Option<NodeId> {
    figcraft_automation_json::id_of(v).map(NodeId)
}

pub fn id(p: &Value, key: &str) -> Option<NodeId> {
    get(p, key).and_then(id_of)
}

pub fn req_id(cmd: &str, p: &Value, key: &str) -> Result<NodeId> {
    id(p, key).ok_or_else(|| bad(cmd, format!("`{key}` must be a node id")))
}

/// The offending value for an error message, cut to a few dozen characters so a huge param
/// doesn't end up in the error whole.
fn shown(v: &Value) -> String {
    const MAX: usize = 40;
    let text = v.to_string();
    match text.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    }
}

/// `key` when it is there: a layer id, or an error naming it. Missing and `null` mean "none", so a
/// command can tell "no parent given" (use the page) from "a parent was given that isn't an id" —
/// which is a mistake, not the default. Without this, `{"$step": 0}` reaching a param that isn't
/// resolved yet silently becomes "no parent" and the layer lands on the page.
pub fn opt_id(cmd: &str, p: &Value, key: &str) -> Result<Option<NodeId>> {
    match get(p, key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => id_of(v).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a node id (got {})", shown(v)))),
    }
}

/// `key` when it is there: a whole, non-negative number, or an error. Missing means "none".
pub fn opt_index(cmd: &str, p: &Value, key: &str) -> Result<Option<usize>> {
    match get(p, key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => req_index(cmd, p, key).map(Some),
    }
}

/// `ids` (array), `id` (single), or the current selection.
pub fn targets(s: &Session, p: &Value) -> Vec<NodeId> {
    if let Some(Value::Array(a)) = get(p, "ids") {
        return a.iter().filter_map(id_of).collect();
    }
    if let Some(i) = id(p, "id") {
        return vec![i];
    }
    s.selection()
}

/// Like [`targets`] but an unparseable `ids` entry, an unparseable `id`, or an empty result is an
/// error: naming something that isn't a layer is a mistake, not an empty selection. An explicit
/// `null` says the same as leaving the key out.
pub fn req_targets(cmd: &str, s: &Session, p: &Value) -> Result<Vec<NodeId>> {
    match get(p, "ids") {
        Some(Value::Array(a)) => {
            if let Some(v) = a.iter().find(|v| id_of(v).is_none()) {
                return Err(bad(cmd, format!("`ids`: every entry must be a node id (got {})", shown(v))));
            }
        }
        Some(Value::Null) | None => {
            if let Some(v) = get(p, "id").filter(|v| !v.is_null())
                && id_of(v).is_none()
            {
                return Err(bad(cmd, format!("`id` must be a node id (got {})", shown(v))));
            }
        }
        Some(v) => return Err(bad(cmd, format!("`ids` must be an array (got {})", shown(v)))),
    }
    let t = targets(s, p);
    if t.is_empty() {
        return Err(EngineError::Disabled(cmd.to_string(), "nothing is selected (pass `ids`)".into()));
    }
    if let Ok(doc) = s.doc() {
        for i in &t {
            if !doc.contains(*i) {
                return Err(EngineError::NoNode(*i));
            }
        }
    }
    Ok(t)
}

pub use figcraft_automation_json::color_of;

pub fn color(p: &Value, key: &str) -> Option<Color> {
    get(p, key).and_then(color_of)
}

pub use figcraft_automation_json::normalize_paint;

/// A paint from JSON: a colour string/object (solid), a full paint object or its shorthand
/// ([`normalize_paint`]).
pub fn paint_of(v: &Value) -> Option<Paint> {
    if let Some(c) = color_of(v) {
        return Some(Paint::solid(c));
    }
    let mut v = v.clone();
    normalize_paint(&mut v);
    let mut p: Paint = serde_json::from_value(v).ok()?;
    p.opacity = if p.opacity.is_finite() { p.opacity.clamp(0.0, 1.0) } else { 1.0 };
    Some(p)
}

pub fn paints_of(v: &Value) -> Option<Vec<Paint>> {
    match v {
        Value::Array(a) => a.iter().map(paint_of).collect(),
        other => paint_of(other).map(|p| vec![p]),
    }
}

pub fn blend(p: &Value, key: &str) -> Option<BlendMode> {
    string(p, key).and_then(|s| BlendMode::from_name(&s))
}

/// Serde-deserialize `key` leniently into `T`.
pub fn parse<T: serde::de::DeserializeOwned>(cmd: &str, p: &Value, key: &str) -> Result<Option<T>> {
    match get(p, key) {
        None => Ok(None),
        Some(v) => serde_json::from_value(v.clone()).map(Some).map_err(|e| bad(cmd, format!("`{key}`: {e}"))),
    }
}

#[cfg(test)]
mod color_of_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn list_indices_refuse_boolean_coercion_without_changing_numeric_parameters() {
        for value in [json!(true), json!(false)] {
            assert!(req_index("test", &json!({"index": value}), "index").is_err(), "a boolean cannot name a list position");
        }
        assert_eq!(req_index("test", &json!({"index": 0}), "index").unwrap(), 0);
        assert_eq!(req_index("test", &json!({"index": 1}), "index").unwrap(), 1);
        assert_eq!(req_index("test", &json!({"index": "1"}), "index").unwrap(), 1);
        assert_eq!(req_num("test", &json!({"value": true}), "value").unwrap(), 1.0);
        assert_eq!(req_num("test", &json!({"value": false}), "value").unwrap(), 0.0);
    }

    #[test]
    fn color_of_rejects_non_numeric_array_entries() {
        assert_eq!(color_of(&json!([1, "x", 0, 0])), None);
        assert_eq!(color_of(&json!([1, 0, 0, "x"])), None);
        assert_eq!(color_of(&json!([1, 0, 0, 0.5])), Some(Color::new(1.0, 0.0, 0.0, 0.5)));
    }
}
