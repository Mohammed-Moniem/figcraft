//! Bulk variable metadata is exercised through actual row and popup input.
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use figcraft_engine::Session;
use serde_json::json;

use crate::layers_drag_tests::Ui;
use crate::live_tests::{flat, has_text, text_center};
use crate::variables_panel::Geom;

fn editor() -> Ui {
    let mut s = Session::new();
    s.execute("variable.createCollection", &json!({"name":"Tokens", "modes":["Light", "Dark"]})).unwrap();
    for (name, hidden, scopes) in
        [("Alpha", false, json!(["FRAME_FILL", "STROKE_COLOR"])), ("Beta", true, json!(["STROKE_COLOR"])), ("Gamma", false, json!([]))]
    {
        let v = s.execute("variable.create", &json!({"collection":"Tokens", "name":name, "type":"color", "value":"#0D99FF"})).unwrap();
        s.execute(
            "variable.update",
            &json!({"variable":v["id"], "description":format!("{name} description"), "hiddenFromPublishing":hidden,
            "scopes":scopes,"codeSyntax":{"WEB":format!("var(--{name})")}}),
        )
        .unwrap();
    }
    let mut u = Ui::new(s);
    u.app.ui.variables = Some(Default::default());
    u.ctx.enable_accesskit();
    for _ in 0..15 {
        u.frame(vec![]);
    }
    u
}

fn click(u: &mut Ui, p: Pos2, modifiers: Modifiers, button: PointerButton) {
    for pressed in [true, false] {
        u.frame_with(vec![Event::PointerMoved(p), Event::PointerButton { pos: p, button, pressed, modifiers }], modifiers);
    }
    u.frame(vec![]);
}

fn row(u: &mut Ui, name: &str) -> Pos2 {
    text_center(&flat(&u.frame_output(vec![]).shapes), name)
}

fn select_pair(u: &mut Ui) {
    let a = row(u, "Alpha");
    click(u, a, Modifiers::NONE, PointerButton::Primary);
    let b = row(u, "Beta");
    click(u, b, Modifiers::COMMAND, PointerButton::Primary);
}

fn open_bulk(u: &mut Ui) {
    let b = row(u, "Beta");
    click(u, b, Modifiers::NONE, PointerButton::Secondary);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&shapes, "Edit variables"), "multi-selection must offer a shared editor");
    click(u, text_center(&shapes, "Edit variables"), Modifiers::NONE, PointerButton::Primary);
    for _ in 0..12 {
        u.frame(vec![]);
    }
    assert!(has_text(&flat(&u.frame_output(vec![]).shapes), "Edit variables"));
}

fn key(u: &mut Ui, key: Key) {
    for pressed in [true, false] {
        u.frame(vec![Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE }]);
    }
}

#[test]
fn command_selection_edits_mixed_publishing_flags_in_one_undo() {
    let mut u = editor();
    let before = u.app.session.doc().unwrap().clone();
    select_pair(&mut u);
    open_bulk(&mut u);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(!has_text(&shapes, "Code syntax"), "names, descriptions and code syntax stay individual");
    click(&mut u, text_center(&shapes, "Hide from publishing"), Modifiers::NONE, PointerButton::Primary);
    let vars = &u.app.session.doc().unwrap().variables.variables;
    assert!(vars[0].hidden_from_publishing && vars[1].hidden_from_publishing);
    assert!(!vars[2].hidden_from_publishing, "the unselected row is unchanged");
    let mut expected = before.clone();
    expected.variables.variables[0].hidden_from_publishing = true;
    assert_eq!(u.app.session.doc().unwrap(), &expected, "all unrelated metadata and mode values remain exact");
    click_label(&mut u, "Hide from publishing");
    assert!(u.app.session.doc().unwrap().variables.variables[..2].iter().all(|v| !v.hidden_from_publishing));
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap(), &expected, "one Undo restores both changed rows");
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap(), &before);
}

#[test]
fn shift_range_selection_marks_all_visible_rows_and_escape_clears_it() {
    let mut u = editor();
    let a = row(&mut u, "Alpha");
    click(&mut u, a, Modifiers::NONE, PointerButton::Primary);
    let c = row(&mut u, "Gamma");
    click(&mut u, c, Modifiers::SHIFT, PointerButton::Primary);
    let fill = crate::theme::Tokens::get(&u.ctx).selected_row;
    let out = u.frame_output(vec![]);
    let shapes = flat(&out.shapes);
    for name in ["Alpha", "Beta", "Gamma"] {
        let p = text_center(&shapes, name);
        assert!(shapes.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill==fill && r.rect.contains(p))), "range must select {name}");
    }
    key(&mut u, Key::Escape);
    assert!(u.app.ui.variables.as_ref().unwrap().selected.is_none());
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(!shapes.iter().any(|s| matches!(s, egui::Shape::Rect(r) if r.fill==fill && r.rect.contains(a))));
}

#[test]
fn a_single_variable_keeps_its_existing_details_and_edit_path() {
    let mut u = editor();
    let a = row(&mut u, "Alpha");
    let g = Geom::floating(u.ctx.content_rect(), None, true);
    click(&mut u, Pos2::new(g.panel.max.x - 20.0, a.y), Modifiers::NONE, PointerButton::Primary);
    for _ in 0..12 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&shapes, "Edit variable") && has_text(&shapes, "Code syntax"));
    click(&mut u, text_center(&shapes, "Hide from publishing"), Modifiers::NONE, PointerButton::Primary);
    assert!(u.app.session.doc().unwrap().variables.variables[0].hidden_from_publishing);
    assert!(!u.app.session.doc().unwrap().variables.variables[2].hidden_from_publishing);
}

fn click_label(u: &mut Ui, label: &str) {
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click(u, text_center(&shapes, label), Modifiers::NONE, PointerButton::Primary);
}

fn checked_state(u: &mut Ui, label: &str) -> Option<egui::accesskit::Toggled> {
    let out = u.frame_output(vec![]);
    let tree = out.platform_output.accesskit_update.expect("AccessKit update");
    tree.nodes
        .iter()
        .find(|(_, n)| n.role() == egui::accesskit::Role::CheckBox && n.label() == Some(label))
        .unwrap_or_else(|| panic!("missing checkbox {label}"))
        .1
        .toggled()
}

#[test]
fn shared_scopes_preserve_other_properties_and_each_click_is_one_undo() {
    let mut u = editor();
    let before = u.app.session.doc().unwrap().clone();
    select_pair(&mut u);
    open_bulk(&mut u);
    assert_eq!(checked_state(&mut u, "Hide from publishing"), Some(egui::accesskit::Toggled::Mixed));
    click_label(&mut u, "Scope");
    assert_eq!(checked_state(&mut u, "Frame fill"), Some(egui::accesskit::Toggled::Mixed));
    assert_eq!(checked_state(&mut u, "Stroke"), Some(egui::accesskit::Toggled::True));
    click_label(&mut u, "Frame fill");
    let vars = &u.app.session.doc().unwrap().variables.variables;
    for v in &vars[..2] {
        assert!(v.in_scope("FRAME_FILL") && v.in_scope("STROKE_COLOR"));
    }
    assert_eq!(vars[2], before.variables.variables[2]);
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap(), &before);
    click_label(&mut u, "All supported properties");
    assert!(u.app.session.doc().unwrap().variables.variables[..2].iter().all(|v| v.scopes.is_none()));
    click_label(&mut u, "All supported properties");
    assert!(u.app.session.doc().unwrap().variables.variables[..2].iter().all(|v| v.scopes.as_ref().is_some_and(Vec::is_empty)));
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    assert!(u.app.session.doc().unwrap().variables.variables[..2].iter().all(|v| v.scopes.is_none()));
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap(), &before);
    click_label(&mut u, "Stroke");
    let vars = &u.app.session.doc().unwrap().variables.variables;
    assert!(vars[0].in_scope("FRAME_FILL") && !vars[0].in_scope("STROKE_COLOR"));
    assert_eq!(vars[1].scopes, Some(vec![]));
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap(), &before);
}

fn mixed_types(second_type: &str, value: serde_json::Value) -> Ui {
    let mut u = editor();
    u.app.session.execute("variable.createCollection", &json!({"name":"Mixed"})).unwrap();
    for (name, kind, val) in [("Alpha", "number", json!(12)), ("Beta", second_type, value)] {
        u.app.session.execute("variable.create", &json!({"collection":"Mixed","name":name,"type":kind,"value":val})).unwrap();
    }
    u.app.ui.variables = Some(crate::variables_panel::Editor::on(Some("Mixed".into())));
    for _ in 0..15 {
        u.frame(vec![]);
    }
    u
}

#[test]
fn mixed_types_offer_only_common_scopes_and_boolean_has_none() {
    let mut u = mixed_types("string", json!("Hello"));
    let before = u.app.session.doc().unwrap().clone();
    select_pair(&mut u);
    open_bulk(&mut u);
    click_label(&mut u, "Scope");
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&shapes, "Text content"));
    assert!(!has_text(&shapes, "Font size") && !has_text(&shapes, "Font family"));
    click_label(&mut u, "Text content");
    let vars = &u.app.session.doc().unwrap().variables.variables;
    assert!(vars.iter().filter(|v| v.collection == vars[3].collection).all(|v| !v.in_scope("TEXT_CONTENT")));
    assert!(vars[3].in_scope("FONT_SIZE") && vars[4].in_scope("FONT_FAMILY"), "non-common scopes stay enabled");
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap(), &before);
    let mut b = mixed_types("boolean", json!(true));
    select_pair(&mut b);
    open_bulk(&mut b);
    click_label(&mut b, "Scope");
    let shapes = flat(&b.frame_output(vec![]).shapes);
    assert!(has_text(&shapes, "No shared scope properties"));
    assert!(!has_text(&shapes, "All supported properties"));
}

#[test]
fn shared_editor_retires_on_document_change_even_with_same_variable_ids() {
    let mut u = editor();
    select_pair(&mut u);
    open_bulk(&mut u);
    let p = text_center(&flat(&u.frame_output(vec![]).shapes), "Hide from publishing");
    let doc = u.app.session.doc().unwrap().clone();
    let old_owner = u.app.session.ds().unwrap().uid;
    u.app.session = Session::with_document(doc.clone());
    assert_ne!(old_owner, u.app.session.ds().unwrap().uid);
    click(&mut u, p, Modifiers::NONE, PointerButton::Primary);
    key(&mut u, Key::Space);
    assert_eq!(u.app.session.doc().unwrap(), &doc, "stale popup input must not target equal IDs in the replacement document");
    assert!(!u.app.session.ds().unwrap().is_dirty());
    assert!(u.app.session.ds().unwrap().history.undo.is_empty());
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(!has_text(&shapes, "Edit variables"));
}

#[test]
fn keyboard_tab_space_and_enter_can_operate_the_shared_checkbox() {
    let mut u = editor();
    select_pair(&mut u);
    open_bulk(&mut u);
    let mut found = false;
    let mut trace = Vec::new();
    for _ in 0..80 {
        key(&mut u, Key::Tab);
        let out = u.frame_output(vec![]);
        let tree = out.platform_output.accesskit_update.unwrap();
        trace.push(format!(
            "focus={:?} memory={:?} label={:?}",
            tree.focus,
            u.ctx.memory(|m| m.focused()),
            tree.nodes.iter().find(|(id, _)| *id == tree.focus).map(|(_, n)| (n.role(), n.label().map(str::to_owned)))
        ));
        if tree
            .nodes
            .iter()
            .any(|(id, n)| *id == tree.focus && n.role() == egui::accesskit::Role::CheckBox && n.label() == Some("Hide from publishing"))
        {
            found = true;
            break;
        }
    }
    assert!(found, "Tab must reach the shared checkbox: {trace:?}");
    key(&mut u, Key::Space);
    assert!(u.app.session.doc().unwrap().variables.variables[..2].iter().all(|v| v.hidden_from_publishing));
    key(&mut u, Key::Enter);
    assert!(u.app.session.doc().unwrap().variables.variables[..2].iter().all(|v| !v.hidden_from_publishing));
    key(&mut u, Key::Escape);
    assert!(!has_text(&flat(&u.frame_output(vec![]).shapes), "Edit variables"));
}

#[test]
fn a_deleted_target_closes_the_editor_without_partially_editing_the_survivor() {
    let mut u = editor();
    select_pair(&mut u);
    open_bulk(&mut u);
    let p = text_center(&flat(&u.frame_output(vec![]).shapes), "Hide from publishing");
    let id = u.app.session.doc().unwrap().variables.variables[1].id.clone();
    u.app.session.execute("variable.delete", &json!({"variable":id})).unwrap();
    let expected = u.app.session.doc().unwrap().clone();
    u.frame(vec![]);
    assert!(!has_text(&flat(&u.frame_output(vec![]).shapes), "Edit variables"));
    click(&mut u, p, Modifiers::NONE, PointerButton::Primary);
    assert_eq!(u.app.session.doc().unwrap(), &expected);
}

#[test]
fn inherited_selection_cannot_open_a_shared_editor_or_run_primary_only_actions() {
    let mut u = editor();
    let ext = u.app.session.execute("variable.extendCollection", &json!({"collection":"Tokens"})).unwrap();
    u.app.ui.variables = Some(crate::variables_panel::Editor::on(Some(ext["id"].as_str().unwrap().into())));
    for _ in 0..15 {
        u.frame(vec![]);
    }
    let expected = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.ds().unwrap().history.undo.len();
    select_pair(&mut u);
    let p = row(&mut u, "Beta");
    click(&mut u, p, Modifiers::NONE, PointerButton::Secondary);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&shapes, "Edit variables"));
    for label in ["Delete variable", "Duplicate", "Rename", "Copy"] {
        assert!(!has_text(&shapes, label));
    }
    click(&mut u, text_center(&shapes, "Edit variables"), Modifiers::NONE, PointerButton::Primary);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(!has_text(&shapes, "Hide from publishing"), "inherited targets cannot enter shared editing");
    assert_eq!(u.app.session.doc().unwrap(), &expected);
    assert_eq!(u.app.session.ds().unwrap().history.undo.len(), undo);
}

#[test]
fn a_published_library_selection_remains_read_only() {
    use figcraft_doc::LibraryLink;
    use figcraft_engine::library;
    use std::sync::Arc;
    let mut u = editor();
    let mut published = u.app.session.doc().unwrap().clone();
    for v in &mut published.variables.variables {
        v.hidden_from_publishing = false;
    }
    let offered = library::default_selection(&published, None);
    let lib = library::publish(&published, None, &offered, None, 1).unwrap();
    // An unsaved document has no relative-library origin; cached references need explicit local authority.
    let fixture_path = std::env::temp_dir().join("synthetic-bulk-library.figcraft");
    let path = fixture_path.to_str().unwrap();
    u.app.session.doc_mut().unwrap().libraries_mut().enabled.push(LibraryLink {
        id: lib.manifest.id.clone(),
        name: lib.manifest.name.clone(),
        path: path.into(),
        version: lib.manifest.version,
    });
    u.app.session.libraries.insert(path, Ok(Arc::new(lib.clone())));
    u.app.ui.variables = Some(crate::variables_panel::Editor::in_library(lib.manifest.id.clone(), Some("Tokens".into())));
    for _ in 0..15 {
        u.frame(vec![]);
    }
    let expected = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.ds().unwrap().history.undo.len();
    select_pair(&mut u);
    let p = row(&mut u, "Beta");
    click(&mut u, p, Modifiers::NONE, PointerButton::Secondary);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(!has_text(&shapes, "Edit variables") && !has_text(&shapes, "Edit variable"));
    assert_eq!(u.app.session.doc().unwrap(), &expected);
    assert_eq!(u.app.session.ds().unwrap().history.undo.len(), undo);
}
