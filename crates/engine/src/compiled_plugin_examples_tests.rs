//! Execute the real Rust-built release WASM modules through the metered production engine.
use crate::{Session, plugins_tests::install};
use serde_json::{Value, json};

fn setup() -> Session {
    let mut s = Session::new();
    for (manifest, wasm) in [
        (include_str!("../../../plugins/examples/counter/figcraft-plugin.json"), include_bytes!("../../../plugins/builtin/counter.wasm").as_slice()),
        (
            include_str!("../../../plugins/examples/focus-list/figcraft-plugin.json"),
            include_bytes!("../../../plugins/builtin/focus_list.wasm").as_slice(),
        ),
        (
            include_str!("../../../plugins/examples/dot-grid/figcraft-plugin.json"),
            include_bytes!("../../../plugins/builtin/dot_grid.wasm").as_slice(),
        ),
        (
            include_str!("../../../plugins/examples/tidy-names/figcraft-plugin.json"),
            include_bytes!("../../../plugins/builtin/tidy_names.wasm").as_slice(),
        ),
        (
            include_str!("../../../plugins/examples/swatch-grid/figcraft-plugin.json"),
            include_bytes!("../../../plugins/builtin/swatch_grid.wasm").as_slice(),
        ),
        (
            include_str!("../../../plugins/examples/export-names/figcraft-plugin.json"),
            include_bytes!("../../../plugins/builtin/export_names.wasm").as_slice(),
        ),
    ] {
        install(&mut s, serde_json::from_str(manifest).unwrap(), wasm);
    }
    s
}
fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

#[test]
fn compiled_counter_keeps_legacy_fields_panel_svg_line_and_adds_step_five() {
    let mut s = setup();
    let w = run(&mut s, "widget.insert", json!({"plugin":"org.figcraft.examples.counter"}))["id"].clone();
    run(&mut s, "widget.setText", json!({"id":w,"field":"label","value":"Scores"}));
    run(&mut s, "widget.menuAction", json!({"id":w,"item":"step","value":"5"}));
    let out = run(&mut s, "widget.click", json!({"id":w,"target":"increment"}));
    assert_eq!(out["state"]["count"], 5);
    assert_eq!(out["state"]["label"], "Scores");
    run(&mut s, "widget.menu", json!({"id":w,"item":"set"}));
    assert!(s.plugins.panel("org.figcraft.examples.counter").is_some());
    run(&mut s, "plugin.event", json!({"id":"org.figcraft.examples.counter","control":"count","value":42}));
    run(&mut s, "plugin.event", json!({"id":"org.figcraft.examples.counter","control":"set"}));
    let list = run(&mut s, "widget.list", json!({}));
    assert_eq!(list["widgets"][0]["state"]["count"], 42);
    assert!(s.plugins.panel("org.figcraft.examples.counter").is_none());
    assert!(s.doc().unwrap().ids().any(|id| s.doc().unwrap().node(id).is_some_and(|n| matches!(n.kind, figcraft_doc::NodeKind::Line))));
}

#[test]
fn compiled_focus_list_edits_checks_ticks_cancels_undoes_and_saves_without_reviving_timers() {
    let mut s = setup();
    let w = run(&mut s, "widget.insert", json!({"plugin":"org.figcraft.examples.focus-list"}))["id"].clone();
    run(&mut s, "widget.setText", json!({"id":w,"input":"title","text":"Deep work"}));
    run(&mut s, "widget.setText", json!({"id":w,"input":"new","text":"Write a test"}));
    let checked = run(&mut s, "widget.click", json!({"id":w,"target":"toggle.0"}));
    assert_eq!(checked["state"]["items"][0]["done"], true);
    let started = run(&mut s, "widget.click", json!({"id":w,"target":"focus"}));
    let left = started["state"]["left"].as_u64().unwrap();
    assert!(left > 0 && crate::widgets::next_timer(&s).is_some());
    let tick = run(&mut s, "widget.timer", json!({"id":w}));
    assert_eq!(tick["state"]["left"], left - 1);
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (doc, _) = figcraft_format::load_bytes(&bytes).unwrap();
    let mut reopened = Session::with_document(doc);
    assert!(crate::widgets::next_timer(&reopened).is_none());
    let saved = run(&mut reopened, "widget.list", json!({}));
    assert_eq!(saved["widgets"][0]["state"]["title"], "Deep work");
    assert!(saved["widgets"][0]["state"]["items"].as_array().unwrap().iter().any(|i| i["text"] == "Write a test"));
    run(&mut s, "edit.undo", json!({}));
    let list = run(&mut s, "widget.list", json!({}));
    assert_eq!(list["widgets"][0]["state"]["left"], left);
}

#[test]
fn compiled_dot_grid_panel_updates_preview_and_generates_exact_geometry_as_one_step() {
    let mut s = setup();
    let p = "org.figcraft.examples.dot-grid";
    run(&mut s, "plugin.run", json!({"id":p,"command":"panel"}));
    let before = s.plugins.panel(p).unwrap().panel.clone();
    run(&mut s, "plugin.event", json!({"id":p,"control":"columns","value":2}));
    let after = s.plugins.panel(p).unwrap().panel.clone();
    let image = |panel: &figcraft_plugin::Panel| panel.controls.iter().find(|c| c.kind == figcraft_plugin::ParamKind::Image).unwrap().data.clone();
    assert_ne!(image(&before), image(&after));
    let undo = s.ds().unwrap().history.undo.len();
    let out = run(&mut s, "plugin.event", json!({"id":p,"control":"generate"}));
    assert_eq!(out["output"]["dots"], 4);
    let group = s.selection()[0];
    assert_eq!(s.doc().unwrap().node(group).unwrap().children.len(), 4);
    assert_eq!(s.ds().unwrap().history.undo.len(), undo + 1);
    run(&mut s, "edit.undo", json!({}));
    assert!(!s.doc().unwrap().contains(group));
}

#[test]
fn compiled_older_examples_and_offline_directory_remain_available() {
    let mut s = setup();
    let shape = run(&mut s, "shape.rectangle", json!({"width":20,"height":20,"props":{"name":"  test  layer copy "}}))["id"].clone();
    run(&mut s, "select.set", json!({"ids":[shape]}));
    run(&mut s, "plugin.run", json!({"id":"org.figcraft.examples.tidy-names","command":"tidy","params":{"case":"Title Case"}}));
    run(&mut s, "plugin.run", json!({"id":"org.figcraft.examples.swatch-grid","command":"panel"}));
    assert!(s.plugins.panel("org.figcraft.examples.swatch-grid").is_some());
    run(&mut s, "plugin.run", json!({"id":"org.figcraft.examples.export-names","command":"export"}));
    assert!(s.ui_requests.iter().any(|r| r.kind == "dialog.pluginSave"));
    let out = run(&mut s, "plugin.directory.list", json!({"server":"builtin"}));
    let plugins = out["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 6);
}
