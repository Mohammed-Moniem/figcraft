//! Saved layer-list highlight colors. Defaults follow the active theme; explicit colors are
//! ordinary document properties, so duplication, history, collaboration and native save agree.
use super::CommandSpec;
use crate::params::{boolean, req_targets};
use crate::{Result, Session, bad, spec};
use figcraft_color::Color;
use figcraft_doc::{Document, NodeId, NodeKind};
use serde_json::{Value, json};

pub fn is_main_level(doc: &Document, id: NodeId) -> bool {
    doc.node(id).is_some_and(|n| {
        n.parent.and_then(|p| doc.node(p)).is_some_and(|p| p.is_page())
            && (n.kind.is_frame_like() || matches!(n.kind, NodeKind::Section { .. } | NodeKind::Group))
    })
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "layer.setHighlight",
            "Set layer highlight",
            "{ids?, color: CSS color|null, allMain?} (saved layer-list color; allMain targets existing top-level frames, sections and groups on this page)",
            true,
            set
        ),
        spec!(
            "layer.resetHighlight",
            "Reset layer highlight",
            "{ids?, allMain?} (restore theme defaults for the selected layers or this page's existing main frames)",
            true,
            reset
        ),
    ]
}

fn targets(s: &Session, p: &Value, cmd: &str) -> Result<Vec<NodeId>> {
    if p.get("allMain").is_some_and(|v| !v.is_boolean()) {
        return Err(bad(cmd, "`allMain` must be a boolean"));
    }
    if boolean(p, "allMain").unwrap_or(false) {
        if p.get("ids").is_some() {
            return Err(bad(cmd, "`ids` and `allMain` cannot be combined"));
        }
        let page = s.page()?;
        let doc = s.doc()?;
        return Ok(doc.node(page).map(|n| n.children.iter().copied().filter(|id| is_main_level(doc, *id)).collect()).unwrap_or_default());
    }
    let ids = req_targets(cmd, s, p)?;
    for id in &ids {
        let n = s.doc()?.get(*id)?;
        if n.is_page() {
            return Err(bad(cmd, "pages have no layer-list highlight"));
        }
    }
    Ok(ids)
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "layer.setHighlight";
    let color = match p.get("color") {
        Some(Value::Null) => None,
        Some(Value::String(s)) => Some(Color::from_hex_or_name(s).ok_or_else(|| bad(cmd, "`color` must be a CSS color or null"))?),
        _ => return Err(bad(cmd, "`color` is required: a CSS color or null")),
    };
    apply(s, p, cmd, color)
}
fn reset(s: &mut Session, p: &Value) -> Result<Value> {
    apply(s, p, "layer.resetHighlight", None)
}
fn apply(s: &mut Session, p: &Value, cmd: &str, color: Option<Color>) -> Result<Value> {
    let ids = targets(s, p, cmd)?;
    let doc = s.doc_mut()?;
    for id in &ids {
        doc.get_mut(*id)?.highlight_color = color;
    }
    Ok(json!({"ids":ids,"color":color.map(|c|c.to_css_hex())}))
}
