//! Animated output is separate from static file.export settings.

use base64::Engine as _;
use figcraft_doc::NodeId;
use figcraft_render::animated_gif::{self, Options};
use serde_json::{Value, json};

use crate::{Result, Session, bad};

const CMD: &str = "motion.export";

fn integer(p: &Value, key: &str, fallback: u32) -> Result<u32> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(fallback),
        Some(v) => v.as_u64().and_then(|v| u32::try_from(v).ok()).ok_or_else(|| bad(CMD, format!("`{key}` must be an unsigned 32-bit integer"))),
    }
}

pub(super) fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let format = match p.get("format") {
        None | Some(Value::Null) => "gif",
        Some(Value::String(v)) if v.eq_ignore_ascii_case("gif") => "gif",
        _ => return Err(bad(CMD, "animated export currently supports `format: gif`")),
    };
    let frame = if p.get("id").is_some_and(|v| !v.is_null()) {
        crate::params::req_id(CMD, p, "id")?
    } else {
        let selected = s.selection();
        match selected.as_slice() {
            [id] => *id,
            _ => return Err(bad(CMD, "select one top-level frame or provide its `id`")),
        }
    };
    let doc = s.doc()?;
    let page = doc.page_of(frame).ok_or_else(|| bad(CMD, "export frame has no page"))?;
    let mut options = Options::from(doc.timelines.get(&page).copied().unwrap_or_default());
    options.duration_ms = integer(p, "duration", options.duration_ms)?;
    options.fps = integer(p, "fps", options.fps)?;
    options.looping = match p.get("loop") {
        None | Some(Value::Null) => options.looping,
        Some(Value::Bool(v)) => *v,
        _ => return Err(bad(CMD, "`loop` must be a boolean")),
    };
    options.scale = match p.get("scale") {
        None | Some(Value::Null) => 1.0,
        Some(v) => v.as_f64().filter(|v| v.is_finite()).ok_or_else(|| bad(CMD, "`scale` must be a finite number"))?,
    };
    let path = match p.get("path") {
        None | Some(Value::Null) => None,
        Some(Value::String(v)) if !v.is_empty() => Some(v),
        _ => return Err(bad(CMD, "`path` must be a nonempty string")),
    };
    if let Some(path) = path {
        crate::automation::file_access::allow(path, crate::automation::file_access::Access::Write)?;
    }
    let output = animated_gif::export(doc, frame, options).map_err(|e| bad(CMD, e))?;
    let mut result = json!({
        "format":format, "id":frame, "width":output.width, "height":output.height,
        "frames":output.frames, "duration":output.duration_ms, "fps":options.fps,
        "loop":options.looping, "bytes":output.bytes.len(), "warnings":output.warnings,
    });
    if let Some(path) = path {
        super::file::write(path, &output.bytes)?;
        result["path"] = json!(path);
    } else {
        result["data"] = json!(base64::engine::general_purpose::STANDARD.encode(output.bytes));
    }
    Ok(result)
}

/// Visible frames the Motion export dialog offers, in page paint order.
pub fn frames(doc: &figcraft_doc::Document, page: NodeId) -> Vec<NodeId> {
    doc.node(page)
        .map(|p| {
            p.children
                .iter()
                .copied()
                .filter(|id| doc.node(*id).is_some_and(|n| n.visible && matches!(n.kind, figcraft_doc::NodeKind::Frame)))
                .collect()
        })
        .unwrap_or_default()
}
