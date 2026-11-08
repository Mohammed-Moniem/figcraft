//! #1840: actual raw Annotation-tool input and existing workflow controls.
use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Pos2, pos2};
use figcraft_doc::{Document, NodeId};
use figcraft_engine::{EngineError, Session};
use serde_json::json;

use crate::layers_drag_tests::Ui;

fn setup() -> (Ui, NodeId) {
    let mut s = Session::new();
    let id = NodeId(
        s.execute("shape.rectangle", &json!({"x":0,"y":0,"width":160,"height":100,"name":"Original annotated layer","props":{"fill":"#42A080"}}))
            .unwrap()["id"]
            .as_u64()
            .unwrap(),
    );
    s.execute("select.none", &json!({})).unwrap();
    let mut u = Ui::new(s);
    u.ctx.set_os(egui::os::OperatingSystem::Mac);
    u.ctx.enable_accesskit();
    u.frame(vec![]);
    (u, id)
}

fn key(u: &mut Ui, key: Key, mods: Modifiers) {
    u.frame_with(
        vec![
            Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: mods },
            Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: mods },
        ],
        mods,
    );
    u.frame(vec![]);
}

fn canvas_key(u: &mut Ui, key_name: Key, mods: Modifiers) {
    let at = u.app.to_screen(40.0, 30.0);
    u.frame(vec![Event::PointerMoved(at)]);
    key(u, key_name, mods);
}

fn click(u: &mut Ui, at: Pos2) {
    u.button(at, true);
    u.button(at, false);
    u.frame(vec![]);
}

#[test]
fn annotation_tool_raw_shift_t_activates_without_a_document_edit_in_both_modes() {
    for tab in [crate::RightTab::Design, crate::RightTab::Inspect] {
        let (mut u, _) = setup();
        u.app.ui.right_tab = tab;
        u.frame(vec![]);
        let before = u.app.session.doc().unwrap().clone();
        let undo = u.app.session.undo_depth();
        canvas_key(&mut u, Key::T, Modifiers::SHIFT);
        assert_eq!(u.app.session.tool.tool.id(), "annotation", "raw Shift+T must activate the real tool");
        assert_eq!(u.app.session.doc().unwrap(), &before);
        assert_eq!(u.app.session.undo_depth(), undo);
    }
}

#[test]
fn annotation_tool_raw_canvas_workflow_composes_commits_and_undoes_one_note() {
    let (mut u, id) = setup();
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth().0;
    canvas_key(&mut u, Key::T, Modifiers::SHIFT);
    let at = u.app.to_screen(40.0, 30.0);
    click(&mut u, at);
    let out = u.frame_output(vec![]);
    let nodes = out.platform_output.accesskit_update.expect("actual AccessKit tree").nodes;
    let note =
        nodes.iter().map(|(_, n)| n).find(|n| matches!(n.role(), Role::TextInput | Role::MultilineTextInput) && n.label() == Some("Annotation note"));
    let shown: Vec<_> = nodes
        .iter()
        .map(|(_, n)| n)
        .filter(|n| matches!(n.role(), Role::TextInput | Role::MultilineTextInput))
        .filter_map(|n| n.label())
        .take(20)
        .collect();
    let field = note.unwrap_or_else(|| panic!("no canvas Annotation note editor after raw Shift+T and layer click; inputs={shown:?}"));
    let r = field.bounds().expect("note has actual input bounds");
    click(&mut u, pos2(((r.x0 + r.x1) / 2.0) as f32, ((r.y0 + r.y1) / 2.0) as f32));
    u.frame(vec![Event::Text("Original canvas annotation".into())]);
    key(&mut u, Key::Enter, Modifiers::NONE);
    let annotations = &u.app.session.doc().unwrap().node(id).unwrap().annotations;
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].label, "Original canvas annotation");
    assert_eq!(u.app.session.undo_depth().0, undo + 1);
    u.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap(), &before, "one Undo restores the untouched document");
    u.app.run("edit.redo", json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap().node(id).unwrap().annotations[0].label, "Original canvas annotation");
    let saved = figcraft_format::save_bytes(u.app.session.doc().unwrap(), None).unwrap();
    let (reopened, issues) = figcraft_format::load_bytes(&saved).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(reopened.node(id).unwrap().annotations, u.app.session.doc().unwrap().node(id).unwrap().annotations);
}

#[test]
fn annotation_tool_existing_data_api_preserves_undo_properties_categories_and_serialization() {
    let (mut u, id) = setup();
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth().0;
    u.app.session.execute("annotation.add", &json!({"id":id,"label":"Existing API note","properties":["width"],"category":"Accessibility"})).unwrap();
    let saved = u.app.session.doc().unwrap();
    let a = &saved.node(id).unwrap().annotations[0];
    assert_eq!(a.label, "Existing API note");
    assert_eq!(a.properties, ["width"]);
    assert_eq!(a.category.as_deref(), Some("accessibility"));
    let round: Document = serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
    assert_eq!(&round, saved);
    assert_eq!(u.app.session.undo_depth().0, undo + 1);
    u.app.session.undo().unwrap();
    assert_eq!(u.app.session.doc().unwrap(), &before);
    u.app.session.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap().node(id).unwrap().annotations.len(), 1);
}

#[test]
fn annotation_tool_plain_t_and_existing_comment_gesture_keep_their_owners() {
    let (mut u, _) = setup();
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    canvas_key(&mut u, Key::T, Modifiers::NONE);
    assert_eq!(u.app.session.tool.tool.id(), "text");
    canvas_key(&mut u, Key::C, Modifiers::NONE);
    assert_eq!(u.app.session.tool.tool.id(), "comment");
    let at = u.app.to_screen(40.0, 30.0);
    click(&mut u, at);
    assert!(u.app.inspect()["comments"]["draft"].is_array());
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
}

#[test]
fn annotation_tool_shift_t_stays_owned_by_the_existing_canvas_text_editor() {
    let mut s = Session::new();
    let id = NodeId(s.execute("text.create", &json!({"x":0,"y":0,"text":"Existing"})).unwrap()["id"].as_u64().unwrap());
    s.execute("text.edit.begin", &json!({"id":id,"caret":8})).unwrap();
    let mut u = Ui::new(s);
    u.ctx.set_os(egui::os::OperatingSystem::Mac);
    let at = u.app.to_screen(20.0, 10.0);
    u.frame_with(
        vec![
            Event::PointerMoved(at),
            Event::Key { key: Key::T, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::SHIFT },
            Event::Text("T".into()),
            Event::Key { key: Key::T, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::SHIFT },
        ],
        Modifiers::SHIFT,
    );
    u.frame(vec![]);
    assert_ne!(u.app.session.tool.tool.id(), "annotation");
    assert!(u.app.session.ds().unwrap().text_edit.is_some());
    let node = u.app.session.doc().unwrap().node(id).unwrap();
    assert_eq!(node.kind.text().unwrap().characters, "ExistingT");
    assert!(node.annotations.is_empty());
}

#[test]
fn annotation_tool_genuine_version_preview_keeps_annotation_and_tool_edits_read_only() {
    let mut s = Session::new();
    let id = NodeId(s.execute("shape.rectangle", &json!({"x":0,"y":0,"width":160,"height":100})).unwrap()["id"].as_u64().unwrap());
    s.execute("file.saveVersion", &json!({"name":"Original read-only snapshot"})).unwrap();
    s.execute("version.preview", &json!({"index":0})).unwrap();
    let mut u = Ui::new(s);
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    canvas_key(&mut u, Key::T, Modifiers::SHIFT);
    assert_ne!(u.app.session.tool.tool.id(), "annotation");
    let e = u.app.session.execute("annotation.add", &json!({"id":id,"label":"Must be refused"})).unwrap_err();
    assert!(matches!(e, EngineError::Disabled(..)), "{e:?}");
    // Existing preview policy permits choosing Text; document edits still refuse.
    u.app.session.execute("tool.text", &json!({})).unwrap();
    assert!(u.app.session.execute("text.create", &json!({"text":"Must stay read-only"})).is_err());
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
}

#[test]
fn annotation_tool_existing_tool_identities_and_ordinals_are_stable() {
    let ids = [
        "move",
        "scale",
        "hand",
        "frame",
        "section",
        "slice",
        "rectangle",
        "line",
        "arrow",
        "ellipse",
        "polygon",
        "star",
        "pen",
        "pencil",
        "text",
        "textOnPath",
        "comment",
        "eyedropper",
        "shapeBuilder",
        "sticky",
        "shape",
        "connector",
        "marker",
        "highlighter",
        "eraser",
        "stamp",
    ];
    for (index, tool) in figcraft_engine::tools::ToolId::ALL.into_iter().take(26).enumerate() {
        assert_eq!(tool.id(), ids[index]);
        assert_eq!(tool as usize, index);
    }
}

fn note_field(u: &mut Ui) -> Option<(Pos2, String)> {
    let out = u.frame_output(vec![]);
    let nodes = out.platform_output.accesskit_update.expect("actual accessible frame").nodes;
    let note = nodes
        .iter()
        .map(|(_, n)| n)
        .find(|n| matches!(n.role(), Role::TextInput | Role::MultilineTextInput) && n.label() == Some("Annotation note"))?;
    let r = note.bounds()?;
    Some((pos2(((r.x0 + r.x1) / 2.0) as f32, ((r.y0 + r.y1) / 2.0) as f32), note.value().unwrap_or("").to_owned()))
}

fn open_note(u: &mut Ui) {
    canvas_key(u, Key::T, Modifiers::SHIFT);
    let at = u.app.to_screen(40.0, 30.0);
    click(u, at);
    assert!(note_field(u).is_some(), "actual canvas composer is open");
}

fn type_note(u: &mut Ui, text: &str) {
    let (at, _) = note_field(u).expect("note field");
    click(u, at);
    u.frame(vec![Event::Text(text.into())]);
}

fn accessible_at(u: &mut Ui, label: &str) -> Pos2 {
    let out = u.frame_output(vec![]);
    let nodes = out.platform_output.accesskit_update.unwrap().nodes;
    let matches: Vec<_> = nodes
        .iter()
        .map(|(_, n)| n)
        .filter(|n| {
            (n.label() == Some(label) || (n.role() == Role::ComboBox && n.value() == Some(label)))
                && n.bounds().is_some()
                && matches!(n.role(), Role::Button | Role::ComboBox | Role::MenuItem | Role::MenuItemCheckBox | Role::CheckBox)
        })
        .collect();
    let node = matches
        .first()
        .unwrap_or_else(|| panic!("missing actual control {label}: {:?}", nodes.iter().filter_map(|(_, n)| n.label()).collect::<Vec<_>>()));
    let r = node.bounds().unwrap();
    pos2(((r.x0 + r.x1) / 2.0) as f32, ((r.y0 + r.y1) / 2.0) as f32)
}

#[test]
fn annotation_tool_escape_reopen_retires_uncommitted_note_and_text_state() {
    let (mut u, id) = setup();
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    open_note(&mut u);
    type_note(&mut u, "Discard this draft");
    key(&mut u, Key::Escape, Modifiers::NONE);
    assert!(note_field(&mut u).is_none());
    assert_eq!(u.app.session.tool.tool.id(), "annotation", "first Escape cancels only the owned draft");
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
    let at = u.app.to_screen(40.0, 30.0);
    click(&mut u, at);
    assert_eq!(note_field(&mut u).unwrap().1, "", "new widget incarnation cannot reuse the canceled text");
    key(&mut u, Key::Escape, Modifiers::NONE);
    key(&mut u, Key::Escape, Modifiers::NONE);
    assert_eq!(u.app.session.tool.tool.id(), "move");
    assert!(u.app.session.doc().unwrap().node(id).unwrap().annotations.is_empty());
}

#[test]
fn annotation_tool_shift_enter_is_a_newline_and_plain_enter_is_one_commit() {
    let (mut u, id) = setup();
    let undo = u.app.session.undo_depth().0;
    open_note(&mut u);
    type_note(&mut u, "First");
    key(&mut u, Key::Enter, Modifiers::SHIFT);
    assert!(u.app.session.doc().unwrap().node(id).unwrap().annotations.is_empty());
    u.frame(vec![Event::Text("Second".into())]);
    key(&mut u, Key::Enter, Modifiers::NONE);
    assert_eq!(u.app.session.doc().unwrap().node(id).unwrap().annotations[0].label, "First\nSecond");
    assert_eq!(u.app.session.undo_depth().0, undo + 1);
}

#[test]
fn annotation_tool_ime_preedit_and_commit_enter_cannot_submit_a_draft() {
    let (mut u, id) = setup();
    open_note(&mut u);
    type_note(&mut u, "A");
    let enter = Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
    u.frame(vec![Event::Ime(egui::ImeEvent::Preedit { text: "語".into(), active_range_chars: Some(0..1) }), enter.clone()]);
    assert!(u.app.session.doc().unwrap().node(id).unwrap().annotations.is_empty());
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    u.frame(vec![Event::Ime(egui::ImeEvent::Commit("語".into())), enter]);
    assert!(u.app.session.doc().unwrap().node(id).unwrap().annotations.is_empty(), "the Enter that confirms IME owns no document submit");
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert!(note_field(&mut u).unwrap().1.contains('語'));
    key(&mut u, Key::Enter, Modifiers::NONE);
    assert_eq!(u.app.session.doc().unwrap().node(id).unwrap().annotations.len(), 1);
}

#[test]
fn annotation_tool_picker_escape_keeps_note_and_property_only_submit_is_undoable() {
    let (mut u, id) = setup();
    let undo = u.app.session.undo_depth().0;
    open_note(&mut u);
    let category = accessible_at(&mut u, "No category");
    click(&mut u, category);
    assert!(egui::Popup::is_any_open(&u.ctx));
    key(&mut u, Key::Escape, Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&u.ctx));
    assert!(note_field(&mut u).is_some(), "picker Escape keeps the owning draft");
    let category = accessible_at(&mut u, "No category");
    click(&mut u, category);
    let before_arrow = u.app.session.doc().unwrap().node(id).unwrap().transform;
    let before_arrow_undo = u.app.session.undo_depth();
    key(&mut u, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(u.app.session.doc().unwrap().node(id).unwrap().transform, before_arrow, "picker ArrowDown must not nudge its annotation target");
    assert_eq!(u.app.session.undo_depth(), before_arrow_undo, "picker navigation makes no Undo step");
    assert!(note_field(&mut u).is_some(), "draft must survive picker ArrowDown before Enter");
    key(&mut u, Key::Enter, Modifiers::NONE);
    assert!(u.app.session.doc().unwrap().node(id).unwrap().annotations.is_empty(), "picker Enter cannot submit the canvas draft");
    assert!(note_field(&mut u).is_some());
    let pin = accessible_at(&mut u, "Pin a property");
    click(&mut u, pin);
    let width = accessible_at(&mut u, "Width");
    click(&mut u, width);
    let add = accessible_at(&mut u, "Add annotation");
    click(&mut u, add);
    let a = &u.app.session.doc().unwrap().node(id).unwrap().annotations;
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].label, "");
    assert_eq!(a[0].properties, ["width"]);
    assert_eq!(u.app.session.undo_depth().0, undo + 1);
}

#[test]
fn annotation_tool_context_shift_f10_opens_the_selected_layer_without_an_edit() {
    let (mut u, id) = setup();
    u.app.run("select.set", json!({"ids":[id]})).unwrap();
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    let at = u.app.to_screen(40.0, 30.0);
    u.frame(vec![Event::PointerMoved(at)]);
    key(&mut u, Key::F10, Modifiers::SHIFT);
    let add = accessible_at(&mut u, "Add annotation");
    click(&mut u, add);
    assert!(note_field(&mut u).is_some());
    assert_eq!(u.app.session.tool.tool.id(), "annotation");
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
}

#[test]
fn annotation_tool_document_and_tool_roundtrips_cannot_revive_an_old_draft() {
    for switch in ["document", "tool"] {
        let (mut u, id) = setup();
        open_note(&mut u);
        type_note(&mut u, "Old owner only");
        if switch == "document" {
            u.app.run("file.new", json!({})).unwrap();
            let other = NodeId(u.app.run("shape.rectangle", json!({"width":160,"height":100})).unwrap()["id"].as_u64().unwrap());
            assert_eq!(id, other, "fresh documents deliberately reuse layer IDs");
            u.app.run("doc.switch", json!({"index":0})).unwrap();
        } else {
            u.app.run("tool.move", json!({})).unwrap();
            u.app.run("tool.annotation", json!({})).unwrap();
        }
        let before = u.app.session.doc().unwrap().clone();
        let undo = u.app.session.undo_depth();
        key(&mut u, Key::Enter, Modifiers::NONE);
        assert!(note_field(&mut u).is_none(), "same final ID/tool cannot revive the retired draft");
        assert_eq!(u.app.session.doc().unwrap(), &before);
        assert_eq!(u.app.session.undo_depth(), undo);
    }
}

#[test]
fn annotation_tool_target_delete_and_transient_focus_loss_retire_before_enter() {
    for cancel in ["delete", "focus"] {
        let (mut u, id) = setup();
        open_note(&mut u);
        type_note(&mut u, "Never submit after cancellation");
        if cancel == "delete" {
            u.app.run("edit.delete", json!({"ids":[id]})).unwrap();
        } else {
            u.frame_raw(egui::RawInput {
                focused: true,
                events: vec![Event::WindowFocused(false), Event::WindowFocused(true)],
                ..Default::default()
            });
        }
        let before = u.app.session.doc().unwrap().clone();
        let undo = u.app.session.undo_depth();
        assert!(note_field(&mut u).is_none());
        assert_eq!(u.app.session.doc().unwrap(), &before);
        assert_eq!(u.app.session.undo_depth(), undo);
        key(&mut u, Key::Enter, Modifiers::NONE);
        assert!(
            u.app.session.doc().unwrap().nodes().all(|n| n.annotations.is_empty()),
            "normal Enter after cancellation must never submit the old draft"
        );
    }
}

#[test]
fn annotation_tool_submit_button_cannot_treat_an_ime_confirming_enter_as_a_commit() {
    let (mut u, id) = setup();
    open_note(&mut u);
    type_note(&mut u, "Keep this draft");
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    let mut focused = false;
    for _ in 0..8 {
        let out = u.frame_output(vec![]);
        let tree = out.platform_output.accesskit_update.unwrap();
        let submit = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Button && n.label() == Some("Add annotation"))
            .map(|(id, _)| *id)
            .expect("actual submit button");
        if tree.focus == submit {
            focused = true;
            break;
        }
        key(&mut u, Key::Tab, Modifiers::NONE);
    }
    assert!(focused, "the real Tab sequence must focus the actual submit button before testing Enter");
    u.frame(vec![
        Event::Ime(egui::ImeEvent::Commit("語".into())),
        Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE },
    ]);
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
    assert!(u.app.session.doc().unwrap().node(id).unwrap().annotations.is_empty());
    assert!(note_field(&mut u).is_some());
}

#[test]
fn annotation_tool_retired_submit_focus_is_cleared_before_replacement_widgets() {
    for action in ["tool", "document", "delete"] {
        let (mut u, id) = setup();
        open_note(&mut u);
        type_note(&mut u, "Uncommitted");
        let mut focused = false;
        for _ in 0..8 {
            let out = u.frame_output(vec![]);
            let tree = out.platform_output.accesskit_update.unwrap();
            let submit = tree.nodes.iter().find(|(_, n)| n.role() == Role::Button && n.label() == Some("Add annotation")).map(|(id, _)| *id).unwrap();
            if tree.focus == submit {
                focused = true;
                break;
            }
            key(&mut u, Key::Tab, Modifiers::NONE);
        }
        assert!(focused);
        let old_focus = u.ctx.memory(|m| m.focused()).expect("actual button owns egui focus");
        match action {
            "tool" => {
                u.app.run("tool.move", json!({})).unwrap();
            }
            "document" => {
                u.app.run("file.new", json!({})).unwrap();
            }
            _ => {
                u.app.run("edit.delete", json!({"ids":[id]})).unwrap();
            }
        }
        // This is the same routine called before a replacement frame's navigation/widgets.
        crate::dev_mode::annotation_tool::sync(&mut u.app, &u.ctx);
        assert_ne!(
            u.ctx.memory(|m| m.focused()),
            Some(old_focus),
            "retirement must release the actual submit response ID before any replacement owner"
        );
        let before = u.app.session.doc().unwrap().clone();
        let undo = u.app.session.undo_depth();
        key(&mut u, Key::Enter, Modifiers::NONE);
        assert_eq!(u.app.session.doc().unwrap(), &before);
        assert_eq!(u.app.session.undo_depth(), undo);
    }
}

#[test]
fn annotation_tool_retired_category_popup_closes_only_its_owned_picker() {
    let (mut u, _) = setup();
    open_note(&mut u);
    let at = accessible_at(&mut u, "No category");
    click(&mut u, at);
    assert!(egui::Popup::is_any_open(&u.ctx), "the actual category picker is open");
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    u.app.run("tool.move", json!({})).unwrap();
    crate::dev_mode::annotation_tool::sync(&mut u.app, &u.ctx);
    assert!(!egui::Popup::is_any_open(&u.ctx), "a retired owner's popup cannot keep the new tool's keyboard");
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
}

#[test]
fn annotation_tool_blank_canvas_and_sidebar_cannot_start_an_annotation() {
    let (mut u, _) = setup();
    canvas_key(&mut u, Key::T, Modifiers::SHIFT);
    let canvas = u.app.canvas.rect;
    let empty = pos2(canvas.left() + 20.0, canvas.top() + 20.0);
    let p = u.app.to_page(empty);
    assert!(canvas.contains(empty));
    assert!(u.app.session.hit(u.app.session.doc().unwrap(), u.app.session.page().unwrap(), p, 1.0).is_none(), "actual empty canvas control");
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    click(&mut u, empty);
    assert!(note_field(&mut u).is_none());
    let sidebar = pos2(canvas.left() - 30.0, canvas.bottom() - 60.0);
    assert!(!canvas.contains(sidebar));
    click(&mut u, sidebar);
    assert!(note_field(&mut u).is_none());
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
}

#[test]
fn annotation_tool_page_viewport_and_player_changes_retire_the_canvas_draft() {
    for action in ["page", "viewport", "present"] {
        let (mut u, _) = setup();
        let frame = NodeId(u.app.run("frame.create", json!({"x":300,"y":0,"width":100,"height":100})).unwrap()["id"].as_u64().unwrap());
        u.app.run("select.none", json!({})).unwrap();
        let page = u.app.session.page().unwrap();
        open_note(&mut u);
        type_note(&mut u, "Retired owner");
        match action {
            "page" => {
                u.app.run("page.add", json!({"name":"Another page"})).unwrap();
                u.app.run("page.select", json!({"id":page})).unwrap();
            }
            "viewport" => {
                let viewport = egui::ViewportId::from_hash_of("different-annotation-window");
                let mut raw = egui::RawInput { viewport_id: viewport, ..Default::default() };
                raw.viewports.insert(viewport, Default::default());
                u.frame_raw(raw);
            }
            _ => {
                u.app.run("proto.present", json!({"start":frame})).unwrap();
                assert!(u.app.ui.present.is_some());
                key(&mut u, Key::Escape, Modifiers::NONE);
            }
        }
        assert!(note_field(&mut u).is_none(), "{action} must retire the old draft");
        assert!(u.app.session.doc().unwrap().nodes().all(|n| n.annotations.is_empty()));
    }
}

#[test]
fn annotation_tool_preferences_and_shortcut_recorder_keep_their_escape_key() {
    for command in ["app.preferences", "help.shortcuts"] {
        let (mut u, _) = setup();
        open_note(&mut u);
        type_note(&mut u, "Uncommitted canvas note");
        let before = u.app.session.doc().unwrap().clone();
        let undo = u.app.session.undo_depth();
        u.app.run(command, json!({})).expect("existing dialog command");
        u.frame(vec![]);
        let opened = if command == "app.preferences" { u.app.ui.preferences } else { u.app.ui.shortcuts_panel };
        assert!(opened, "the actual dialog must be open before Escape");
        key(&mut u, Key::Escape, Modifiers::NONE);
        let opened = if command == "app.preferences" { u.app.ui.preferences } else { u.app.ui.shortcuts_panel };
        assert!(!opened, "the dialog keeps its own Escape rather than losing it to a canvas draft");
        assert_eq!(u.app.session.doc().unwrap(), &before);
        assert_eq!(u.app.session.undo_depth(), undo);
    }
}

fn tab_to_button(u: &mut Ui, label: &str) -> egui::Id {
    for _ in 0..10 {
        let out = u.frame_output(vec![]);
        let tree = out.platform_output.accesskit_update.unwrap();
        let button = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Button && n.label() == Some(label))
            .map(|(id, _)| *id)
            .expect("actual button in composer");
        if tree.focus == button {
            return u.ctx.memory(|m| m.focused()).expect("real egui button focus");
        }
        key(u, Key::Tab, Modifiers::NONE);
    }
    panic!("Tab never reached actual {label} button");
}

fn double_click_metadata_target(text: bool) {
    let (mut u, rect) = setup();
    let id = if text {
        NodeId(u.app.run("text.create", json!({"x":0,"y":160,"text":"Original annotated text"})).unwrap()["id"].as_u64().unwrap())
    } else {
        rect
    };
    u.app.run("select.none", json!({})).unwrap();
    canvas_key(&mut u, Key::T, Modifiers::SHIFT);
    let bounds = u.app.session.doc().unwrap().world_bounds(id).unwrap();
    let at = u.app.to_screen(bounds.center().x, bounds.center().y);
    assert!(u.app.canvas.rect.contains(at));
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    u.button(at, true);
    u.button(at, false);
    u.frame(vec![]);
    assert!(note_field(&mut u).is_some());
    u.button(at, true);
    u.button(at, false);
    assert!(u.ctx.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary)), "actual raw double-click before checking tool effects");
    u.frame(vec![]);
    assert_eq!(u.app.session.doc().unwrap(), &before, "Annotation double-click cannot enter geometry/text editing");
    assert_eq!(u.app.session.undo_depth(), undo);
    assert_eq!(u.app.session.tool.tool.id(), "annotation");
    assert!(u.app.session.ds().unwrap().vector_edit.is_none());
    assert!(u.app.session.ds().unwrap().text_edit.is_none());
    assert!(note_field(&mut u).is_some());
}

#[test]
fn annotation_tool_peer_double_click_rectangle_keeps_metadata_authoring() {
    double_click_metadata_target(false);
}

#[test]
fn annotation_tool_peer_double_click_text_keeps_metadata_authoring() {
    double_click_metadata_target(true);
}

#[test]
#[expect(deprecated, reason = "Exercise the pinned native IME cancellation event")]
fn annotation_tool_peer_ime_disabled_cancels_preedit_then_allows_explicit_submit() {
    let (mut u, id) = setup();
    open_note(&mut u);
    type_note(&mut u, "Committed");
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    u.frame(vec![Event::Ime(egui::ImeEvent::Preedit { text: "候補".into(), active_range_chars: Some(0..2) })]);
    u.frame(vec![Event::Ime(egui::ImeEvent::Disabled)]);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
    assert_eq!(note_field(&mut u).unwrap().1, "Committed", "uncommitted canceled IME text cannot become the annotation note");
    key(&mut u, Key::Enter, Modifiers::NONE);
    assert_eq!(u.app.session.doc().unwrap().node(id).unwrap().annotations[0].label, "Committed");
    assert_eq!(u.app.session.undo_depth().0, undo.0 + 1);
}

#[test]
fn annotation_tool_peer_cancel_button_owns_escape_without_leaving_the_tool() {
    let (mut u, _) = setup();
    open_note(&mut u);
    type_note(&mut u, "Uncommitted");
    tab_to_button(&mut u, "Cancel");
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    key(&mut u, Key::Escape, Modifiers::NONE);
    assert!(note_field(&mut u).is_none());
    assert_eq!(u.app.session.tool.tool.id(), "annotation", "first Escape belongs to the composer Cancel control");
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
}

#[test]
fn annotation_tool_peer_cancel_focus_retires_before_tool_or_document_replacement() {
    for command in ["tool.move", "file.new"] {
        let (mut u, _) = setup();
        open_note(&mut u);
        type_note(&mut u, "Uncommitted");
        let focus = tab_to_button(&mut u, "Cancel");
        u.app.run(command, json!({})).unwrap();
        crate::dev_mode::annotation_tool::sync(&mut u.app, &u.ctx);
        assert_ne!(u.ctx.memory(|m| m.focused()), Some(focus), "retire the actual Cancel response before replacement widgets");
    }
}

#[test]
fn annotation_tool_peer_board_refuses_all_activation_and_compose_entry_points() {
    let mut s = Session::new();
    s.execute("file.newBoard", &json!({"name":"Original board refusal control"})).unwrap();
    let id = NodeId(s.execute("board.shape", &json!({"x":0,"y":0,"text":"Board object"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids":[id]})).unwrap();
    let before = s.doc().unwrap().clone();
    let undo = s.undo_depth();
    let tool = s.tool.tool;
    let selection = s.selection();
    s.ui_requests.clear();
    for (command, params) in [("tool.annotation", json!({})), ("tool.set", json!({"tool":"annotation"})), ("annotation.new", json!({"id":id}))] {
        assert!(s.execute(command, &params).is_err(), "Board must refuse {command}");
        assert_eq!(s.doc().unwrap(), &before);
        assert_eq!(s.undo_depth(), undo);
        assert_eq!(s.tool.tool, tool);
        assert_eq!(s.selection(), selection);
        assert!(s.ui_requests.is_empty());
    }
    let mut u = Ui::new(s);
    u.ctx.set_os(egui::os::OperatingSystem::Mac);
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    canvas_key(&mut u, Key::T, Modifiers::SHIFT);
    assert_eq!(u.app.session.tool.tool, tool);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
}

#[test]
fn annotation_tool_peer_spanish_ui_catalog_rows_are_real_ui_translations() {
    let spanish = crate::i18n::languages().iter().find(|language| language.code == "es").unwrap();
    for (key, expected) in [
        ("Annotation", "Anotación"),
        ("Annotation note", "Nota de anotación"),
        ("Add annotation", "Añadir anotación"),
        ("Annotation category", "Categoría de anotación"),
    ] {
        let actual = spanish.ui.iter().find(|(entry, _)| *entry == key).and_then(|(_, text)| match text {
            crate::i18n::Text::Plain(value) => Some(*value),
            _ => None,
        });
        assert_eq!(actual, Some(expected), "tr/try_tr read the UI catalog, not menu entries: {key}");
    }
}

fn foreign_picker_frame(u: &mut Ui, time: &mut f64, mut events: Vec<Event>) -> (egui::Rect, egui::Id, bool) {
    *time += 1.0 / 60.0;
    u.advance_clock(*time);
    events.insert(0, Event::ModifiersChanged(Modifiers::NONE));
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
        time: Some(*time),
        events,
        ..Default::default()
    };
    let mut button = None;
    let mut popup = None;
    let mut saw_arrow = false;
    let ctx = &u.ctx;
    let app = &mut u.app;
    let mut output = ctx.run_ui(raw, |ui| {
        app.ui(ui);
        // A separate real modal owns this picker and draws after the app, like global editors.
        egui::Modal::new(egui::Id::unique("annotation_foreign_modal_control")).show(ctx, |ui| {
            let response = ui.button("Foreign picker choices");
            button = Some(response.rect);
            popup = Some(egui::Popup::default_response_id(&response));
            egui::Popup::menu(&response).show(|ui| {
                saw_arrow = ui.input(|input| input.events.iter().any(|event| matches!(event, Event::Key { key: Key::ArrowDown, pressed: true, .. })));
                let _ = ui.button("Foreign first choice");
                let _ = ui.button("Foreign second choice");
            });
        });
    });
    output.textures_delta.clear();
    (button.expect("actual foreign button"), popup.expect("actual foreign popup"), saw_arrow)
}

#[test]
fn annotation_tool_foreign_popup_keeps_its_input_after_the_annotation_canvas_draws() {
    let (mut u, _) = setup();
    open_note(&mut u);
    type_note(&mut u, "Uncommitted canvas note");
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    let mut time = 10.0;
    foreign_picker_frame(&mut u, &mut time, vec![]);
    let (button, _, _) = foreign_picker_frame(&mut u, &mut time, vec![]);
    let at = button.center();
    for pressed in [true, false] {
        foreign_picker_frame(
            &mut u,
            &mut time,
            vec![
                Event::PointerMoved(at),
                Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE },
            ],
        );
    }
    let (_, popup, _) = foreign_picker_frame(&mut u, &mut time, vec![]);
    assert!(egui::Popup::is_id_open(&u.ctx, popup), "the real foreign popup must be open before the ownership comparison");
    let (_, _, saw_arrow) = foreign_picker_frame(
        &mut u,
        &mut time,
        vec![
            Event::Key { key: Key::ArrowDown, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE },
            Event::Key { key: Key::ArrowDown, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE },
        ],
    );
    assert!(saw_arrow, "the annotation canvas cannot consume a foreign popup's ArrowDown before its rows draw");
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
    assert_eq!(u.app.session.tool.tool.id(), "annotation");
}

// Original combined Motion export and annotation ownership controls.
type AnnotationMotionDownloads = std::rc::Rc<std::cell::RefCell<Vec<(String, Vec<u8>)>>>;

struct AnnotationMotionDownloadSink(AnnotationMotionDownloads);

impl crate::Services for AnnotationMotionDownloadSink {
    fn downloads(&self) -> bool {
        true
    }
    fn download(&mut self, name: &str, bytes: &[u8]) -> bool {
        self.0.borrow_mut().push((name.to_owned(), bytes.to_vec()));
        true
    }
}

fn annotation_motion_setup() -> (Ui, NodeId, AnnotationMotionDownloads) {
    let mut session = Session::new();
    let page = session.page().unwrap();
    let target = {
        let doc = session.doc_mut().unwrap();
        let frame = doc.create(page, figcraft_doc::NodeKind::Frame, "GIF owner frame", 0.0, 0.0, 240.0, 180.0).unwrap();
        let target = doc.create(frame, figcraft_doc::NodeKind::Rectangle, "Annotation target", 0.0, 0.0, 160.0, 100.0).unwrap();
        doc.node_mut(target).unwrap().fills = vec![figcraft_color::Paint::solid(figcraft_color::Color::from_rgba8(66, 160, 128, 255))];
        doc.create(page, figcraft_doc::NodeKind::Frame, "GIF second frame", 320.0, 0.0, 240.0, 180.0).unwrap();
        target
    };
    session.execute("motion.timeline", &json!({"duration":200,"fps":10,"loop":true})).unwrap();
    session.execute("select.none", &json!({})).unwrap();
    let mut u = Ui::new(session);
    u.ctx.set_os(egui::os::OperatingSystem::Mac);
    u.ctx.enable_accesskit();
    let downloads = AnnotationMotionDownloads::default();
    u.app.services = Box::new(AnnotationMotionDownloadSink(downloads.clone()));
    // Motion is already open when the note starts: the exporter must retire the draft.
    crate::motion::set_open(&mut u.app, true);
    u.frame(vec![]);
    (u, target, downloads)
}

fn annotation_motion_window_control(u: &mut Ui, role: Role, label: Option<&str>) -> Pos2 {
    let out = u.frame_output(vec![]);
    let nodes = out.platform_output.accesskit_update.expect("actual composed AccessKit frame").nodes;
    let window = nodes
        .iter()
        .map(|(_, n)| n)
        .find(|n| n.role() == Role::Window && n.label() == Some("Export animation"))
        .expect("actual Motion export window")
        .bounds()
        .expect("Motion export window bounds");
    let window = egui::Rect::from_min_max(pos2(window.x0 as f32, window.y0 as f32), pos2(window.x1 as f32, window.y1 as f32));
    let matches: Vec<_> = nodes
        .iter()
        .map(|(_, n)| n)
        .filter(|n| n.role() == role && label.is_none_or(|label| n.label() == Some(label)))
        .filter_map(|n| n.bounds())
        .map(|r| egui::Rect::from_min_max(pos2(r.x0 as f32, r.y0 as f32), pos2(r.x1 as f32, r.y1 as f32)))
        .filter(|r| window.contains(r.center()))
        .collect();
    assert_eq!(matches.len(), 1, "one actual {role:?} {label:?} inside Motion export; bounds={matches:?}");
    let control = matches[0];
    let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0));
    assert!(screen.contains(control.center()), "actual control is on screen");
    control.center()
}

fn annotation_motion_open_export(u: &mut Ui) {
    assert!(u.app.ui.motion.export.is_none());
    // Before the dialog opens, only the timeline has an Export GIF button.
    let at = accessible_at(u, "Export GIF…");
    click(u, at);
    assert!(u.app.ui.motion.export.is_some(), "the actual timeline button opened Motion export");
    annotation_motion_window_control(u, Role::Button, Some("Export GIF…"));
}

#[test]
fn annotation_tool_motion_export_retires_old_draft_before_enter_and_escape_then_allows_fresh_annotation() {
    let (mut u, target, downloads) = annotation_motion_setup();
    open_note(&mut u);
    assert_eq!(u.app.session.selection(), vec![target], "actual canvas click captured the intended child");
    type_note(&mut u, "Old note must not submit behind Motion export");
    assert!(note_field(&mut u).unwrap().1.contains("Old note"));
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();

    annotation_motion_open_export(&mut u);
    assert!(note_field(&mut u).is_none(), "Motion export replaces the old annotation editor owner");
    key(&mut u, Key::Enter, Modifiers::NONE);
    assert!(u.app.ui.motion.export.is_some());
    assert!(u.app.session.doc().unwrap().node(target).unwrap().annotations.is_empty());
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
    assert!(downloads.borrow().is_empty(), "old note Enter cannot export either");

    key(&mut u, Key::Escape, Modifiers::NONE);
    assert!(u.app.ui.motion.export.is_none(), "Escape closes only the Motion export owner");
    assert!(note_field(&mut u).is_none());
    assert_eq!(u.app.session.tool.tool.id(), "annotation");
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
    assert!(downloads.borrow().is_empty());

    let at = u.app.to_screen(40.0, 30.0);
    click(&mut u, at);
    assert_eq!(note_field(&mut u).expect("fresh annotation after Motion cancel").1, "");
    type_note(&mut u, "Fresh annotation after Motion cancel");
    key(&mut u, Key::Enter, Modifiers::NONE);
    let annotations = &u.app.session.doc().unwrap().node(target).unwrap().annotations;
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].label, "Fresh annotation after Motion cancel");
    assert_eq!(u.app.session.undo_depth().0, undo.0 + 1);
    u.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap(), &before, "one Undo removes only the fresh annotation");
}

#[test]
fn annotation_tool_motion_export_picker_owns_navigation_and_escape_without_committing_a_note() {
    let (mut u, target, downloads) = annotation_motion_setup();
    open_note(&mut u);
    assert_eq!(u.app.session.selection(), vec![target], "actual canvas click captured the intended child");
    type_note(&mut u, "Uncommitted annotation picker owner");
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    let category = accessible_at(&mut u, "No category");
    click(&mut u, category);
    assert!(egui::Popup::is_any_open(&u.ctx));
    key(&mut u, Key::Escape, Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&u.ctx));
    assert!(note_field(&mut u).is_some(), "existing annotation picker keeps its first Escape");

    annotation_motion_open_export(&mut u);
    assert!(note_field(&mut u).is_none());
    let frame = annotation_motion_window_control(&mut u, Role::ComboBox, None);
    click(&mut u, frame);
    assert!(egui::Popup::is_any_open(&u.ctx), "actual GIF frame picker opened");
    key(&mut u, Key::ArrowDown, Modifiers::NONE);
    key(&mut u, Key::Enter, Modifiers::NONE);
    assert!(u.app.ui.motion.export.is_some());
    assert!(note_field(&mut u).is_none());
    assert!(u.app.session.doc().unwrap().node(target).unwrap().annotations.is_empty());
    assert_eq!(u.app.session.doc().unwrap(), &before, "picker navigation cannot nudge the old target");
    assert_eq!(u.app.session.undo_depth(), undo);
    assert!(downloads.borrow().is_empty());

    if !egui::Popup::is_any_open(&u.ctx) {
        let frame = annotation_motion_window_control(&mut u, Role::ComboBox, None);
        click(&mut u, frame);
    }
    assert!(egui::Popup::is_any_open(&u.ctx));
    key(&mut u, Key::Escape, Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&u.ctx));
    assert!(u.app.ui.motion.export.is_some(), "picker Escape keeps its parent and cannot reach Annotation");
    assert_eq!(u.app.session.tool.tool.id(), "annotation");
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
    assert!(downloads.borrow().is_empty());
    key(&mut u, Key::Escape, Modifiers::NONE);
    assert!(u.app.ui.motion.export.is_none());
    assert!(note_field(&mut u).is_none());
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
}

#[test]
fn annotation_tool_motion_export_fresh_download_still_works_after_replacing_and_cancelling_annotation() {
    let (mut u, target, downloads) = annotation_motion_setup();
    open_note(&mut u);
    assert_eq!(u.app.session.selection(), vec![target], "actual canvas click captured the intended child");
    type_note(&mut u, "Retired before the export positive control");
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    annotation_motion_open_export(&mut u);
    assert!(note_field(&mut u).is_none());
    key(&mut u, Key::Escape, Modifiers::NONE);
    assert!(u.app.ui.motion.export.is_none());
    assert!(downloads.borrow().is_empty());

    annotation_motion_open_export(&mut u);
    let export = annotation_motion_window_control(&mut u, Role::Button, Some("Export GIF…"));
    click(&mut u, export);
    assert!(u.app.ui.motion.export.is_none());
    {
        let captured = downloads.borrow();
        assert_eq!(captured.len(), 1, "a genuinely new Export click still delivers one GIF");
        let (name, bytes) = &captured[0];
        assert!(name.ends_with(".gif"));
        assert!(bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a"));
        assert!(bytes.len() >= 10);
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), 240);
        assert_eq!(u16::from_le_bytes([bytes[8], bytes[9]]), 180);
    }
    assert!(note_field(&mut u).is_none());
    assert_eq!(u.app.session.tool.tool.id(), "annotation");
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo, "export does not add a document edit");
}

// V2 staged extension: GIF V2 leaves the Motion UI API unchanged.
// This is a proposed same-batch cancellation counterfactual, not an executed RED.
#[test]
fn annotation_tool_motion_export_release_with_escape_cancels_before_delivery_then_allows_fresh_export() {
    let (mut u, target, downloads) = annotation_motion_setup();
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    annotation_motion_open_export(&mut u);
    let export = annotation_motion_window_control(&mut u, Role::Button, Some("Export GIF…"));

    u.button(export, true);
    assert!(downloads.borrow().is_empty(), "the painted Export button is only held");
    assert!(u.app.ui.motion.export.is_some());
    // The actual release and cancellation are coalesced in one real egui input batch.
    u.frame(vec![
        Event::PointerMoved(export),
        Event::PointerButton { pos: export, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
        Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE },
        Event::Key { key: Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE },
    ]);
    u.frame(vec![]);
    assert!(downloads.borrow().is_empty(), "Escape must veto the same-batch Export release");
    assert!(u.app.pending_dialog.is_none(), "cancellation cannot queue a native picker either");
    assert!(u.app.ui.motion.export.is_none());
    assert!(u.app.session.doc().unwrap().node(target).unwrap().annotations.is_empty());
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);

    annotation_motion_open_export(&mut u);
    let export = annotation_motion_window_control(&mut u, Role::Button, Some("Export GIF…"));
    click(&mut u, export);
    assert!(u.app.ui.motion.export.is_none());
    assert!(u.app.pending_dialog.is_none());
    {
        let captured = downloads.borrow();
        assert_eq!(captured.len(), 1, "a fresh actual click must still deliver exactly one GIF");
        let (name, bytes) = &captured[0];
        assert!(name.ends_with(".gif"));
        assert!(bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a"));
        assert!(bytes.len() >= 10);
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), 240);
        assert_eq!(u16::from_le_bytes([bytes[8], bytes[9]]), 180);
    }
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
}

// Export ownership when its timeline or interface disappears.
fn annotation_motion_timeline_close(u: &mut Ui) -> Pos2 {
    let out = u.frame_output(vec![]);
    let nodes = out.platform_output.accesskit_update.expect("actual timeline/export AccessKit frame").nodes;
    let rect = |r: egui::accesskit::Rect| egui::Rect::from_min_max(pos2(r.x0 as f32, r.y0 as f32), pos2(r.x1 as f32, r.y1 as f32));
    let anchors: Vec<_> = nodes
        .iter()
        .map(|(_, n)| n)
        .filter(|n| n.role() == Role::Button && n.label() == Some("Collapse layers"))
        .filter_map(|n| n.bounds())
        .map(rect)
        .collect();
    assert_eq!(anchors.len(), 1, "one actual timeline header anchor");
    let anchor = anchors[0];
    let windows: Vec<_> = nodes
        .iter()
        .map(|(_, n)| n)
        .filter(|n| n.role() == Role::Window && n.label() == Some("Export animation"))
        .filter_map(|n| n.bounds())
        .map(rect)
        .collect();
    assert_eq!(windows.len(), 1, "the exporter is visibly open before its timeline closes");
    let closes: Vec<_> = nodes
        .iter()
        .filter(|(_, n)| n.role() == Role::Button && n.label() == Some("Close"))
        .filter_map(|(id, n)| n.bounds().map(|r| (*id, rect(r))))
        .filter(|(_, r)| r.min.x > anchor.max.x && r.y_range().contains(anchor.center().y))
        .filter(|(_, r)| !windows[0].contains(r.center()))
        .collect();
    assert_eq!(closes.len(), 1, "one actual Close to the right of the timeline header anchor: {closes:?}");
    let (wanted, close_rect) = closes[0];
    let at = close_rect.center();
    assert!(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0)).contains(at));
    // Let egui hit-test this actual target before attempting the click. A painted
    // accessibility rectangle alone does not establish pointer ownership.
    u.frame(vec![Event::PointerMoved(at)]);
    u.frame(vec![]);
    let hits = u.ctx.interaction_snapshot(|i| i.hovered.iter().copied().collect::<Vec<_>>());
    let descriptions: Vec<_> = hits
        .iter()
        .map(|id| {
            let label = nodes.iter().find(|(node_id, _)| *node_id == id.accesskit_id()).and_then(|(_, n)| n.label());
            let response = u.ctx.read_response(*id).map(|r| (r.rect, r.interact_rect, r.sense));
            format!("{id:?} label={label:?} response={response:?}")
        })
        .collect();
    assert!(
        hits.iter().any(|id| id.accesskit_id() == wanted),
        "painted timeline Close must own the actual pointer before click: wanted={wanted:?}, at={at:?}, close={close_rect:?}, hovered={descriptions:?}"
    );
    at
}

#[test]
fn annotation_tool_motion_export_timeline_close_retires_exporter_and_restores_fresh_canvas_keys() {
    let (mut u, _, downloads) = annotation_motion_setup();
    assert!(u.app.ui.motion_open);
    assert_eq!(u.app.session.tool.tool.id(), "move");
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    annotation_motion_open_export(&mut u);
    let close = annotation_motion_timeline_close(&mut u);
    click(&mut u, close);
    assert!(!u.app.ui.motion_open, "the painted timeline Close actually changed Motion mode");
    assert!(u.app.ui.motion.export.is_none(), "closing the owning timeline retires its exporter");
    assert!(downloads.borrow().is_empty());
    assert!(u.app.pending_dialog.is_none());
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);

    canvas_key(&mut u, Key::K, Modifiers::NONE);
    assert_eq!(u.app.session.tool.tool.id(), "scale", "a fresh canvas K is no longer owned by the closed exporter");
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
    assert!(downloads.borrow().is_empty());
}

#[test]
fn annotation_tool_motion_export_hide_ui_retires_exporter_and_restores_fresh_canvas_keys() {
    let (mut u, _, downloads) = annotation_motion_setup();
    assert!(!u.app.session.ds().unwrap().view.ui_hidden);
    assert_eq!(u.app.session.tool.tool.id(), "move");
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    annotation_motion_open_export(&mut u);
    // Verified existing public command: cmd/view.rs view.toggleUi accepts {on?}.
    // Use explicit true so a mistaken toggle cannot accidentally show the interface.
    u.app.run("view.toggleUi", json!({"on":true})).unwrap();
    u.frame(vec![]);
    assert!(u.app.session.ds().unwrap().view.ui_hidden, "the public Hide UI command actually hid the interface");
    assert!(u.app.ui.motion.export.is_none(), "Hide UI cannot retain an invisible exporter input owner");
    assert!(downloads.borrow().is_empty());
    assert!(u.app.pending_dialog.is_none());
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);

    canvas_key(&mut u, Key::K, Modifiers::NONE);
    assert_eq!(u.app.session.tool.tool.id(), "scale", "fresh canvas input survives Hide UI");
    assert!(u.app.session.ds().unwrap().view.ui_hidden);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
    assert!(downloads.borrow().is_empty());
}
