//! Real popover controls for key phase, hold duration and capture ownership.

use egui::{Event, Key, Modifiers, Pos2};
use figcraft_doc::NodeId;
use figcraft_doc::proto::Trigger;
use figcraft_engine::Session;
use serde_json::json;

use super::{editor, key_owner, open_editor};
use crate::present_tests::{Ui, id};

fn setup() -> (Ui, NodeId) {
    let mut session = Session::new();
    let node = id(&session.execute("frame.create", &json!({"name":"Original key authoring fixture","width":300,"height":200})).unwrap());
    session.execute("proto.interaction.add", &json!({"id":node,"trigger":{"type":"keyPress","key":"A"},"action":{"type":"back"}})).unwrap();
    session.execute("select.set", &json!({"ids":[node]})).unwrap();
    let mut ui = Ui::new(session);
    ui.app.ui.right_tab = crate::RightTab::Prototype;
    ui.ctx.enable_accesskit();
    open_editor(&ui.ctx, Some((node, 0)));
    ui.frames(3, Vec::new);
    (ui, node)
}

fn trigger(ui: &Ui, node: NodeId) -> Trigger {
    ui.app.session.doc().unwrap().node(node).unwrap().interactions[0].trigger.clone()
}

fn click_at(ui: &mut Ui, point: Pos2) {
    assert!(ui.ctx.content_rect().contains(point), "the real control must be visible: {point:?}");
    ui.button(point, true);
    ui.button(point, false);
    ui.frames(2, Vec::new);
}

fn click(ui: &mut Ui, text: &str) {
    let tree = ui.frame_output(Vec::new()).platform_output.accesskit_update.unwrap().nodes;
    let node = tree
        .into_iter()
        .map(|(_, node)| node)
        .find(|node| node.label() == Some(text) || node.value() == Some(text))
        .unwrap_or_else(|| panic!("no visible control {text:?}"));
    let bounds = node.bounds().unwrap();
    click_at(ui, egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32));
}

fn press(ui: &mut Ui, key: Key, modifiers: Modifiers) {
    let event = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
    ui.frame(vec![event(true), event(false)]);
    ui.frames(2, Vec::new);
}

fn focus(ui: &mut Ui, focused: bool) {
    ui.time += 1.0 / 60.0;
    let raw = egui::RawInput { screen_rect: Some(ui.ctx.content_rect()), time: Some(ui.time), focused, ..Default::default() };
    let app = &mut ui.app;
    let mut output = ui.ctx.run_ui(raw, |screen| app.ui(screen));
    output.textures_delta.clear();
}

fn minimum(ui: &Ui, node: NodeId, phase: &str) -> egui::Id {
    let owner = key_owner(&ui.app, &ui.ctx, node, 0).with(phase);
    super::minimum_hold_id(&ui.app, &ui.ctx, owner, node).unwrap()
}

#[test]
fn prototype_key_timing_editor_authors_phases_duration_controller_and_recorded_combination() {
    let (mut ui, node) = setup();
    click(&mut ui, "Key/Gamepad");
    click(&mut ui, "Key down");
    assert_eq!(trigger(&ui, node), Trigger::KeyDown { key: "A".into(), ms: 0 });
    let field = minimum(&ui, node, "Key down");
    let point = ui.ctx.read_response(field.with("field")).unwrap().rect.center();
    click_at(&mut ui, point);
    ui.frame(vec![Event::Text("500".into())]);
    press(&mut ui, Key::Enter, Modifiers::NONE);
    assert_eq!(trigger(&ui, node), Trigger::KeyDown { key: "A".into(), ms: 500 });
    let accepted = minimum(&ui, node, "Key down");
    let point = ui.ctx.read_response(accepted.with("field")).unwrap().rect.center();
    click_at(&mut ui, point);
    press(&mut ui, Key::ArrowUp, Modifiers::NONE);
    assert_eq!(trigger(&ui, node), Trigger::KeyDown { key: "A".into(), ms: 510 });
    assert_eq!(minimum(&ui, node, "Key down"), accepted, "the field's own accepted step keeps its identity");
    press(&mut ui, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(trigger(&ui, node), Trigger::KeyDown { key: "A".into(), ms: 500 });
    press(&mut ui, Key::Enter, Modifiers::NONE);
    click(&mut ui, "Key down");
    click(&mut ui, "Key up");
    assert_eq!(trigger(&ui, node), Trigger::KeyUp { key: "A".into(), ms: 500 });
    click(&mut ui, "Controller");
    click(&mut ui, "Gamepad A");
    assert_eq!(trigger(&ui, node), Trigger::KeyUp { key: "Gamepad A".into(), ms: 500 });
    click(&mut ui, "Record key");
    press(&mut ui, Key::K, Modifiers::SHIFT);
    assert_eq!(trigger(&ui, node), Trigger::KeyUp { key: "Shift+K".into(), ms: 500 });
    assert!(ui.app.session.undo().unwrap());
    assert_eq!(trigger(&ui, node), Trigger::KeyUp { key: "Gamepad A".into(), ms: 500 }, "recording is an ordinary undoable command");
}

#[test]
fn prototype_key_timing_editor_capture_cannot_cross_reopening_or_source_replacement() {
    for replace in ["reopen", "source", "focus"] {
        let (mut ui, node) = setup();
        click(&mut ui, "Record key");
        assert!(super::capturing(&ui.ctx));
        match replace {
            "source" => {
                ui.run("node.set", json!({"id":node,"props":{"name":"Replaced original interaction owner"}}));
            }
            "focus" => {
                focus(&mut ui, false);
                focus(&mut ui, true);
            }
            _ => {
                open_editor(&ui.ctx, None);
                open_editor(&ui.ctx, Some((node, 0)));
                ui.frames(2, Vec::new);
            }
        }
        assert!(!super::capturing(&ui.ctx), "{replace} must retire capture");
        press(&mut ui, Key::B, Modifiers::NONE);
        assert_eq!(trigger(&ui, node), Trigger::KeyPress { key: "A".into() });
        assert_eq!(editor(&ui.ctx), Some((node, 0)));
    }
}

#[test]
fn prototype_key_timing_editor_reopen_does_not_commit_an_unfinished_hold_draft() {
    let (mut ui, node) = setup();
    ui.run("proto.interaction.set", json!({"id":node,"index":0,"trigger":{"type":"keyDown","key":"A","ms":500}}));
    let before = ui.app.session.undo_depth();
    let old = minimum(&ui, node, "Key down");
    let point = ui.ctx.read_response(old.with("field")).unwrap().rect.center();
    click_at(&mut ui, point);
    ui.frame(vec![Event::Text("999".into())]);
    assert_eq!(trigger(&ui, node), Trigger::KeyDown { key: "A".into(), ms: 500 });
    press(&mut ui, Key::Escape, Modifiers::NONE);
    assert_eq!(trigger(&ui, node), Trigger::KeyDown { key: "A".into(), ms: 500 });
    open_editor(&ui.ctx, None);
    open_editor(&ui.ctx, Some((node, 0)));
    ui.frames(2, Vec::new);
    let new = minimum(&ui, node, "Key down");
    assert_ne!(old, new, "each opening owns its own unfinished number draft");
    press(&mut ui, Key::Enter, Modifiers::NONE);
    assert_eq!(trigger(&ui, node), Trigger::KeyDown { key: "A".into(), ms: 500 });
    assert_eq!(ui.app.session.undo_depth(), before);
}

#[test]
fn prototype_key_timing_editor_source_replacement_retires_an_unfinished_hold_draft() {
    let (mut ui, node) = setup();
    ui.run("proto.interaction.set", json!({"id":node,"index":0,"trigger":{"type":"keyDown","key":"A","ms":500}}));
    let old = minimum(&ui, node, "Key down");
    let point = ui.ctx.read_response(old.with("field")).unwrap().rect.center();
    click_at(&mut ui, point);
    ui.frame(vec![Event::Text("999".into())]);
    assert_eq!(trigger(&ui, node), Trigger::KeyDown { key: "A".into(), ms: 500 });
    ui.run("proto.interaction.set", json!({"id":node,"index":0,"trigger":{"type":"keyDown","key":"B","ms":200}}));
    let history = ui.app.session.undo_depth();
    press(&mut ui, Key::Enter, Modifiers::NONE);
    assert_eq!(trigger(&ui, node), Trigger::KeyDown { key: "B".into(), ms: 200 }, "Enter cannot apply the old owner's999ms draft to its replacement");
    assert_eq!(ui.app.session.undo_depth(), history);
}
