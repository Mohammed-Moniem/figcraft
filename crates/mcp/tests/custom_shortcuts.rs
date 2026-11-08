#![allow(clippy::unwrap_used)]

use figcraft_engine::keymap::Platform;
use figcraft_mcp::{Backend, Headless};
use serde_json::json;

#[test]
fn custom_shortcuts_headless_key_and_command_catalog_share_saved_overrides() {
    let mut h = Headless::new();
    h.session.execute("pref.shortcut", &json!({"id": "tool.rectangle", "keys": ["Alt+F8"]})).unwrap();
    let commands = h.call("engine.commands", json!({})).unwrap();
    assert_eq!(commands.as_array().unwrap().iter().find(|c| c["id"] == "tool.rectangle").unwrap()["shortcut"], "Alt+F8");
    let result = h.call("ui.key", json!({"key": "F8", "alt": true})).unwrap();
    assert_eq!(result["command"], "tool.rectangle");
    assert_eq!(h.session.tool.tool.id(), "rectangle");
    h.session.execute("tool.move", &json!({})).unwrap();
    assert!(h.call("ui.key", json!({"key": "R"})).is_err());
    assert_eq!(h.session.tool.tool.id(), "move");
    h.session.execute("pref.resetShortcut", &json!({"id": "tool.rectangle"})).unwrap();
    assert_eq!(h.session.prefs.shortcuts.primary("tool.rectangle", Platform::current()), Some("R"));
    assert_eq!(h.call("ui.key", json!({"key": "R"})).unwrap()["command"], "tool.rectangle");
}

#[test]
fn custom_shortcuts_headless_canvas_bindings_do_not_escape_text_editing() {
    let mut h = Headless::new();
    let text = h.session.execute("text.create", &json!({"text": "Hello"})).unwrap()["id"].clone();
    h.session.execute("select.set", &json!({"ids": [text]})).unwrap();
    h.session.execute("text.edit.begin", &json!({"id": text})).unwrap();
    h.session.execute("pref.shortcut", &json!({"id": "tool.rectangle", "keys": ["Cmd+R"], "replace": true})).unwrap();
    let result = h.call("ui.key", json!({"key": "R", "cmd": true})).unwrap();
    assert_eq!(result["textEdit"], true);
    assert_eq!(h.session.tool.tool.id(), "move");
    assert!(h.session.ds().unwrap().text_edit.is_some());
    h.session.execute("pref.shortcut", &json!({"id": "text.underline", "keys": ["Alt+F9"]})).unwrap();
    assert_eq!(h.call("ui.key", json!({"key": "F9", "alt": true})).unwrap()["command"], "text.underline");
    assert!(h.session.ds().unwrap().text_edit.is_some());
    assert_eq!(h.call("ui.key", json!({"key": "C", "cmd": true})).unwrap()["command"], "edit.copy");
    h.session.execute("pref.shortcut", &json!({"id": "edit.undo", "keys": ["Alt+F8"]})).unwrap();
    assert_eq!(h.call("ui.key", json!({"key": "Z", "cmd": true})).unwrap()["command"], "edit.undo", "native editing fallback remains canonical");
}
