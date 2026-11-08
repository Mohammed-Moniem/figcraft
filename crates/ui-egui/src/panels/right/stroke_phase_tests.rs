//! Original actual-egui manual-phase controls, including draft ownership and mixed values.
use super::*;
use crate::export_preview_tests::{click, labelled};
use crate::layers_drag_tests::Ui as TestUi;
use egui::{Event, Key, Modifiers, RawInput};
use figcraft_engine::Session;

fn scene() -> (TestUi, NodeId, NodeId) {
    let mut s = Session::new();
    let mut make = |x, phase| {
        let id = NodeId(s.execute("shape.rectangle", &json!({"x":x,"width":100,"height":80})).unwrap()["id"].as_u64().unwrap());
        s.execute("node.set", &json!({"ids":[id],"props":{"strokes":["#000000"],"strokeWeight":2,"dashes":[6,3],"dashPhase":phase}})).unwrap();
        id
    };
    let a = make(0, None::<f64>);
    let b = make(150, Some(4.125));
    s.execute("select.set", &json!({"ids":[a]})).unwrap();
    let mut u = TestUi::new(s);
    open(&mut u);
    (u, a, b)
}

fn open(u: &mut TestUi) {
    if !labelled(u).iter().any(|(s, _)| s == "Manual dash phase") {
        let button = u.app.ui.stroke_settings_button.expect("actual advanced stroke button");
        u.button(button.center(), true);
        u.button(button.center(), false);
        for _ in 0..3 {
            u.frame(vec![]);
        }
    }
    assert!(labelled(u).iter().any(|(s, _)| s == "Manual dash phase"));
}

fn toggle(u: &mut TestUi) {
    let at = labelled(u).into_iter().find(|(s, _)| s == "Manual dash phase").expect("actual named phase checkbox").1;
    click(u, at);
    u.frame(vec![]);
}

fn phase(u: &TestUi, id: NodeId) -> Option<f64> {
    u.app.session.doc().unwrap().node(id).unwrap().stroke.dash_phase
}

fn field(u: &TestUi) -> Id {
    let key = Id::unique(("stroke_phase_owner", u.ctx.viewport_id()));
    u.ctx.data(|d| d.get_temp::<PhaseField>(key)).and_then(|s| s.id()).expect("active scoped phase field")
}

fn key(u: &mut TestUi, key: Key) {
    u.frame(vec![
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE },
        Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE },
    ]);
    u.frame(vec![]);
}

fn focus(u: &mut TestUi) -> egui::Pos2 {
    let at = u.ctx.read_response(field(u).with("field")).expect("actual phase value response").rect.center();
    u.button(at, true);
    u.button(at, false);
    u.frame(vec![]);
    at
}

fn draft(u: &mut TestUi, text: &str) {
    focus(u);
    u.frame(vec![Event::Text(text.into())]);
}

#[test]
fn manual_phase_actual_checkbox_fractional_value_arrows_and_single_undo() {
    let (mut u, a, _) = scene();
    let before = u.app.session.doc().unwrap().clone();
    let history = u.app.session.undo_depth();
    assert_eq!(phase(&u, a), None);
    u.frame(vec![]);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    toggle(&mut u);
    assert_eq!(phase(&u, a), Some(0.0));
    assert_eq!(u.app.session.undo_depth().0, history.0 + 1);
    u.ctx.enable_accesskit();
    let access = u.frame_output(vec![]).platform_output.accesskit_update.unwrap();
    assert!(access.nodes.iter().any(|(_, n)| n.role() == egui::accesskit::Role::SpinButton && n.label() == Some("Phase")));
    draft(&mut u, "0.125");
    key(&mut u, Key::Enter);
    assert_eq!(phase(&u, a), Some(0.125));
    assert_eq!(u.app.session.undo_depth().0, history.0 + 2);
    let labels = labelled(&mut u);
    assert!(labels.iter().any(|(s, _)| s == "0.125px"), "display keeps exact fractional phase with its unit: {labels:?}");
    draft(&mut u, "0.125");
    let owner = field(&u);
    key(&mut u, Key::ArrowUp);
    assert_eq!(phase(&u, a), Some(1.125));
    assert_eq!(field(&u), owner, "own accepted step keeps the editor for continuation");
    key(&mut u, Key::ArrowDown);
    assert_eq!(phase(&u, a), Some(0.125));
    assert_eq!(field(&u), owner);
    assert_eq!(u.app.session.doc().unwrap().node(a).unwrap().stroke.dashes, [6.0, 3.0]);
    key(&mut u, Key::Enter);
    toggle(&mut u);
    assert_eq!(phase(&u, a), None, "return to automatic removes the setting");
}

#[test]
fn manual_phase_mixed_mode_enable_preserves_existing_offsets_and_steps_relatively() {
    let (mut u, a, b) = scene();
    u.app.run("select.set", json!({"ids":[a,b]}));
    u.frame(vec![]);
    open(&mut u);
    let history = u.app.session.undo_depth();
    toggle(&mut u);
    assert_eq!((phase(&u, a), phase(&u, b)), (Some(0.0), Some(4.125)), "enable missing manual modes without resetting existing ones");
    assert_eq!(u.app.session.undo_depth().0, history.0 + 1);
    let at = u.ctx.read_response(field(&u).with("field")).unwrap().rect.center();
    u.button(at, true);
    u.button(at, false);
    let history = u.app.session.undo_depth();
    key(&mut u, Key::ArrowUp);
    assert_eq!((phase(&u, a), phase(&u, b)), (Some(1.0), Some(5.125)));
    assert_eq!(u.app.session.undo_depth().0, history.0 + 1, "one relative edit is one Undo across both layers");
    u.app.run("edit.undo", json!({}));
    assert_eq!((phase(&u, a), phase(&u, b)), (Some(0.0), Some(4.125)));
}

#[test]
fn manual_phase_escape_and_reopening_discard_an_uncommitted_draft() {
    let (mut u, a, _) = scene();
    toggle(&mut u);
    let before = u.app.session.doc().unwrap().clone();
    let history = u.app.session.undo_depth();
    draft(&mut u, "999");
    key(&mut u, Key::Escape);
    open(&mut u);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), history);
    assert_eq!(phase(&u, a), Some(0.0));
    assert!(!labelled(&mut u).iter().any(|(s, _)| s == "999"));
}

#[test]
fn manual_phase_selection_switch_and_return_do_not_reuse_an_old_draft() {
    let (mut u, a, b) = scene();
    toggle(&mut u);
    draft(&mut u, "999");
    u.app.run("select.set", json!({"ids":[b]}));
    u.frame(vec![]);
    open(&mut u);
    key(&mut u, Key::Enter);
    assert_eq!(phase(&u, b), Some(4.125));
    u.app.run("select.set", json!({"ids":[a]}));
    u.frame(vec![]);
    open(&mut u);
    key(&mut u, Key::Enter);
    assert_eq!(phase(&u, a), Some(0.0));
    assert!(!labelled(&mut u).iter().any(|(s, _)| s == "999"));
}

#[test]
fn manual_phase_external_value_change_retires_the_focused_draft_before_enter() {
    let (mut u, a, _) = scene();
    toggle(&mut u);
    draft(&mut u, "999");
    let old = field(&u);
    u.app.run("node.set", json!({"ids":[a],"props":{"dashPhase":2.5}}));
    let history = u.app.session.undo_depth();
    key(&mut u, Key::Enter);
    assert_eq!(phase(&u, a), Some(2.5));
    assert_eq!(u.app.session.undo_depth(), history);
    assert_ne!(field(&u), old);
    assert_ne!(u.ctx.memory(|m| m.focused()), Some(old.with("te")));
}

#[test]
fn manual_phase_window_focus_loss_retires_an_uncommitted_editor() {
    let (mut u, a, _) = scene();
    toggle(&mut u);
    draft(&mut u, "999");
    let old = field(&u);
    let history = u.app.session.undo_depth();
    let before = u.app.session.doc().unwrap().clone();
    u.frame_raw(RawInput { focused: false, ..Default::default() });
    assert_eq!(u.app.session.doc().unwrap(), &before, "focus loss must not commit a draft");
    assert_ne!(u.ctx.memory(|m| m.focused()), Some(old.with("te")));
    u.frame_raw(RawInput { focused: true, ..Default::default() });
    open(&mut u);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), history);
    focus(&mut u);
    key(&mut u, Key::Enter);
    assert_eq!(phase(&u, a), Some(0.0));
    assert_eq!(u.app.session.undo_depth(), history, "unexpected history: {:?}", u.app.session.undo_labels(3));
}

#[test]
fn manual_phase_document_switch_with_reused_ids_and_values_retires_the_old_draft() {
    let (mut u, a, _) = scene();
    toggle(&mut u);
    draft(&mut u, "999");
    let old = field(&u);
    let old_uid = u.app.session.ds().unwrap().uid;
    u.app.run("file.new", json!({"name":"Original other phase owner"}));
    let b = NodeId(u.app.run("shape.rectangle", json!({"width":100,"height":80})).unwrap()["id"].as_u64().unwrap());
    u.app.run("node.set", json!({"id":b,"props":{"strokes":["#000000"],"dashes":[6,3],"dashPhase":0}}));
    u.app.run("select.set", json!({"ids":[b]}));
    assert_eq!(a, b, "original fixture deliberately reuses its document-local layer id");
    assert_ne!(u.app.session.ds().unwrap().uid, old_uid);
    u.frame(vec![]);
    open(&mut u);
    assert_ne!(field(&u), old);
    assert!(!labelled(&mut u).iter().any(|(s, _)| s == "999"));
    // The old editor is retired. Reenter the actual new owner's field before Enter:
    // unowned Enter on the canvas intentionally invokes the existing Edit vector action.
    focus(&mut u);
    let history = u.app.session.undo_depth();
    let before = u.app.session.doc().unwrap().clone();
    key(&mut u, Key::Enter);
    assert_eq!(phase(&u, b), Some(0.0));
    assert_eq!(u.app.session.doc().unwrap(), &before, "unowned Enter must not edit the document");
    assert_eq!(u.app.session.undo_depth(), history, "unexpected history: {:?}", u.app.session.undo_labels(3));
    assert_ne!(field(&u), old);
}

#[test]
fn manual_phase_transient_window_focus_loss_discards_the_draft_in_one_batch() {
    let (mut u, a, _) = scene();
    toggle(&mut u);
    draft(&mut u, "999");
    let old = field(&u);
    let before = u.app.session.doc().unwrap().clone();
    let history = u.app.session.undo_depth();
    u.frame_raw(RawInput { focused: true, events: vec![Event::WindowFocused(false), Event::WindowFocused(true)], ..Default::default() });
    assert_ne!(field(&u), old, "a reported focus-loss boundary retires the old editor even after focus returned");
    assert_ne!(u.ctx.memory(|m| m.focused()), Some(old.with("te")));
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), history);
    focus(&mut u);
    key(&mut u, Key::Enter);
    assert_eq!(phase(&u, a), Some(0.0));
    assert_eq!(u.app.session.undo_depth(), history);
}

#[test]
fn manual_phase_mixed_focused_wheel_keeps_relative_values_and_one_gesture_undo() {
    let (mut u, a, b) = scene();
    u.app.run("pref.numberWheel", json!({"on":true}));
    u.app.run("select.set", json!({"ids":[a,b]}));
    u.frame(vec![]);
    open(&mut u);
    toggle(&mut u);
    let at = focus(&mut u);
    let history = u.app.session.undo_depth();
    for _ in 0..3 {
        u.frame(vec![
            Event::PointerMoved(at),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                source: egui::MouseWheelSource::Unknown,
                delta: egui::vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ]);
    }
    assert_eq!((phase(&u, a), phase(&u, b)), (Some(3.0), Some(7.125)));
    assert_eq!(u.app.session.undo_depth(), history, "an active wheel gesture waits for its boundary");
    u.advance_clock(u.ctx.input(|i| i.time) + 0.5);
    u.frame(vec![]);
    assert_eq!(u.app.session.undo_depth().0, history.0 + 1);
    u.app.run("edit.undo", json!({}));
    assert_eq!((phase(&u, a), phase(&u, b)), (Some(0.0), Some(4.125)));
}
