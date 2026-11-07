//! Product contracts for complete local workspace behavior.
use crate::{Session, doc::NodeId};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
fn document_events(s: &Session, after: u64) -> Vec<crate::events::Event> {
    s.events.poll(after, &["document".into()], 1000, Duration::ZERO).events
}
fn install(s: &mut Session, id: &str) {
    s.execute("plugin.directory.install", &json!({"id":format!("org.figcraft.examples.{id}"),"server":"builtin","confirm":true})).unwrap();
}
#[test]
fn real_builtin_tidy_changes_the_document_and_undoes_once() {
    let mut s = Session::new();
    let id = s.execute("shape.rectangle", &json!({"props":{"name":"  Card   title copy 2 "}})).unwrap()["id"].clone();
    install(&mut s, "tidy-names");
    let before = s.undo_depth().0;
    let out = s.execute("plugin.run", &json!({"id":"org.figcraft.examples.tidy-names","command":"quick"})).unwrap();
    assert_eq!(out["output"]["renamed"], 1);
    assert_eq!(s.doc().unwrap().node(NodeId(id.as_u64().unwrap())).unwrap().name, "Card title");
    assert_eq!(s.undo_depth().0, before + 1);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap().node(NodeId(id.as_u64().unwrap())).unwrap().name, "  Card   title copy 2 ");
}
#[test]
fn real_builtin_grid_counter_and_exporter_perform_their_advertised_work() {
    let mut s = Session::new();
    install(&mut s, "swatch-grid");
    let grid = s
        .execute(
            "plugin.run",
            &json!({"id":"org.figcraft.examples.swatch-grid","command":"grid","params":{"palette":"#123456,#abcdef","columns":2,"labels":false}}),
        )
        .unwrap();
    assert_eq!(grid["output"]["swatches"], 2);
    install(&mut s, "counter");
    let widget = s.execute("widget.insert", &json!({"plugin":"org.figcraft.examples.counter"})).unwrap();
    let id = widget["id"].clone();
    assert_eq!(widget["state"]["count"], 0);
    let clicked = s.execute("widget.click", &json!({"id":id,"target":"increment"})).unwrap();
    assert_eq!(clicked["state"]["count"], 1);
    install(&mut s, "export-names");
    s.execute("select.set", &json!({"ids":grid["output"]["ids"]})).unwrap();
    let exported = s.execute("plugin.run", &json!({"id":"org.figcraft.examples.export-names","command":"export"})).unwrap();
    assert_eq!(exported["output"]["names"], 2);
    let save = s.ui_requests.iter().find(|r| r.kind == "dialog.pluginSave").unwrap();
    assert_eq!(save.data["name"], "layer-names.txt");
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(save.data["data"].as_str().unwrap()).unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap(), "#123456\n#ABCDEF\n");
}
#[test]
fn unsaved_events_observe_commands_gestures_direct_text_and_cancelled_work() {
    let mut s = Session::new();
    let node = s.execute("shape.rectangle", &json!({"x":1,"y":2})).unwrap()["id"].as_u64().unwrap();
    let before = s.events.latest();
    s.begin_interaction("Drag").unwrap();
    s.preview(|doc, _| {
        doc.node_mut(NodeId(node)).unwrap().name = "Preview".into();
        Ok(())
    })
    .unwrap();
    s.observe_document_changes();
    assert!(document_events(&s, before).is_empty());
    s.cancel_interaction().unwrap();
    s.observe_document_changes();
    assert!(document_events(&s, before).is_empty());
    s.begin_interaction("Drag").unwrap();
    s.preview(|doc, _| {
        doc.node_mut(NodeId(node)).unwrap().name = "Committed".into();
        Ok(())
    })
    .unwrap();
    s.commit_interaction().unwrap();
    s.observe_document_changes();
    let events = document_events(&s, before);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data["document"], s.ds().unwrap().uid);
    assert_eq!(events[0].data["dirty"], true);
    let before = s.events.latest();
    s.ds_mut().unwrap().doc_mut().node_mut(NodeId(node)).unwrap().name = "Direct committed edit".into();
    s.observe_document_changes();
    assert_eq!(document_events(&s, before).len(), 1);
    let before = s.events.latest();
    s.execute("doc.list", &json!({})).unwrap();
    s.execute("node.get", &json!({"id":node})).unwrap();
    assert!(document_events(&s, before).is_empty());
    let before = s.events.latest();
    let result: crate::Result<()> = s.grouped("rollback", |s| {
        s.execute("object.rename", &json!({"ids":[node],"name":"lost"}))?;
        Err(crate::EngineError::Other("rollback".into()))
    });
    assert!(result.is_err());
    assert!(document_events(&s, before).is_empty());
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(!document_events(&s, before).is_empty());
}
#[test]
fn event_overflow_and_payload_bounds_require_resynchronization() {
    let log = crate::events::EventLog::default();
    for _ in 0..crate::events::MAX_EVENTS + 2 {
        log.push("document.changed", "Doc", None, json!({"document":1}));
    }
    let result = log.poll(1, &["document".into()], 1000, Duration::ZERO);
    assert!(result.reset && result.dropped > 0);
    assert!(result.events.len() <= 1000);
    log.push("document.changed", &"🧪".repeat(10000), Some(&"/".repeat(10000)), json!({"oversized":"x".repeat(20000)}));
    let result = log.poll(log.latest() - 1, &["document".into()], 1, Duration::ZERO);
    let event = &result.events[0];
    assert_eq!(event.data["reset"], true);
    assert!(event.document.len() <= 512 && event.path.as_ref().unwrap().len() <= 4096);
}
#[test]
fn large_real_edit_uses_reset_instead_of_unbounded_changed_ids() {
    let mut s = Session::new();
    let before = s.events.latest();
    s.grouped("large", |s| {
        for i in 0..260 {
            s.execute("shape.rectangle", &json!({"x":i,"y":i}))?;
        }
        Ok(())
    })
    .unwrap();
    let events = document_events(&s, before);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data["reset"], true);
    assert_eq!(events[0].data["changedIds"], json!([]));
}
#[test]
fn external_refresh_keeps_clean_tab_identity_and_refuses_dirty_overwrite() {
    let dir = std::env::temp_dir().join(format!("figcraft-leaf-refresh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("shared.figcraft");
    let text = path.to_string_lossy().to_string();
    let mut writer = Session::new();
    writer.execute("file.saveAs", &json!({"path":text})).unwrap();
    let mut reader = Session::empty();
    reader.execute("file.open", &json!({"path":text})).unwrap();
    let uid = reader.ds().unwrap().uid;
    reader.ds_mut().unwrap().view.zoom = 2.0;
    writer.execute("file.rename", &json!({"name":"External"})).unwrap();
    writer.execute("file.save", &json!({})).unwrap();
    let refresh = reader.execute("file.refresh", &json!({})).unwrap();
    assert_eq!(refresh["refreshed"], json!([uid]));
    assert_eq!(reader.doc().unwrap().name, "External");
    assert_eq!(reader.ds().unwrap().uid, uid);
    assert_eq!(reader.ds().unwrap().view.zoom, 2.0);
    assert!(!reader.ds().unwrap().is_dirty());
    reader.execute("shape.rectangle", &json!({})).unwrap();
    writer.execute("file.rename", &json!({"name":"Newer external"})).unwrap();
    writer.execute("file.save", &json!({})).unwrap();
    let disk = std::fs::read(&path).unwrap();
    let local = Arc::clone(&reader.ds().unwrap().doc);
    assert_eq!(reader.execute("file.refresh", &json!({})).unwrap()["refreshed"], json!([]));
    assert_eq!(reader.ds().unwrap().doc, local);
    assert!(reader.execute("file.save", &json!({"path":text})).is_err());
    assert!(reader.execute("file.saveAs", &json!({"path":text})).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), disk);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn project_ids_wrap_without_hanging_and_full_library_refuses_new_favorites() {
    let mut p = crate::projects::Projects { next_id: u64::MAX, ..Default::default() };
    let first = p.create_folder("Last", None, 0).unwrap();
    let second = p.create_folder("Next", None, 0).unwrap();
    assert_ne!(first, second);
    assert_eq!(second, "f1");
    p.files = (0..crate::projects::MAX_FILES).map(|i| crate::projects::ProjectFile { path: format!("file-{i}"), ..Default::default() }).collect();
    assert!(p.set_favorite(&crate::projects::Favorite::File("overflow".into()), true, 0).is_err());
    assert!(p.favorites.is_empty());
}
#[test]
fn malformed_event_options_are_refused_without_log_side_effects() {
    for p in [
        json!(null),
        json!({"after":-1}),
        json!({"limit":0}),
        json!({"limit":1001}),
        json!({"timeoutMs":1}),
        json!({"extra":true}),
        json!({"topics":"document"}),
    ] {
        assert!(crate::events::poll_options(&p, 0, 0).is_err(), "{p}");
    }
    let options = crate::events::poll_options(&json!({"after":2,"topics":["document"],"limit":100}), 0, 0).unwrap();
    assert_eq!(options.0, 2);
    assert_eq!(options.1, vec!["document"]);
}
#[test]
fn authenticated_snippets_include_only_credential_paths_and_quote_spaces() {
    use crate::harness::{self, HarnessKind};
    let path = std::path::Path::new("/private/tmp/My App's State/control-47800.token");
    let json: serde_json::Value =
        serde_json::from_str(&harness::snippet_with_token_file(HarnessKind::ClaudeCode, "/Applications/My Kit/figcraft-cli", 47800, path)).unwrap();
    assert_eq!(json["mcpServers"]["figcraft"]["args"][3], "--control-token-file");
    assert_eq!(json["mcpServers"]["figcraft"]["args"][4], path.to_string_lossy().as_ref());
    assert!(harness::attach_command_with_token_file("/Applications/My Kit/figcraft-cli", 47800, path).contains("'\\''"));
}
