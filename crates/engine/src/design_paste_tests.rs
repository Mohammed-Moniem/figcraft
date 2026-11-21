//! Paste of layers copied from another design app: the clipboard's design data (a `.fig` canvas in
//! its HTML) read through the `.fig` import and pasted as Paste pastes our own layers.

use base64::Engine as _;
use serde_json::json;

use super::{NodeId, Session, Value, params};
use crate::cmd::edit::{CLIP_CLOSE, CLIP_OPEN, design_clipboard_buffer};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn id(v: &Value) -> NodeId {
    params::id_of(&v["id"]).unwrap()
}

fn copied_file() -> Vec<u8> {
    let mut s = Session::new();
    let frame = id(&s.execute("frame.create", &json!({"x": 0, "y": 0, "width": 200, "height": 100, "name": "Card"})).unwrap());
    s.execute("node.create", &json!({"type": "rectangle", "x": 10, "y": 10, "width": 40, "height": 20, "parent": frame, "name": "Badge"})).unwrap();
    s.execute("node.create", &json!({"type": "ellipse", "x": 300, "y": 0, "width": 50, "height": 50, "name": "Dot"})).unwrap();
    let r = s.execute("file.export", &json!({"format": "fig"})).unwrap();
    B64.decode(r["data"].as_str().unwrap()).unwrap()
}

fn clipboard_html(buffer: &[u8]) -> String {
    let mut wrapped = B64.encode(buffer);
    wrapped.insert(8, '\n');
    format!(r#"<meta charset="utf-8"><span data-buffer="<!--{CLIP_OPEN}{wrapped}{CLIP_CLOSE}-->"></span>"#)
}

fn names(s: &Session, ids: &[NodeId]) -> Vec<String> {
    let doc = s.doc().unwrap();
    ids.iter().map(|i| doc.node(*i).unwrap().name.clone()).collect()
}

#[test]
fn buffer_is_found_in_the_html() {
    let file = copied_file();
    assert_eq!(design_clipboard_buffer(&clipboard_html(&file)), Some(file));
    assert_eq!(design_clipboard_buffer("<p>plain</p>"), None);
    assert_eq!(design_clipboard_buffer(&format!("{CLIP_OPEN}not base64!{CLIP_CLOSE}")), None);
    assert_eq!(design_clipboard_buffer(&format!("{CLIP_OPEN}{}{CLIP_CLOSE}", B64.encode(b"hello"))), None);
    assert_eq!(design_clipboard_buffer(&format!("{CLIP_OPEN}ZmlnLQ")), None);
    // Web text that only mentions the markers holds no design data.
    assert_eq!(design_clipboard_buffer(&format!("<p>see {CLIP_OPEN} and {CLIP_CLOSE} in the docs</p>")), None);
}

#[test]
fn oversized_design_data_is_refused_before_decoding() {
    // Just past the cap (256 MB of data, as base64), made of valid base64 characters.
    let big = "A".repeat((256 * 1024 * 1024usize).div_ceil(3) * 4 + 4);
    assert_eq!(design_clipboard_buffer(&format!("{CLIP_OPEN}{big}{CLIP_CLOSE}")), None);
    let mut s = Session::new();
    let e = s.execute("edit.pasteDesignLayers", &json!({ "data": big })).unwrap_err();
    assert!(e.to_string().contains("256 MB"), "{e}");
    assert!(s.clipboard.nodes.is_empty());
}

fn bound_clipboard() -> Vec<u8> {
    let mut s = Session::new();
    let frame = id(&s.execute("node.create", &json!({"type":"frame","name":"Token frame"})).unwrap());
    s.execute("variable.createCollection", &json!({"name":"Theme","modes":["Light","Dark"]})).unwrap();
    s.execute("variable.create", &json!({"name":"Surface","collection":"Theme","type":"color","values":{"Light":"#112233","Dark":"#ABCDEF"}}))
        .unwrap();
    s.execute(
        "variable.create",
        &json!({"name":"Panel","collection":"Theme","type":"color","values":{"Light":{"alias":"Surface"},"Dark":{"alias":"Surface"}}}),
    )
    .unwrap();
    s.execute("variable.bind", &json!({"ids":[frame],"field":"fill","variable":"Panel"})).unwrap();
    s.execute("variable.setMode", &json!({"ids":[frame],"collection":"Theme","mode":"Dark"})).unwrap();
    let r = s.execute("file.export", &json!({"format": "fig"})).unwrap();
    B64.decode(r["data"].as_str().unwrap()).unwrap()
}

/// Figma sometimes sends layers with their GUID bindings but without the variable and collection
/// definitions. Remove those two asset kinds from an otherwise ordinary exported canvas.
fn clipboard_without_definitions(fig: &[u8]) -> Vec<u8> {
    use figcraft_format::fig::kiwi::{Kv, Schema, encode};
    use std::io::{Read as _, Write as _};
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(fig)).unwrap();
    let mut canvas = Vec::new();
    zip.by_name("canvas.fig").unwrap().read_to_end(&mut canvas).unwrap();
    let mut next = &canvas[12..];
    let mut inflate = || {
        let len = u32::from_le_bytes(next[..4].try_into().unwrap()) as usize;
        let mut bytes = Vec::new();
        flate2::read::DeflateDecoder::new(&next[4..4 + len]).read_to_end(&mut bytes).unwrap();
        next = &next[4 + len..];
        bytes
    };
    let schema_bytes = inflate();
    let schema = Schema::decode(&schema_bytes).unwrap();
    let message = schema.find("Message").unwrap();
    let Kv::Object(mut fields) = schema.decode_message(message, &inflate()).unwrap() else { panic!("message object") };
    let Kv::Array(changes) = &mut fields.iter_mut().find(|(key, _)| *key == "nodeChanges").unwrap().1 else { panic!("node changes") };
    changes.retain(|n| !matches!(n.get("type").and_then(Kv::str), Some("VARIABLE" | "VARIABLE_SET")));
    let message_bytes = encode::message(&schema, message, &Kv::Object(fields)).unwrap();
    let mut result = canvas[..12].to_vec();
    for chunk in [schema_bytes, message_bytes] {
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&chunk).unwrap();
        let packed = z.finish().unwrap();
        result.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        result.extend(packed);
    }
    result
}

#[test]
fn design_paste_keeps_resolved_fills_when_the_clipboard_omits_variables() {
    let full = bound_clipboard();
    let partial = clipboard_without_definitions(&full);
    let (decoded, _) = figcraft_format::fig::try_import(&partial, "Clipboard").unwrap().unwrap();
    let original = decoded.nodes().find(|n| n.name == "Token frame").unwrap();
    assert!(decoded.variables.variables.is_empty());
    assert!(!original.fills.is_empty());
    let mut s = Session::new();
    let (complete, _) = figcraft_format::fig::try_import(&full, "Clipboard").unwrap().unwrap();
    let with_definitions = complete.nodes().find(|n| n.name == "Token frame").unwrap();
    assert!(!with_definitions.bound.is_empty() && !with_definitions.var_modes.is_empty());
    s.doc_mut().unwrap().variables = complete.variables.clone();
    for var in &mut s.doc_mut().unwrap().variables.variables {
        var.name = format!("Unrelated {}", var.name);
    }
    let before = s.doc().unwrap().variables.clone();
    let result = s.execute("edit.pasteDesignLayers", &json!({"html":clipboard_html(&partial)})).unwrap();
    let layer = s.doc().unwrap().node(params::id_of(&result["ids"][0]).unwrap()).unwrap();
    assert!(layer.bound.is_empty() && layer.var_modes.is_empty());
    assert_eq!(layer.fills, original.fills);
    assert_eq!(s.doc().unwrap().variables, before);
    // Some clipboard snapshots have retained GUIDs even after the definitions disappeared.
    // A repeated Paste must check the copied source, even when the target owns these GUIDs.
    let copied = s.clipboard.nodes.iter_mut().flat_map(|(_, nodes)| nodes).find(|n| n.name == "Token frame").unwrap();
    copied.bound.clone_from(&with_definitions.bound);
    copied.var_modes.clone_from(&with_definitions.var_modes);
    let again = s.execute("edit.paste", &json!({"parent":s.page().unwrap()})).unwrap();
    let second = s.doc().unwrap().node(params::id_of(&again["ids"][0]).unwrap()).unwrap();
    assert!(second.bound.is_empty() && second.var_modes.is_empty());
    assert_eq!(second.fills, original.fills);
}

#[test]
fn design_paste_imports_the_bound_variable_alias_and_mode_without_overwriting_destination_tokens() {
    let full = bound_clipboard();
    let (decoded, _) = figcraft_format::fig::try_import(&full, "Clipboard").unwrap().unwrap();
    let original = decoded.nodes().find(|n| n.name == "Token frame").unwrap();
    let mut s = Session::new();
    s.doc_mut().unwrap().variables = decoded.variables.clone();
    for var in &mut s.doc_mut().unwrap().variables.variables {
        var.name = format!("Destination {}", var.name);
    }
    let before = s.doc().unwrap().variables.clone();
    let pasted = s.execute("edit.pasteDesignLayers", &json!({"html":clipboard_html(&full)})).unwrap();
    let doc = s.doc().unwrap();
    let layer = doc.node(params::id_of(&pasted["ids"][0]).unwrap()).unwrap();
    let binding = layer.bound.values().next().unwrap();
    assert_ne!(binding, original.bound.values().next().unwrap());
    let var = doc.variables.get_by_id(binding).unwrap();
    assert_eq!(var.name, "Panel");
    let chosen = &layer.var_modes[&var.collection];
    let coll = doc.variables.collection_by_id(&var.collection).unwrap();
    assert_eq!(coll.mode_by_id(chosen).unwrap().name, "Dark");
    let refs = var.values[chosen].references();
    assert_eq!(doc.variables.get_by_id(refs[0]).unwrap().name, "Surface");
    assert_eq!(
        doc.variables.resolve(binding, &layer.var_modes),
        decoded.variables.resolve(original.bound.values().next().unwrap(), &original.var_modes)
    );
    assert_eq!(layer.fills, original.fills);
    for old in &before.variables {
        assert_eq!(doc.variables.get_by_id(&old.id), Some(old));
    }
    let imported = doc.variables.clone();
    let page = s.page().unwrap();
    s.execute("edit.paste", &json!({"parent":page})).unwrap();
    assert_eq!(s.doc().unwrap().variables, imported, "repeated paste reuses token definitions");
    s.undo().unwrap();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap().variables, before);
}

#[test]
fn layers_paste_into_the_page_and_undo() {
    let mut s = Session::new();
    let page = s.page().unwrap();
    let r = s.execute("edit.pasteDesignLayers", &json!({"html": clipboard_html(&copied_file())})).unwrap();
    let ids: Vec<NodeId> = r["ids"].as_array().unwrap().iter().map(|v| params::id_of(v).unwrap()).collect();
    assert_eq!(names(&s, &ids), ["Card", "Dot"]);
    assert_eq!(s.selection(), ids);
    let doc = s.doc().unwrap();
    assert!(ids.iter().all(|i| doc.node(*i).unwrap().parent == Some(page)));
    let card = doc.node(ids[0]).unwrap();
    assert_eq!(names(&s, &card.children), ["Badge"]);
    assert_eq!((card.width, card.height), (200.0, 100.0));
    assert!(s.undo().unwrap());
    assert!(ids.iter().all(|i| !s.doc().unwrap().contains(*i)));
}

#[test]
fn a_second_paste_repeats_the_copied_layers() {
    let mut s = Session::new();
    // `data` may be wrapped and unpadded, like the HTML's base64.
    let mut data = B64.encode(copied_file()).trim_end_matches('=').to_string();
    data.insert(8, '\n');
    s.execute("edit.pasteDesignLayers", &json!({ "data": data })).unwrap();
    let r = s.execute("edit.paste", &json!({})).unwrap();
    let ids: Vec<NodeId> = r["ids"].as_array().unwrap().iter().map(|v| params::id_of(v).unwrap()).collect();
    assert_eq!(names(&s, &ids), ["Card", "Dot"]);
}

#[test]
fn pastes_into_the_selected_frame() {
    let mut s = Session::new();
    let host = id(&s.execute("frame.create", &json!({"x": 1000, "y": 0, "width": 800, "height": 600})).unwrap());
    s.execute("select.set", &json!({"ids": [host]})).unwrap();
    let r = s.execute("edit.pasteDesignLayers", &json!({"html": clipboard_html(&copied_file())})).unwrap();
    let first = params::id_of(&r["ids"][0]).unwrap();
    assert_eq!(s.doc().unwrap().node(first).unwrap().parent, Some(host));
}

#[test]
fn other_content_is_refused_without_changes() {
    let mut s = Session::new();
    let before = s.doc().unwrap().nodes().count();
    assert!(s.execute("edit.pasteDesignLayers", &json!({})).is_err());
    assert!(s.execute("edit.pasteDesignLayers", &json!({"html": "<b>hi</b>"})).is_err());
    assert!(s.execute("edit.pasteDesignLayers", &json!({"data": B64.encode(b"PK not a zip")})).is_err());
    assert!(s.execute("edit.pasteDesignLayers", &json!({"data": B64.encode(b"fig-kiwi\x00\x00\x00\x00junk")})).is_err());
    assert_eq!(s.doc().unwrap().nodes().count(), before);
    assert!(s.clipboard.nodes.is_empty());
}
