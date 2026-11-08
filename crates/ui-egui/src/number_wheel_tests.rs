//! Focused number-wheel editing, driven through real egui input.
use figcraft_engine::Session;
use serde_json::json;

#[test]
fn number_wheel_is_an_opt_in_saved_preference() {
    let mut session = Session::new();
    let old: figcraft_engine::Prefs = serde_json::from_value(json!({})).unwrap();
    assert!(!old.number_wheel, "existing saved preferences also default off");
    assert_eq!(serde_json::to_value(&session.prefs).unwrap()["number_wheel"], false);
    assert_eq!(session.execute("pref.numberWheel", &json!({"on":false})).unwrap()["on"], false);
    session.execute("pref.numberWheel", &json!({"on":true})).unwrap();
    assert_eq!(session.execute("pref.numberWheel", &json!({"on":true})).unwrap()["on"], true);
    let saved = serde_json::to_value(&session.prefs).unwrap();
    let loaded: figcraft_engine::Prefs = serde_json::from_value(saved).unwrap();
    assert!(serde_json::to_value(loaded).unwrap()["number_wheel"].as_bool().unwrap());
}

use crate::layers_drag_tests::Ui;
use egui::{Event, Modifiers, MouseWheelSource, MouseWheelUnit, Pos2, TouchPhase, pos2, vec2};
use figcraft_doc::NodeId;

fn setup(enabled: bool) -> (Ui, NodeId, NodeId) {
    let mut s = Session::new();
    s.prefs.number_wheel = enabled;
    let a = NodeId(s.execute("shape.rectangle", &json!({"x":10,"width":40,"height":30})).unwrap()["id"].as_u64().unwrap());
    let b = NodeId(s.execute("shape.rectangle", &json!({"x":100,"width":20,"height":30})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids":[a]})).unwrap();
    let mut u = Ui::new(s);
    u.ctx.enable_accesskit();
    u.frame(vec![]);
    (u, a, b)
}

fn field(u: &mut Ui, name: &str) -> Pos2 {
    let out = u.frame_output(vec![]);
    let nodes = out.platform_output.accesskit_update.unwrap().nodes;
    let n = nodes
        .iter()
        .map(|(_, n)| n)
        .find(|n| n.role() == egui::accesskit::Role::SpinButton && n.label() == Some(name))
        .unwrap_or_else(|| panic!("no field {name}"));
    let r = n.bounds().unwrap();
    pos2(((r.x0 + r.x1) * 0.5) as f32, ((r.y0 + r.y1) * 0.5) as f32)
}
fn focus(u: &mut Ui, name: &str) -> Pos2 {
    let p = field(u, name);
    u.button(p, true);
    u.button(p, false);
    u.frame(vec![]);
    p
}
fn event(unit: MouseWheelUnit, delta: f32, m: Modifiers) -> Event {
    Event::MouseWheel { unit, source: MouseWheelSource::Unknown, delta: vec2(0.0, delta), phase: TouchPhase::Move, modifiers: m }
}
fn wheel(u: &mut Ui, p: Pos2, unit: MouseWheelUnit, delta: f32, m: Modifiers) {
    u.frame_with(vec![Event::PointerMoved(p), event(unit, delta, m)], m);
}
fn width(u: &Ui, id: NodeId) -> f64 {
    u.app.session.doc().unwrap().node(id).unwrap().width
}
fn idle(u: &mut Ui) {
    u.advance_clock(u.ctx.input(|i| i.time) + 0.5);
    u.frame(vec![]);
}

#[test]
fn wheel_requires_opt_in_focus_and_pointer_over_the_field() {
    let (mut u, a, _) = setup(false);
    let p = focus(&mut u, "Width");
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), 40.0, "default off leaves panel scrolling alone");
    u.app.run("pref.numberWheel", json!({"on":true}));
    // An actual click on the canvas releases every field, regardless of its widget ID.
    u.button(pos2(700.0, 700.0), true);
    u.button(pos2(700.0, 700.0), false);
    u.app.session.execute("select.set", &json!({"ids":[a]})).unwrap();
    let p = field(&mut u, "Width");
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), 40.0, "hover alone does not edit");
    let p = focus(&mut u, "Width");
    wheel(&mut u, pos2(700.0, 700.0), MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), 40.0, "a focused field does not capture canvas input");
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::COMMAND);
    assert_eq!(width(&u, a), 40.0, "command wheel is reserved for navigation");
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), 41.0);
}

#[test]
fn wheel_uses_nudges_modifiers_units_and_the_typed_arithmetic() {
    let (mut u, a, _) = setup(true);
    u.app.run("pref.nudge", json!({"small":2,"big":25}));
    let p = focus(&mut u, "Width");
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), 42.0);
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::SHIFT);
    assert_eq!(width(&u, a), 67.0);
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::ALT);
    assert_eq!(width(&u, a), 67.2);
    wheel(&mut u, p, MouseWheelUnit::Line, -1.0, Modifiers::ALT);
    assert_eq!(width(&u, a), 67.0, "logical negative delta decrements without floating point dust");
    u.frame(vec![Event::Text("10px * 2".into())]);
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), 22.0, "first notch steps the pending arithmetic and units");
    let p = focus(&mut u, "Opacity");
    wheel(&mut u, p, MouseWheelUnit::Line, -1.0, Modifiers::ALT);
    wheel(&mut u, p, MouseWheelUnit::Line, -1.0, Modifiers::ALT);
    assert!((u.app.session.doc().unwrap().node(a).unwrap().opacity - 0.998).abs() < 1e-6, "percent fields ignore pixel nudge and preserve tenths");
}

#[test]
fn trackpad_fractions_accumulate_without_changing_values_until_a_whole_step() {
    let (mut u, a, _) = setup(true);
    let p = focus(&mut u, "Width");
    let line = u.ctx.options(|o| o.input_options.line_scroll_speed);
    let before = u.app.session.undo_depth().0;
    for _ in 0..3 {
        wheel(&mut u, p, MouseWheelUnit::Point, line * 0.25, Modifiers::NONE);
        assert_eq!(width(&u, a), 40.0);
    }
    assert!(u.app.session.ds().unwrap().interaction.is_none(), "fractions alone do not start history");
    wheel(&mut u, p, MouseWheelUnit::Point, line * 0.25, Modifiers::NONE);
    assert_eq!(width(&u, a), 41.0);
    wheel(&mut u, p, MouseWheelUnit::Point, -line * 0.5, Modifiers::NONE);
    assert_eq!(width(&u, a), 41.0);
    wheel(&mut u, p, MouseWheelUnit::Point, -line * 0.5, Modifiers::NONE);
    assert_eq!(width(&u, a), 40.0);
    idle(&mut u);
    assert_eq!(u.app.session.undo_depth().0, before, "round trip has no undo entry");
    wheel(&mut u, p, MouseWheelUnit::Point, line * 0.75, Modifiers::NONE);
    idle(&mut u);
    wheel(&mut u, p, MouseWheelUnit::Point, line * 0.25, Modifiers::NONE);
    assert_eq!(width(&u, a), 40.0, "idle resets a fractional gesture");
}

#[test]
fn mixed_values_step_relatively_and_a_gesture_is_one_undo() {
    let (mut u, a, b) = setup(true);
    u.app.session.execute("select.set", &json!({"ids":[a,b]})).unwrap();
    let p = focus(&mut u, "Width");
    let before = u.app.session.undo_depth().0;
    for _ in 0..3 {
        wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    }
    assert_eq!((width(&u, a), width(&u, b)), (43.0, 23.0));
    assert_eq!(u.app.session.undo_depth().0, before, "gesture waits for its boundary");
    idle(&mut u);
    assert_eq!(u.app.session.undo_depth().0, before + 1);
    u.app.run("edit.undo", json!({}));
    assert_eq!((width(&u, a), width(&u, b)), (40.0, 20.0));
    u.app.run("edit.redo", json!({}));
    assert_eq!((width(&u, a), width(&u, b)), (43.0, 23.0));
}

#[test]
fn wheel_clamps_and_reverses_at_limits_without_replaying_a_stale_draft() {
    let (mut u, a, _) = setup(true);
    let p = focus(&mut u, "Opacity");
    let before = u.app.session.undo_depth().0;
    wheel(&mut u, p, MouseWheelUnit::Line, 50.0, Modifiers::NONE);
    assert_eq!(u.app.session.doc().unwrap().node(a).unwrap().opacity, 1.0);
    wheel(&mut u, p, MouseWheelUnit::Line, -1.0, Modifiers::NONE);
    assert_eq!(u.app.session.doc().unwrap().node(a).unwrap().opacity, 0.99);
    u.frame(vec![Event::PointerMoved(pos2(700.0, 700.0))]);
    assert_eq!(u.app.session.undo_depth().0, before + 1, "leaving ends the gesture");
    let p = focus(&mut u, "Width");
    wheel(&mut u, p, MouseWheelUnit::Line, -100.0, Modifiers::NONE);
    let minimum = width(&u, a);
    assert_eq!(minimum, 0.0, "dimension fields retain their existing nonnegative clamp");
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), minimum + 1.0);
}

#[test]
fn modifiers_and_discarded_passes_have_precise_undo_boundaries() {
    let (mut u, a, _) = setup(true);
    let p = focus(&mut u, "Width");
    let before = u.app.session.undo_depth().0;
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::SHIFT);
    assert_eq!(width(&u, a), 51.0);
    assert_eq!(u.app.session.undo_depth().0, before + 1, "modifier change commits the previous rate");
    idle(&mut u);
    assert_eq!(u.app.session.undo_depth().0, before + 2);
    u.frame_raw_with_discard(egui::RawInput {
        events: vec![Event::ModifiersChanged(Modifiers::NONE), Event::PointerMoved(p), event(MouseWheelUnit::Line, 1.0, Modifiers::NONE)],
        ..Default::default()
    });
    assert_eq!(width(&u, a), 52.0, "rerunning an egui pass does not apply a notch twice");
    idle(&mut u);
    assert_eq!(u.app.session.undo_depth().0, before + 3);
}

#[test]
fn changing_the_owner_commits_only_its_gesture_and_discards_its_cached_editor() {
    for document in [false, true] {
        let (mut u, a, b) = setup(true);
        let p = focus(&mut u, "Width");
        let before = u.app.session.undo_depth().0;
        let editor = u.ctx.memory(|m| m.focused()).unwrap();
        wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
        let owner = u.app.session.ds().unwrap().uid;
        if document {
            u.app.session.execute("file.new", &json!({"name":"Other"})).unwrap();
            u.app.session.execute("shape.rectangle", &json!({"width":200,"height":30})).unwrap();
        } else {
            u.app.session.execute("select.set", &json!({"ids":[b]})).unwrap();
        }
        let now = u.app.session.doc().unwrap().clone();
        let history = u.app.session.undo_depth().0;
        u.frame(vec![]);
        assert_eq!(*u.app.session.doc().unwrap(), now, "old editor cannot write into replacement selection/document");
        assert!(!u.ctx.memory(|m| m.has_focus(editor)), "replacement owner cannot keep the previous editor focused");
        if document {
            assert_eq!(u.app.session.undo_depth().0, history);
            let owned = u.app.session.docs.iter().find(|d| d.uid == owner).unwrap();
            assert_eq!(owned.history.undo.len(), before + 1);
            assert_eq!(owned.doc.node(a).unwrap().width, 41.0);
            assert!(owned.interaction.is_none());
        } else {
            assert_eq!(width(&u, a), 41.0);
            assert_eq!(width(&u, b), 20.0);
            assert_eq!(u.app.session.undo_depth().0, before + 1);
        }
    }
}

#[test]
fn unrelated_commands_and_disabling_the_preference_close_the_wheel_first() {
    let (mut u, a, _) = setup(true);
    let p = focus(&mut u, "Width");
    let before = u.app.session.undo_depth().0;
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    u.app.run("node.set", json!({"id":a,"props":{"height":80}}));
    assert_eq!(u.app.session.undo_depth().0, before + 2, "a separate command has its own undo");
    u.app.run("edit.undo", json!({}));
    assert_eq!(width(&u, a), 41.0);
    assert_eq!(u.app.session.doc().unwrap().node(a).unwrap().height, 30.0);
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    u.app.run("pref.numberWheel", json!({"on":false}));
    assert!(u.app.session.ds().unwrap().interaction.is_none());
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), 42.0);
}

#[test]
fn native_window_focus_loss_ends_the_gesture_immediately() {
    let (mut u, a, _) = setup(true);
    let p = focus(&mut u, "Width");
    let before = u.app.session.undo_depth().0;
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert!(u.app.session.ds().unwrap().interaction.is_some());
    u.frame_raw(egui::RawInput { focused: false, ..Default::default() });
    assert_eq!(width(&u, a), 41.0);
    assert!(u.app.session.ds().unwrap().interaction.is_none());
    assert_eq!(u.app.session.undo_depth().0, before + 1);
}

#[test]
fn a_second_command_in_the_same_pass_cannot_join_a_number_wheel_edit() {
    let (mut u, a, _) = setup(true);
    let ctx = egui::Context::default();
    u.app.ui_context = Some(ctx.clone());
    let id = egui::Id::unique("test-wheel-command-permit");
    let draw = |app: &mut crate::FigApp, events, time| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(400.0, 200.0))),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                crate::number_wheel::sync(app, &ctx);
                ctx.memory_mut(|m| m.request_focus(id.with("field")));
                let value = app.session.doc().unwrap().node(a).unwrap().width;
                let edit = crate::widgets::number_field_mixed(ui, id, crate::widgets::Lead::None, Some(value), "", 100.0, 1.0);
                let changed = edit.is_some();
                app.field_of(&[a], "width", edit, |n| n.width, |v| json!(v));
                if changed {
                    app.run("node.set", json!({"id":a,"props":{"height":80}}));
                }
                crate::number_wheel::end_frame(&ctx);
            },
        )
        .textures_delta
        .clear();
    };
    draw(&mut u.app, vec![], 1.0);
    let p = ctx.read_response(id.with("field")).unwrap().rect.center();
    let before = u.app.session.undo_depth().0;
    let mut diagonal = event(MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    if let Event::MouseWheel { delta, .. } = &mut diagonal {
        delta.x = 2.0;
    }
    draw(&mut u.app, vec![Event::PointerMoved(p), diagonal], 1.1);
    assert!(
        ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::MouseWheel { delta, .. } if delta.x == 2.0 && delta.y == 0.0))),
        "horizontal input remains available"
    );
    assert_eq!(width(&u, a), 41.0);
    assert_eq!(u.app.session.undo_depth().0, before + 2, "each actual command owns its undo entry");
    assert!(u.app.session.ds().unwrap().interaction.is_none());
    u.app.run("edit.undo", json!({}));
    assert_eq!(width(&u, a), 41.0);
    assert_eq!(u.app.session.doc().unwrap().node(a).unwrap().height, 30.0);
}

#[test]
fn disabled_fields_and_bound_variable_chips_do_not_capture_wheel_events() {
    for bound in [false, true] {
        let (mut u, a, _) = setup(true);
        let ctx = egui::Context::default();
        u.app.ui_context = Some(ctx.clone());
        let id = egui::Id::unique("disabled-or-bound-wheel");
        let draw = |app: &mut crate::FigApp, events, time, guard: bool| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(400.0, 200.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    crate::number_wheel::sync(app, &ctx);
                    ctx.memory_mut(|m| m.request_focus(id.with("field")));
                    let mut edit = None;
                    ui.add_enabled_ui(!guard || bound, |ui| {
                        edit = crate::widgets::number_field_bindable(
                            ui,
                            id,
                            crate::widgets::Lead::None,
                            Some(40.0),
                            "",
                            100.0,
                            1.0,
                            (guard && bound).then_some(Some("Bound width")),
                            crate::widgets::Empty::Text("Mixed"),
                        )
                        .0;
                    });
                    assert!(edit.is_none());
                    crate::number_wheel::end_frame(&ctx);
                },
            )
            .textures_delta
            .clear();
        };
        draw(&mut u.app, vec![], 1.0, false);
        let p = ctx.read_response(id.with("field")).unwrap().rect.center();
        draw(&mut u.app, vec![Event::PointerMoved(p), event(MouseWheelUnit::Line, 1.0, Modifiers::NONE)], 1.1, true);
        assert_eq!(width(&u, a), 40.0);
        assert!(
            ctx.input(|i| i.events.iter().any(|e| matches!(e,Event::MouseWheel{delta,..} if delta.y==1.0))),
            "unhandled input remains available: bound={bound}"
        );
    }
}

#[test]
fn a_wheel_never_commits_an_interaction_owned_by_another_editor() {
    let (mut u, a, _) = setup(true);
    let p = focus(&mut u, "Width");
    u.app.session.begin_interaction("Other editor").unwrap();
    let snapshot = u.app.session.ds().unwrap().interaction.as_ref().unwrap().doc.clone();
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), 41.0);
    idle(&mut u);
    let i = u.app.session.ds().unwrap().interaction.as_ref().unwrap();
    assert_eq!(i.label, "Other editor");
    assert!(std::sync::Arc::ptr_eq(&i.doc, &snapshot));
}

#[test]
fn opt_in_does_not_change_canvas_wheel_pan_or_zoom() {
    for (unit, mods) in [
        (MouseWheelUnit::Line, Modifiers::NONE),
        (MouseWheelUnit::Point, Modifiers::NONE),
        (MouseWheelUnit::Line, Modifiers::COMMAND),
        (MouseWheelUnit::Line, Modifiers::SHIFT),
    ] {
        let (mut off, _, _) = setup(false);
        let (mut on, _, _) = setup(true);
        focus(&mut off, "Width");
        focus(&mut on, "Width");
        let before = off.app.session.ds().unwrap().view.clone();
        for u in [&mut off, &mut on] {
            wheel(u, pos2(700.0, 700.0), unit, 12.0, mods);
        }
        assert_eq!(off.app.session.ds().unwrap().view, on.app.session.ds().unwrap().view, "canvas path with {unit:?}/{mods:?}");
        assert_ne!(off.app.session.ds().unwrap().view, before, "the test actually navigated the canvas");
    }
}

#[test]
fn an_untouched_rounded_display_keeps_its_fraction_and_typing_takes_priority() {
    let (mut u, a, _) = setup(true);
    u.app.session.execute("node.set", &json!({"id":a,"props":{"opacity":0.091234}})).unwrap();
    let p = focus(&mut u, "Opacity");
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert!(
        (f64::from(u.app.session.doc().unwrap().node(a).unwrap().opacity) - 0.101234).abs() < 1e-6,
        "the display's two decimals are not written back"
    );
    u.frame_with(vec![Event::Text("10% * 2".into()), Event::PointerMoved(p), event(MouseWheelUnit::Line, 1.0, Modifiers::NONE)], Modifiers::NONE);
    assert!(
        (f64::from(u.app.session.doc().unwrap().node(a).unwrap().opacity) - 0.101234).abs() < 1e-6,
        "wheel does not overwrite text delivered in the same batch"
    );
    u.frame(vec![Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    assert_eq!(u.app.session.doc().unwrap().node(a).unwrap().opacity, 0.2);
}

#[test]
fn a_same_pass_control_route_cannot_reuse_the_wheel_owners_cached_editor() {
    let (mut u, a, _) = setup(true);
    let first = u.app.session.active;
    u.app.session.execute("file.new", &json!({"name":"Wheel replacement"})).unwrap();
    let second = u.app.session.active;
    let b = NodeId(u.app.session.execute("shape.rectangle", &json!({"width":200,"height":30})).unwrap()["id"].as_u64().unwrap());
    u.app.session.execute("doc.switch", &json!({"index":first})).unwrap();
    let p = focus(&mut u, "Width");
    let editor = u.ctx.memory(|m| m.focused()).unwrap();
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), 41.0);
    let (reply, rx) = std::sync::mpsc::channel();
    u.app.control_queue.push(crate::ControlRequest {
        id: json!("wheel-switch"),
        method: "engine.execute".into(),
        params: json!({"command":"doc.switch","params":{"index":second},"window":0}),
        reply,
        state: Default::default(),
    });
    u.frame(vec![]);
    assert!(rx.try_recv().unwrap().result.is_ok());
    assert_eq!(u.app.session.active, second);
    assert_eq!(width(&u, b), 200.0);
    assert!(!u.ctx.memory(|m| m.has_focus(editor)), "the second sync still knows who owned the cached field");
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, b), 200.0, "the old keyboard focus cannot step the new file");
    assert!(u.app.session.docs[first].interaction.is_none());
}

#[test]
fn mixed_percentages_clamp_each_layer_and_reverse_from_its_accepted_value() {
    let (mut u, a, b) = setup(true);
    u.app.session.execute("node.set", &json!({"id":a,"props":{"opacity":0.99}})).unwrap();
    u.app.session.execute("node.set", &json!({"id":b,"props":{"opacity":0.5}})).unwrap();
    u.app.session.execute("select.set", &json!({"ids":[a,b]})).unwrap();
    let p = focus(&mut u, "Opacity");
    let before = u.app.session.undo_depth().0;
    wheel(&mut u, p, MouseWheelUnit::Line, 2.0, Modifiers::NONE);
    let op = |u: &Ui, id| u.app.session.doc().unwrap().node(id).unwrap().opacity;
    assert_eq!((op(&u, a), op(&u, b)), (1.0, 0.52));
    wheel(&mut u, p, MouseWheelUnit::Line, -1.0, Modifiers::NONE);
    assert_eq!((op(&u, a), op(&u, b)), (0.99, 0.51));
    idle(&mut u);
    assert_eq!(u.app.session.undo_depth().0, before + 1);
    u.app.run("edit.undo", json!({}));
    assert_eq!((op(&u, a), op(&u, b)), (0.99, 0.5));
}

#[test]
fn idle_deadline_waits_for_the_last_fraction_or_notch() {
    let (mut u, a, _) = setup(true);
    let p = focus(&mut u, "Width");
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    let last = u.ctx.input(|i| i.time);
    u.advance_clock(last + 0.39 - 1.0 / 60.0);
    let out = u.frame_output(vec![]);
    assert!(u.app.session.ds().unwrap().interaction.is_some(), "an early callback does not end it");
    let delay = out.viewport_output.get(&egui::ViewportId::ROOT).unwrap().repaint_delay;
    assert!(delay <= std::time::Duration::from_millis(20), "rearm the last 10ms, got {delay:?}");
    u.advance_clock(last + 0.41 - 1.0 / 60.0);
    u.frame(vec![]);
    assert!(u.app.session.ds().unwrap().interaction.is_none());
    assert_eq!(width(&u, a), 41.0);
}

#[test]
fn queued_read_only_inspection_does_not_split_a_human_wheel_gesture() {
    let (mut u, a, _) = setup(true);
    let p = focus(&mut u, "Width");
    let before = u.app.session.undo_depth().0;
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    let (reply, rx) = std::sync::mpsc::channel();
    u.app.control_queue.push(crate::ControlRequest {
        id: json!("wheel-inspect"),
        method: "ui.inspect".into(),
        params: json!({"window":0}),
        reply,
        state: Default::default(),
    });
    u.frame(vec![]);
    assert!(rx.try_recv().unwrap().result.is_ok());
    assert!(u.app.session.ds().unwrap().interaction.is_some());
    wheel(&mut u, p, MouseWheelUnit::Line, 1.0, Modifiers::NONE);
    assert_eq!(width(&u, a), 42.0);
    idle(&mut u);
    assert_eq!(u.app.session.undo_depth().0, before + 1);
}
