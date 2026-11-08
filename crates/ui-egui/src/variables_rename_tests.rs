//! Renames in the variables window, drawn headless with the screen reader tree on: creating a
//! collection opens its name for editing, and so does a double click on its row. On every frame
//! the focused node must be in the AccessKit tree; the platform adapter (UI Automation on Windows)
//! aborts the app when it isn't (#224). A collection's context menu extends it.

use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use figcraft_engine::Session;

use crate::layers_drag_tests::Ui;
use crate::live_tests::{flat, has_text, text_center};

/// The variables window open on a document without collections, with AccessKit on.
fn variables_window() -> Ui {
    let mut u = Ui::new(Session::new());
    u.app.ui.variables = Some(Default::default());
    u.ctx.enable_accesskit();
    for _ in 0..10 {
        u.frame(vec![]);
    }
    u
}

/// One frame with `events`, checking the focused node is in the screen reader tree.
fn checked_frame(u: &mut Ui, events: Vec<Event>, k: usize) {
    let out = u.frame_output(events);
    let tree = out.platform_output.accesskit_update.expect("an AccessKit update");
    assert!(tree.nodes.iter().any(|(i, _)| *i == tree.focus), "frame {k}: the focused node {:?} isn't in the tree", tree.focus);
}

/// `clicks` clicks at `p` (two make a double click) and the frames that follow, each checked.
fn click_checking_focus(u: &mut Ui, p: Pos2, clicks: usize) {
    for _ in 0..10 {
        u.frame(vec![Event::PointerMoved(p)]);
    }
    let presses = [true, false].into_iter().cycle().take(clicks * 2).map(Some);
    for (k, pressed) in presses.chain(std::iter::repeat_n(None, 20)).enumerate() {
        let events = pressed.map(|pressed| Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        checked_frame(u, events.into_iter().collect(), k);
    }
}

/// Type `name` and press Enter (each frame checked).
fn type_name(u: &mut Ui, name: &str) {
    checked_frame(u, vec![Event::Text(name.into())], 0);
    for pressed in [true, false] {
        checked_frame(u, vec![Event::Key { key: Key::Enter, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE }], 1);
    }
    for k in 0..5 {
        checked_frame(u, vec![], k + 2);
    }
}

fn collection_names(u: &Ui) -> Vec<String> {
    u.app.session.doc().unwrap().variables.collections.iter().map(|c| c.name.clone()).collect()
}

#[test]
fn creating_a_collection_puts_its_name_in_edit_with_the_focus_in_the_screen_reader_tree() {
    let mut u = variables_window();
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_checking_focus(&mut u, text_center(&shapes, "Create collection"), 1);
    assert_eq!(collection_names(&u), ["Collection"], "a click creates one collection");
    // The keyboard is in its name: typing renames it.
    type_name(&mut u, "Brand");
    assert_eq!(collection_names(&u), ["Brand"]);
    assert!(u.app.ui.last_error.is_none(), "{:?}", u.app.ui.last_error);
}

#[test]
fn double_clicking_a_variable_renames_it_with_the_focus_in_the_screen_reader_tree() {
    let mut u = variables_window();
    let coll = u.app.session.execute("variable.createCollection", &serde_json::json!({"name": "C"})).unwrap();
    let cid = coll["id"].as_str().unwrap().to_string();
    u.app.session.execute("variable.create", &serde_json::json!({"collection": cid, "name": "Ren", "type": "Color"})).unwrap();
    for _ in 0..10 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_checking_focus(&mut u, text_center(&shapes, "Ren"), 2);
    // The row's text was replaced by the field, whose text is selected: typing renames it.
    type_name(&mut u, "Renamed");
    let names: Vec<String> = u.app.session.doc().unwrap().variables.variables.iter().map(|v| v.name.clone()).collect();
    assert!(names.contains(&"Renamed".to_string()), "names = {names:?}");
    assert!(u.app.ui.last_error.is_none(), "{:?}", u.app.ui.last_error);
}

#[test]
fn dragging_the_floating_panel_edge_resizes_it() {
    let mut u = variables_window();
    let before = u.app.ui.variables.as_ref().unwrap().size;
    // The panel is centred: 880x480 on a 1440x900 screen → min (280,210), max (1160,690).
    let right_edge = Pos2::new(1160.0, 500.0);
    let to = Pos2::new(1260.0, 500.0);
    u.button(right_edge, true);
    for k in 1..=10 {
        u.frame(vec![Event::PointerMoved(right_edge + (to - right_edge) * (k as f32 / 10.0))]);
    }
    u.button(to, false);
    let after = u.app.ui.variables.as_ref().unwrap().size;
    assert_ne!(before, after, "size should change when the edge is dragged");
    let after = after.expect("a size after dragging");
    assert!(after.x > 880.0 - 1.0, "width grew: {after:?}");
    assert!(u.app.ui.last_error.is_none(), "{:?}", u.app.ui.last_error);
}

/// Press on `from`, drag to `to` in steps, and release — a row drag, not just a press.
fn drag_release(u: &mut Ui, from: Pos2, to: Pos2) {
    u.button(from, true);
    for k in 1..=10 {
        u.frame(vec![Event::PointerMoved(from + (to - from) * (k as f32 / 10.0))]);
    }
    u.button(to, false);
}

#[test]
fn dragging_a_variable_to_the_empty_space_below_reorders_it_to_the_end() {
    let mut u = variables_window();
    let coll = u.app.session.execute("variable.createCollection", &serde_json::json!({"name": "C"})).unwrap();
    let cid = coll["id"].as_str().unwrap().to_string();
    u.app.session.execute("variable.create", &serde_json::json!({"collection": cid, "name": "A", "type": "Color"})).unwrap();
    u.app.session.execute("variable.create", &serde_json::json!({"collection": cid, "name": "B", "type": "Color"})).unwrap();
    for _ in 0..10 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let a = text_center(&shapes, "A");
    let b = text_center(&shapes, "B");
    // Drag A's row down into the empty space below the list's last row.
    drag_release(&mut u, a, Pos2::new(a.x, b.y + 300.0));
    let order: Vec<String> = u.app.session.doc().unwrap().variables.variables.iter().map(|v| v.name.clone()).collect();
    assert_eq!(order, ["B", "A"], "A moved to the end: {order:?}");
    assert!(u.app.ui.last_error.is_none(), "{:?}", u.app.ui.last_error);
}

#[test]
fn double_clicking_a_collection_renames_it_with_the_focus_in_the_screen_reader_tree() {
    let mut u = variables_window();
    u.app.session.execute("variable.createCollection", &serde_json::json!({"name": "Spacing"})).unwrap();
    for _ in 0..10 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_checking_focus(&mut u, text_center(&shapes, "Spacing"), 2);
    // The row's text was replaced by the field, whose text is selected: typing replaces it.
    type_name(&mut u, "Space");
    assert_eq!(collection_names(&u), ["Space"]);
    assert!(u.app.ui.last_error.is_none(), "{:?}", u.app.ui.last_error);
}

/// One click of `button` at `p` and the frames that follow.
fn click_with(u: &mut Ui, p: Pos2, button: PointerButton) {
    for pressed in [true, false] {
        u.frame(vec![Event::PointerMoved(p), Event::PointerButton { pos: p, button, pressed, modifiers: Modifiers::NONE }]);
    }
    for _ in 0..5 {
        u.frame(vec![]);
    }
}

#[test]
fn a_collection_of_this_file_extends_from_its_context_menu() {
    let mut u = variables_window();
    u.app.session.execute("variable.createCollection", &serde_json::json!({"name": "Theme", "modes": ["Light", "Dark"]})).unwrap();
    u.app.session.execute("variable.create", &serde_json::json!({"collection": "Theme", "name": "radius", "type": "number", "value": 4})).unwrap();
    for _ in 0..10 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_with(&mut u, text_center(&shapes, "Theme"), PointerButton::Secondary);
    let menu = flat(&u.frame_output(vec![]).shapes);
    for item in ["Rename", "Extend collection", "Delete collection"] {
        assert!(has_text(&menu, item), "{item} in the menu");
    }
    click_with(&mut u, text_center(&menu, "Extend collection"), PointerButton::Primary);
    assert!(u.app.ui.last_error.is_none(), "{:?}", u.app.ui.last_error);
    assert_eq!(collection_names(&u), ["Theme", "Theme (extended)"]);
    let doc = u.app.session.doc().unwrap();
    let ext = doc.variables.collection("Theme (extended)").unwrap();
    assert_eq!(ext.extends.as_ref(), Some(&doc.variables.collection("Theme").unwrap().id));
    // The new collection is the one shown: its inherited variable and a note on what it extends.
    let shown = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&shown, "radius"));
    assert!(has_text(&shown, "Extends \"Theme\": overridden values are marked"));
}

// Append this review-owned fixture to variables_rename_tests.rs only for the parent-owned
// serial red/green run. It reuses actual shared egui frame/AccessKit helpers in that module.
// Source was independently read; reviewer did not compile or mutate product files.

fn variables_owner_fixture_with_native_clone() -> (Ui, String, usize, usize) {
    let mut u = variables_window();
    let coll = u.app.session.execute("variable.createCollection", &serde_json::json!({"name":"Owner collection"})).unwrap();
    let cid = coll["id"].as_str().unwrap().to_string();
    let v = u
        .app
        .session
        .execute("variable.create", &serde_json::json!({"collection":cid,"name":"Owner variable","type":"String","value":"Original"}))
        .unwrap();
    let vid = v["id"].as_str().unwrap().to_string();
    let a = u.app.session.active;
    let bytes = figcraft_format::save_bytes(u.app.session.doc().unwrap(), None).unwrap();
    let (cloned, issues) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty());
    u.app.session.execute("file.new", &serde_json::json!({"name":"Other owner"})).unwrap();
    let b = u.app.session.active;
    u.app.session.ds_mut().unwrap().doc = std::sync::Arc::new(cloned);
    assert_ne!(u.app.session.docs[a].uid, u.app.session.docs[b].uid);
    assert_eq!(
        u.app.session.docs[a].doc.variables, u.app.session.docs[b].doc.variables,
        "actual native format preserves the variable/collection UUIDs"
    );
    u.app.session.active = a;
    for _ in 0..10 {
        u.frame(vec![]);
    }
    (u, vid, a, b)
}

#[test]
fn variables_pending_rename_cannot_commit_into_a_native_copy_with_the_same_uuid() {
    let (mut u, vid, a, b) = variables_owner_fixture_with_native_clone();
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_checking_focus(&mut u, text_center(&shapes, "Owner variable"), 2);
    assert!(matches!(&u.app.ui.variables.as_ref().unwrap().rename,Some(crate::variables_panel::Rename::Variable(id)) if *id==vid));
    checked_frame(&mut u, vec![Event::Text("Must not reach another document".into())], 0);
    assert_eq!(u.app.session.docs[a].doc.variables.get_by_id(&vid).unwrap().name, "Owner variable", "draft is still uncommitted in A");
    let a_before = u.app.session.docs[a].doc.clone();
    let b_before = u.app.session.docs[b].doc.clone();
    u.app.session.active = b;
    let b_undo = u.app.session.undo_depth();
    for pressed in [true, false] {
        checked_frame(&mut u, vec![Event::Key { key: Key::Enter, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE }], 1);
    }
    for _ in 0..3 {
        u.frame(vec![]);
    }
    assert_eq!(u.app.session.docs[b].doc, b_before, "pending A rename must never rename B's equal UUID");
    assert_eq!(u.app.session.undo_depth(), b_undo, "B gets no rename undo step");
    assert_eq!(u.app.session.docs[a].doc, a_before, "ownership change cancels the uncommitted name without changing A");
    assert!(u.app.ui.variables.as_ref().is_none_or(|st| st.rename.is_none()), "stale rename target cleared");
}

#[test]
fn variables_pending_collection_rename_cannot_commit_into_a_native_copy() {
    let (mut u, _vid, a, b) = variables_owner_fixture_with_native_clone();
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_checking_focus(&mut u, text_center(&shapes, "Owner collection"), 2);
    checked_frame(&mut u, vec![Event::Text("Foreign collection name".into())], 0);
    let a_before = u.app.session.docs[a].doc.clone();
    let b_before = u.app.session.docs[b].doc.clone();
    u.app.session.active = b;
    let undo = u.app.session.undo_depth();
    for pressed in [true, false] {
        checked_frame(&mut u, vec![Event::Key { key: Key::Enter, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE }], 1);
    }
    assert_eq!(u.app.session.docs[b].doc, b_before, "collection rename draft cannot follow the UUID into B");
    assert_eq!(u.app.session.docs[a].doc, a_before);
    assert_eq!(u.app.session.undo_depth(), undo);
}

// Append beside variables_owner_fixture_with_native_clone in variables_rename_tests.rs.
// Actual egui pointer/keyboard drafts across A -> B -> A; no direct temp-buffer injection.
fn owner_enter(u: &mut Ui) {
    for pressed in [true, false] {
        checked_frame(u, vec![Event::Key { key: Key::Enter, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE }], 0);
    }
    for _ in 0..3 {
        u.frame(vec![]);
    }
}
fn owner_replace_draft(u: &mut Ui, text: &str) {
    for pressed in [true, false] {
        checked_frame(u, vec![Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::COMMAND }], 0);
    }
    checked_frame(u, vec![Event::Text(text.into())], 0);
}
fn owner_open_details(u: &mut Ui) {
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let row = text_center(&shapes, "Owner variable");
    let g = crate::variables_panel::Geom::floating(u.ctx.content_rect(), None, true);
    click_checking_focus(u, Pos2::new(g.panel.max.x - 20.0, row.y), 1);
    for _ in 0..3 {
        u.frame(vec![]);
    }
}
#[test]
fn variables_value_draft_does_not_resurrect_after_a_to_b_to_a() {
    let (mut u, vid, a, b) = variables_owner_fixture_with_native_clone();
    let a_before = u.app.session.docs[a].doc.clone();
    let b_before = u.app.session.docs[b].doc.clone();
    let a_undo = u.app.session.undo_depth();
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_checking_focus(&mut u, text_center(&shapes, "Original"), 1);
    owner_replace_draft(&mut u, "Stale value draft");
    assert_eq!(u.app.session.docs[a].doc, a_before, "typed value is an uncommitted draft");
    u.app.session.active = b;
    u.frame(vec![]);
    owner_enter(&mut u);
    assert_eq!(u.app.session.docs[b].doc, b_before, "B never receives A's value draft");
    u.app.session.active = a;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(!has_text(&shapes, "Stale value draft"), "returning to A cannot revive old field buffer");
    assert_eq!(u.app.session.docs[a].doc, a_before, "returning to the same UUID/value cell cannot commit old draft");
    assert_eq!(u.app.session.undo_depth(), a_undo);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_checking_focus(&mut u, text_center(&shapes, "Original"), 1);
    owner_replace_draft(&mut u, "Fresh value");
    owner_enter(&mut u);
    let values = &u.app.session.doc().unwrap().variables.get_by_id(&vid).unwrap().values;
    assert!(values.values().any(|v| *v == figcraft_doc::VariableValue::String("Fresh value".into())), "fresh same-document cell edits still work");
    assert_eq!(u.app.session.docs[b].doc, b_before);
    assert_eq!(u.app.session.undo_depth().0, a_undo.0 + 1, "fresh value edit is one undo step");
}
#[test]
fn variables_details_draft_does_not_resurrect_after_a_to_b_to_a() {
    let (mut u, vid, a, b) = variables_owner_fixture_with_native_clone();
    for index in [a, b] {
        u.app.session.active = index;
        u.app.session.execute("variable.update", &serde_json::json!({"variable":vid,"description":"Original description"})).unwrap();
    }
    u.app.session.active = a;
    for _ in 0..5 {
        u.frame(vec![]);
    }
    let a_before = u.app.session.docs[a].doc.clone();
    let b_before = u.app.session.docs[b].doc.clone();
    let a_undo = u.app.session.undo_depth();
    owner_open_details(&mut u);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_checking_focus(&mut u, text_center(&shapes, "Original description"), 1);
    owner_replace_draft(&mut u, "Stale details draft");
    assert_eq!(u.app.session.docs[a].doc, a_before, "description remains a draft");
    u.app.session.active = b;
    u.frame(vec![]);
    owner_enter(&mut u);
    assert_eq!(u.app.session.docs[b].doc, b_before, "details draft never crosses into B");
    u.app.session.active = a;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    owner_open_details(&mut u);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(!has_text(&shapes, "Stale details draft"), "same UUID details field must start from authored data after return");
    assert_eq!(u.app.session.docs[a].doc, a_before);
    assert_eq!(u.app.session.undo_depth(), a_undo);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_checking_focus(&mut u, text_center(&shapes, "Original description"), 1);
    owner_replace_draft(&mut u, "Fresh description");
    owner_enter(&mut u);
    assert_eq!(u.app.session.doc().unwrap().variables.get_by_id(&vid).unwrap().description, "Fresh description");
    assert_eq!(u.app.session.docs[b].doc, b_before);
    assert_eq!(u.app.session.undo_depth().0, a_undo.0 + 1);
}

// Append to variables_rename_tests.rs for the root-owned serial red/green run.
// Reuses the actual frame, pointer, shape and native-copy helpers in that module.
// This reviewer did not run Cargo or modify product source.

fn owner_reclone_b(u: &mut Ui, a: usize, b: usize) {
    let bytes = figcraft_format::save_bytes(&u.app.session.docs[a].doc, None).unwrap();
    let (copy, issues) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty());
    u.app.session.docs[b].doc = std::sync::Arc::new(copy);
    assert_ne!(u.app.session.docs[a].uid, u.app.session.docs[b].uid);
    assert_eq!(u.app.session.docs[a].doc.variables, u.app.session.docs[b].doc.variables);
    u.app.session.active = a;
    for _ in 0..8 {
        u.frame(vec![]);
    }
}

fn variables_context_menu_owner_case(kind: &str) {
    let (mut u, _vid, a, b) = variables_owner_fixture_with_native_clone();
    if kind == "mode" {
        let collection = u.app.session.doc().unwrap().variables.collections[0].id.clone();
        u.app.session.execute("variable.addMode", &serde_json::json!({"collection":collection,"name":"Owner extra mode"})).unwrap();
        owner_reclone_b(&mut u, a, b);
    }
    let (row, action) = match kind {
        "collection" => ("Owner collection", "Delete collection"),
        "mode" => ("Owner extra mode", "Delete mode"),
        _ => ("Owner variable", "Delete variable"),
    };
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_with(&mut u, text_center(&shapes, row), PointerButton::Secondary);
    let menu = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&menu, action), "the actual A {kind} menu opened");
    let before_a = u.app.session.docs[a].doc.clone();
    let before_b = u.app.session.docs[b].doc.clone();
    let undo_a = u.app.session.docs[a].history.undo.len();
    let undo_b = u.app.session.docs[b].history.undo.len();
    // No outside pointer press: this is the real command-driven file switch path.
    u.app.run("doc.switch", serde_json::json!({"index":b})).unwrap();
    let after_switch = flat(&u.frame_output(vec![]).shapes);
    let stale_menu = has_text(&after_switch, action);
    if stale_menu {
        // Exercise the destructive old menu if it survived, rather than just inspecting IDs.
        click_with(&mut u, text_center(&after_switch, action), PointerButton::Primary);
    }
    assert_eq!(u.app.session.docs[b].doc, before_b, "A's {kind} menu cannot delete matching data in B");
    assert_eq!(u.app.session.docs[a].doc, before_a);
    assert_eq!(u.app.session.docs[a].history.undo.len(), undo_a);
    assert_eq!(u.app.session.docs[b].history.undo.len(), undo_b, "B has no foreign menu history");
    assert!(!stale_menu, "the old {kind} menu must retire on owner change");
    // A fresh same-document menu still performs the requested action once.
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_with(&mut u, text_center(&shapes, row), PointerButton::Secondary);
    let fresh = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&fresh, action));
    click_with(&mut u, text_center(&fresh, action), PointerButton::Primary);
    assert_ne!(u.app.session.docs[b].doc, before_b, "a fresh B {kind} menu remains usable");
    assert_eq!(u.app.session.docs[b].history.undo.len(), undo_b + 1);
    assert_eq!(u.app.session.docs[a].doc, before_a);
}

#[test]
fn variables_context_variable_delete_cannot_retarget_a_native_copy() {
    variables_context_menu_owner_case("variable");
}

#[test]
fn variables_context_collection_delete_cannot_retarget_a_native_copy() {
    variables_context_menu_owner_case("collection");
}

#[test]
fn variables_context_mode_delete_cannot_retarget_a_native_copy() {
    variables_context_menu_owner_case("mode");
}

fn owner_hold_drag(u: &mut Ui, from: Pos2) {
    u.frame(vec![Event::PointerMoved(from)]);
    u.frame(vec![Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
    for dy in [4.0, 10.0, 16.0] {
        u.frame(vec![Event::PointerMoved(from + egui::vec2(0.0, dy))]);
    }
}

fn owner_release_drag(u: &mut Ui, at: Pos2) {
    u.frame(vec![Event::PointerMoved(at)]);
    u.frame(vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    for _ in 0..3 {
        u.frame(vec![]);
    }
}

#[test]
fn variables_held_collection_drag_cannot_reorder_a_native_copy() {
    let (mut u, _vid, a, b) = variables_owner_fixture_with_native_clone();
    for name in ["Second owner collection", "Third owner collection"] {
        u.app.session.execute("variable.createCollection", &serde_json::json!({"name":name})).unwrap();
    }
    owner_reclone_b(&mut u, a, b);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let from = text_center(&shapes, "Owner collection");
    let to = text_center(&shapes, "Third owner collection") + egui::vec2(0.0, 14.0);
    let before_a = u.app.session.docs[a].doc.clone();
    let before_b = u.app.session.docs[b].doc.clone();
    let undo_a = u.app.session.docs[a].history.undo.len();
    let undo_b = u.app.session.docs[b].history.undo.len();
    owner_hold_drag(&mut u, from);
    u.app.run("doc.switch", serde_json::json!({"index":b})).unwrap();
    u.frame(vec![]); // the pointer remains held while B draws its same collection IDs
    owner_release_drag(&mut u, to);
    assert_eq!(u.app.session.docs[b].doc, before_b, "A's held collection cannot reorder B");
    assert_eq!(u.app.session.docs[a].doc, before_a);
    assert_eq!(u.app.session.docs[a].history.undo.len(), undo_a);
    assert_eq!(u.app.session.docs[b].history.undo.len(), undo_b);
    // Demonstrate that these real coordinates do cause a reorder when owned by B.
    owner_hold_drag(&mut u, from);
    owner_release_drag(&mut u, to);
    assert_ne!(u.app.session.docs[b].doc, before_b, "a fresh owned collection drag still reorders");
    assert_eq!(u.app.session.docs[b].history.undo.len(), undo_b + 1);
    assert_eq!(u.app.session.docs[a].doc, before_a);
}

#[test]
fn variables_held_group_drag_cannot_reorder_the_native_copys_view() {
    let (mut u, vid, a, b) = variables_owner_fixture_with_native_clone();
    let collection = u.app.session.doc().unwrap().variables.collections[0].id.clone();
    u.app.session.execute("variable.update", &serde_json::json!({"variable":vid,"name":"Alpha owner group/One"})).unwrap();
    for name in ["Beta owner group/Two", "Gamma owner group/Three"] {
        u.app.session.execute("variable.create", &serde_json::json!({"collection":collection,"name":name,"type":"String","value":"value"})).unwrap();
    }
    owner_reclone_b(&mut u, a, b);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let from = text_center(&shapes, "Alpha owner group");
    let to = text_center(&shapes, "Gamma owner group") + egui::vec2(0.0, 12.0);
    let panel = crate::variables_panel::Geom::floating(u.ctx.content_rect(), None, true).panel;
    assert!(from.x < panel.min.x + u.app.ui.variables.as_ref().unwrap().sidebar_w, "real group sidebar row");
    let before_a = u.app.session.docs[a].doc.clone();
    let before_b = u.app.session.docs[b].doc.clone();
    let undo_a = u.app.session.docs[a].history.undo.len();
    let undo_b = u.app.session.docs[b].history.undo.len();
    assert!(u.app.ui.variables.as_ref().unwrap().group_order.is_empty());
    owner_hold_drag(&mut u, from);
    u.app.run("doc.switch", serde_json::json!({"index":b})).unwrap();
    u.frame(vec![]);
    owner_release_drag(&mut u, to);
    assert!(u.app.ui.variables.as_ref().unwrap().group_order.is_empty(), "A's held group must not write B's group ordering");
    assert_eq!(u.app.session.docs[b].doc, before_b);
    assert_eq!(u.app.session.docs[a].doc, before_a);
    assert_eq!(u.app.session.docs[a].history.undo.len(), undo_a);
    assert_eq!(u.app.session.docs[b].history.undo.len(), undo_b);
    owner_hold_drag(&mut u, from);
    owner_release_drag(&mut u, to);
    assert!(!u.app.ui.variables.as_ref().unwrap().group_order.is_empty(), "fresh same-owner group drag still reorders the view");
    assert_eq!(u.app.session.docs[b].doc, before_b, "group ordering remains a view preference");
    assert_eq!(u.app.session.docs[b].history.undo.len(), undo_b);
}

/// Exercise the actual group context menu, retaining every stored field in both modes.
fn pr2087_grouped_window(group: &str) -> Ui {
    let mut u = variables_window();
    u.app.session.execute("variable.createCollection", &serde_json::json!({"name": "Theme", "modes": ["Light", "Dark"]})).unwrap();
    for (leaf, light, dark) in [("sm", 4, 8), ("md", 16, 32)] {
        let name = format!("{group}/{leaf}");
        u.app
            .session
            .execute("variable.create", &serde_json::json!({"collection": "Theme", "name": name, "type": "number", "value": light}))
            .unwrap();
        u.app.session.execute("variable.set", &serde_json::json!({"variable": name, "mode": "Dark", "value": dark})).unwrap();
        u.app
            .session
            .execute(
                "variable.update",
                &serde_json::json!({
                    "variable": name, "scopes": ["GAP"], "description": name, "hiddenFromPublishing": true,
                    "codeSyntax": {"WEB": format!("var(--{leaf})"), "iOS": leaf, "ANDROID": format!("R.dimen.{leaf}")}
                }),
            )
            .unwrap();
    }
    for _ in 0..10 {
        u.frame(vec![]);
    }
    u
}

fn pr2087_duplicate_group(u: &mut Ui, leaf: &str) {
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click_with(u, text_center(&shapes, leaf), PointerButton::Secondary);
    let menu = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&menu, "Duplicate group"), "the group context menu offers duplication");
    click_with(u, text_center(&menu, "Duplicate group"), PointerButton::Primary);
    assert!(u.app.ui.last_error.is_none(), "{:?}", u.app.ui.last_error);
}

#[test]
fn duplicate_group_copies_every_field_and_both_modes_in_one_undo_step() {
    let mut u = pr2087_grouped_window("space");
    let before = u.app.session.undo_depth();
    let originals = u.app.session.doc().unwrap().variables.variables.clone();
    pr2087_duplicate_group(&mut u, "space");
    let doc = u.app.session.doc().unwrap();
    assert_eq!(doc.variables.variables.len(), 4);
    for old in &originals {
        let name = old.name.replacen("space/", "space 2/", 1);
        let new = doc.variables.variables.iter().find(|v| v.name == name).expect("copied variable");
        assert_ne!(new.id, old.id);
        let mut expected = old.clone();
        expected.id = new.id.clone();
        expected.name = name;
        assert_eq!(new, &expected, "every field, including all mode values and code syntax");
    }
    assert_eq!(u.app.session.undo_depth(), (before.0 + 1, 0));
    assert_eq!(u.app.session.undo_labels(1), ["Duplicate group"]);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(text_center(&shapes, "space 2").y > text_center(&shapes, "space").y);
    u.app.session.undo().unwrap();
    assert_eq!(u.app.session.doc().unwrap().variables.variables, originals, "one undo removes the complete copy");
    u.app.session.redo().unwrap();
    assert_eq!(u.app.session.doc().unwrap().variables.variables.len(), 4);
}

#[test]
fn duplicate_nested_group_uses_next_free_sibling_and_keeps_alias_targets() {
    let mut u = pr2087_grouped_window("theme/space");
    u.app
        .session
        .execute("variable.create", &serde_json::json!({"collection": "Theme", "name": "theme/space 2/taken", "type": "number", "value": 99}))
        .unwrap();
    u.app
        .session
        .execute(
            "variable.create",
            &serde_json::json!({"collection": "Theme", "name": "theme/space/child/alias", "type": "number", "value": {"alias": "theme/space/sm"}}),
        )
        .unwrap();
    for _ in 0..10 {
        u.frame(vec![]);
    }
    let originals = u.app.session.doc().unwrap().variables.variables.clone();
    pr2087_duplicate_group(&mut u, "space");
    let vars = &u.app.session.doc().unwrap().variables.variables;
    for old in originals.iter().filter(|v| v.name.starts_with("theme/space/")) {
        let name = old.name.replacen("theme/space/", "theme/space 3/", 1);
        let new = vars.iter().find(|v| v.name == name).expect("nested copy");
        assert_eq!(new.values, old.values, "aliases keep their original target IDs");
    }
    assert!(vars.iter().all(|v| !v.name.starts_with("space 3/")), "parent path retained");
    pr2087_duplicate_group(&mut u, "space");
    assert!(u.app.session.doc().unwrap().variables.variables.iter().any(|v| v.name == "theme/space 4/md"));
}

#[test]
fn duplicate_group_preserves_custom_order_and_places_copy_after_source() {
    let mut u = pr2087_grouped_window("space");
    for name in ["before/x", "after/x"] {
        u.app.session.execute("variable.create", &serde_json::json!({"collection": "Theme", "name": name, "type": "number", "value": 1})).unwrap();
    }
    let collection = u.app.session.doc().unwrap().variables.collections[0].id.clone();
    u.app.ui.variables.as_mut().unwrap().group_order.insert(collection, vec!["before".into(), "space".into(), "after".into()]);
    for _ in 0..10 {
        u.frame(vec![]);
    }
    pr2087_duplicate_group(&mut u, "space");
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let ys: Vec<f32> = ["before", "space", "space 2", "after"].iter().map(|name| text_center(&shapes, name).y).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "existing custom order plus adjacent copy: {ys:?}");
}
