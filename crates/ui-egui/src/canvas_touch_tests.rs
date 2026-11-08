//! A finger on the canvas, driven through egui headless the way Android reports it: a drag on the
//! canvas background pans the view (a touch screen has no scroll wheel and the Move tool's drag
//! there selected nothing), while a press on a layer stays the tool's, a tap still clicks and a
//! still hold is egui's long-press menu. A mouse press is never arbitrated, so the desktop keeps
//! every drag it has.

use egui::{Event, Modifiers, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase, vec2};
use figcraft_engine::Session;
use serde_json::json;

use crate::layers_drag_tests::Ui;

fn touch(phase: TouchPhase, p: Pos2) -> Event {
    Event::Touch { device_id: TouchDeviceId(1), id: TouchId(1), phase, pos: p, force: Some(0.5) }
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

/// A finger goes down at `p` (the touch plus the pointer events egui-winit sends with it).
fn touch_down(u: &mut Ui, p: Pos2) {
    u.frame(vec![touch(TouchPhase::Start, p), Event::PointerMoved(p), button(p, true)]);
}

/// The finger moves to `p` while held.
fn touch_move(u: &mut Ui, p: Pos2) {
    u.frame(vec![touch(TouchPhase::Move, p), Event::PointerMoved(p)]);
}

/// The finger lifts at `p`.
fn touch_up(u: &mut Ui, p: Pos2) {
    u.frame(vec![touch(TouchPhase::End, p), button(p, false), Event::PointerGone]);
}

/// A point on the canvas background: far from `around`, inside the canvas.
fn background(u: &Ui, around: Pos2) -> Pos2 {
    let rect = u.app.canvas.rect;
    let mut p = Pos2::new(rect.center().x, rect.center().y);
    if p.distance(around) < 150.0 {
        p += vec2(0.0, 200.0);
    }
    assert!(rect.contains(p), "the point is on the canvas: {p:?} in {rect:?}");
    p
}

/// A document with one rectangle, selected, drawn on the canvas.
fn selected_rect() -> (Ui, Pos2) {
    let mut s = Session::new();
    let r = s.execute("shape.rectangle", &json!({"x": 100, "y": 100, "width": 200, "height": 120})).unwrap();
    let r = figcraft_engine::params::id_of(&r["id"]).unwrap();
    s.execute("select.set", &json!({"ids": [r]})).unwrap();
    let mut u = Ui::new(s);
    u.frame(vec![]);
    let c = u.app.session.doc().unwrap().world_bounds(r).unwrap().center();
    let at = u.app.to_screen(c.x, c.y);
    (u, at)
}

#[test]
fn a_finger_drag_on_the_canvas_background_pans_without_touching_the_selection() {
    let (mut u, on_rect) = selected_rect();
    let start = background(&u, on_rect);
    let cx = u.app.session.ds().unwrap().view.cx;
    let sel = u.app.session.selection();
    touch_down(&mut u, start);
    for k in 1..=6 {
        touch_move(&mut u, start + vec2(25.0 * k as f32, 0.0));
    }
    let zoom = u.app.session.ds().unwrap().view.zoom;
    let moved = u.app.session.ds().unwrap().view.cx;
    assert!((moved - cx + 150.0 / zoom).abs() < 1.0, "the view panned under the finger: {cx} -> {moved}");
    assert_eq!(u.app.session.selection(), sel, "panning never clears or changes the selection");
    assert!(u.app.session.tool.overlay.marquee.is_none(), "no marquee is drawn");
    touch_up(&mut u, start + vec2(150.0, 0.0));
    assert_eq!(u.app.session.selection(), sel, "the lift ends the pan, nothing else");
}

#[test]
fn a_finger_drag_on_a_layer_still_moves_it_and_never_pans() {
    let (mut u, on_rect) = selected_rect();
    let cx = u.app.session.ds().unwrap().view.cx;
    let before = figcraft_geom::translation(u.app.session.doc().unwrap().world_transform(u.app.session.selection()[0]));
    touch_down(&mut u, on_rect);
    for k in 1..=6 {
        touch_move(&mut u, on_rect + vec2(10.0 * k as f32, 0.0));
    }
    touch_up(&mut u, on_rect + vec2(60.0, 0.0));
    let after = figcraft_geom::translation(u.app.session.doc().unwrap().world_transform(u.app.session.selection()[0]));
    assert!((after.x - before.x) > 30.0, "the layer moved with the finger: {:?} -> {:?}", before, after);
    assert_eq!(u.app.session.ds().unwrap().view.cx, cx, "the view never panned");
}

#[test]
fn a_finger_tap_on_the_canvas_background_still_deselects() {
    let (mut u, on_rect) = selected_rect();
    let p = background(&u, on_rect);
    touch_down(&mut u, p);
    touch_up(&mut u, p);
    u.frame(vec![]);
    assert!(u.app.session.selection().is_empty(), "a tap on empty canvas deselects");
}

#[test]
fn a_still_finger_hold_opens_the_menu_and_keeps_the_selection() {
    let (mut u, on_rect) = selected_rect();
    let p = background(&u, on_rect);
    let sel = u.app.session.selection();
    touch_down(&mut u, p);
    // Past egui's long-press window (0.8 s at the harness's 60 fps).
    for _ in 0..60 {
        u.frame(vec![]);
    }
    assert!(u.app.ui.context_menu.is_some(), "a still hold opens the canvas menu");
    assert_eq!(u.app.session.selection(), sel, "the hold never clears the selection first");
    touch_up(&mut u, p);
}

#[test]
fn a_mouse_drag_on_the_canvas_background_still_marquees() {
    let (mut u, on_rect) = selected_rect();
    u.app.session.execute("select.none", &json!({})).unwrap();
    let start = background(&u, on_rect);
    let cx = u.app.session.ds().unwrap().view.cx;
    u.frame(vec![Event::PointerMoved(start)]);
    u.frame(vec![button(start, true)]);
    for k in 1..=8 {
        u.frame(vec![Event::PointerMoved(start + vec2(20.0 * k as f32, 0.0))]);
    }
    // Across the rectangle, so the marquee reaches it.
    let over = on_rect;
    u.frame(vec![Event::PointerMoved(over)]);
    u.frame(vec![button(over, false)]);
    assert!(!u.app.session.selection().is_empty(), "the mouse marquee still selects");
    assert_eq!(u.app.session.ds().unwrap().view.cx, cx, "a mouse drag never pans");
}

#[test]
fn a_withheld_canvas_press_cannot_move_or_click_a_different_document_page_or_tool() {
    for changed in ["document", "page", "tool"] {
        let (mut u, on_rect) = selected_rect();
        let start = background(&u, on_rect);
        touch_down(&mut u, start);
        match changed {
            "document" => {
                u.app.session.execute("file.new", &json!({})).unwrap();
            }
            "page" => {
                u.app.session.execute("page.add", &json!({"name":"Other page"})).unwrap();
            }
            _ => {
                u.app.session.tool.tool = figcraft_engine::ToolId::Rectangle;
            }
        }
        // A new tab/page establishes its viewport from the canvas size on its first frame.
        // That same frame must cancel the withheld press before any subsequent movement.
        u.frame(vec![]);
        let before = u.app.session.ds().unwrap().clone();
        for k in 1..=5 {
            touch_move(&mut u, start + vec2(15.0 * k as f32, 0.0));
        }
        touch_up(&mut u, start + vec2(75.0, 0.0));
        let after = u.app.session.ds().unwrap();
        assert_eq!(after.view.cx, before.view.cx, "{changed}: no pan leaks into the new context");
        assert_eq!(after.selection, before.selection, "{changed}: no delayed click changes selection");
        assert!(std::sync::Arc::ptr_eq(&after.doc, &before.doc), "{changed}: no shape is created by a withheld press");
    }
}

/// Two distinct finger contacts, including PointerGone between contacts, travel
/// through the real touch arbiter; mouse-only double-click tests cannot stand in.
fn two_taps(u: &mut Ui, at: Pos2, gap_frames: usize) {
    touch_down(u, at);
    touch_up(u, at);
    for _ in 0..gap_frames {
        u.frame(vec![]);
    }
    touch_down(u, at);
    touch_up(u, at);
    u.frame(vec![]);
}

#[test]
fn two_finger_taps_on_a_shape_enter_the_same_vector_edit_as_a_double_click() {
    for selected in [false, true] {
        // All gaps fit the app's 0.5s double-click interval at the harness's 60fps.
        for gap in [0, 8, 20] {
            let (mut u, at) = selected_rect();
            let shape = u.app.session.selection()[0];
            if !selected {
                u.app.session.execute("select.none", &json!({})).unwrap();
                u.frame(vec![]);
            }
            let view = (u.app.session.ds().unwrap().view.cx, u.app.session.ds().unwrap().view.cy);
            two_taps(&mut u, at, gap);
            assert_eq!(
                u.app.session.ds().unwrap().vector_edit.as_ref().map(|edit| edit.id),
                Some(shape),
                "selected={selected}, gap={gap}: two real touch contacts edit the hit shape"
            );
            assert_eq!((u.app.session.ds().unwrap().view.cx, u.app.session.ds().unwrap().view.cy), view, "a stationary two-tap never pans");
            assert!(u.app.ui.context_menu.is_none(), "a two-tap is not a long-press menu");
            assert_eq!(u.app.inspect()["tool"]["busy"], false, "released contacts leave no withheld/drag gesture");
        }
    }
}

#[test]
fn two_taps_after_the_double_click_interval_stay_single_clicks() {
    for selected in [false, true] {
        let (mut u, at) = selected_rect();
        let shape = u.app.session.selection()[0];
        if !selected {
            u.app.session.execute("select.none", &json!({})).unwrap();
            u.frame(vec![]);
        }
        let before = u.app.session.ds().unwrap().doc.clone();
        let history = u.app.session.undo_depth();
        let view = (u.app.session.ds().unwrap().view.cx, u.app.session.ds().unwrap().view.cy);
        //40 idle frames plus the release/next press are longer than the app's 0.5s interval.
        two_taps(&mut u, at, 40);
        let after = u.app.session.ds().unwrap();
        assert!(after.vector_edit.is_none() && after.text_edit.is_none(), "slow contacts are two ordinary clicks");
        assert_eq!(after.selection, vec![shape]);
        assert_eq!(before, after.doc, "stationary contacts do not change authored geometry");
        assert_eq!(u.app.session.undo_depth(), history, "single clicks are not replayed as edits");
        assert_eq!((after.view.cx, after.view.cy), view);
        assert!(u.app.ui.context_menu.is_none());
        assert_eq!(u.app.inspect()["tool"]["busy"], false);
    }
}

#[test]
fn two_finger_taps_on_text_open_the_actual_inline_text_edit() {
    for selected in [false, true] {
        for gap in [0, 8, 20] {
            let mut s = Session::new();
            let id = s.execute("text.create", &json!({"text":"Touch editable text","x":100,"y":100,"props":{"fontSize":28}})).unwrap();
            let id = figcraft_engine::params::id_of(&id["id"]).unwrap();
            if !selected {
                s.execute("select.none", &json!({})).unwrap();
            }
            let mut u = Ui::new(s);
            let bounds = u.app.session.doc().unwrap().world_bounds(id).unwrap();
            let at = u.app.to_screen(bounds.center().x, bounds.center().y);
            let view = (u.app.session.ds().unwrap().view.cx, u.app.session.ds().unwrap().view.cy);
            two_taps(&mut u, at, gap);
            assert_eq!(
                u.app.session.ds().unwrap().text_edit.as_ref().map(|edit| edit.id),
                Some(id),
                "selected={selected}, gap={gap}: two actual finger contacts enter text editing"
            );
            assert!(u.app.ui.context_menu.is_none());
            assert_eq!((u.app.session.ds().unwrap().view.cx, u.app.session.ds().unwrap().view.cy), view);
            assert_eq!(u.app.inspect()["tool"]["busy"], false);
        }
    }
}

#[test]
fn two_finger_taps_on_empty_canvas_leave_vector_edit_without_panning_or_creating_a_layer() {
    for gap in [0, 8, 20] {
        let (mut u, on_rect) = selected_rect();
        u.frame(vec![Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
        u.frame(vec![Event::Key { key: egui::Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
        assert!(u.app.session.ds().unwrap().vector_edit.is_some(), "the known keyboard path establishes vector edit");
        let at = background(&u, on_rect);
        let before = u.app.session.doc().unwrap().nodes().count();
        let view = (u.app.session.ds().unwrap().view.cx, u.app.session.ds().unwrap().view.cy);
        two_taps(&mut u, at, gap);
        assert!(u.app.session.ds().unwrap().vector_edit.is_none(), "gap={gap}: a two-tap on empty canvas exits vector edit like mouse double-click");
        assert_eq!(u.app.session.doc().unwrap().nodes().count(), before, "no accidental shape is committed");
        assert_eq!((u.app.session.ds().unwrap().view.cx, u.app.session.ds().unwrap().view.cy), view);
        assert_eq!(u.app.inspect()["tool"]["busy"], false);
    }
}

#[test]
fn mouse_double_click_on_empty_canvas_also_releases_its_vector_gesture() {
    let (mut u, on_rect) = selected_rect();
    u.frame(vec![Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    u.frame(vec![Event::Key { key: egui::Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert!(u.app.session.ds().unwrap().vector_edit.is_some());
    let at = background(&u, on_rect);
    let before = u.app.session.ds().unwrap().doc.clone();
    let view = (u.app.session.ds().unwrap().view.cx, u.app.session.ds().unwrap().view.cy);
    for _ in 0..2 {
        u.frame(vec![Event::PointerMoved(at)]);
        u.frame(vec![button(at, true)]);
        u.frame(vec![button(at, false)]);
    }
    u.frame(vec![]);
    let after = u.app.session.ds().unwrap();
    assert!(after.vector_edit.is_none());
    assert!(after.interaction.is_none());
    assert_eq!(after.doc, before, "leaving edit mode does not change authored geometry");
    assert_eq!((after.view.cx, after.view.cy), view);
    assert!(u.app.ui.context_menu.is_none());
    assert_eq!(u.app.inspect()["tool"]["busy"], false, "the second press ends on its own release");
}

// Two actual touch IDs plus the extra primary-pointer events emitted by eframe web.
fn web_finger(phase: TouchPhase, id: u64, p: Pos2) -> Event {
    Event::Touch { device_id: TouchDeviceId(1), id: TouchId(id), phase, pos: p, force: Some(0.5) }
}

fn web_finger_down(u: &mut Ui, id: u64, p: Pos2) {
    // Pinned eframe web emits Down at the retained primary position before Touch Start.
    let primary = u.ctx.input(|i| if i.any_touches() { i.pointer.interact_pos().unwrap_or(p) } else { p });
    u.frame(vec![button(primary, true), web_finger(TouchPhase::Start, id, p)]);
}

fn web_finger_up(u: &mut Ui, id: u64, p: Pos2) {
    let primary = u.ctx.input(|i| i.pointer.interact_pos().unwrap_or(p));
    u.frame(vec![button(primary, false), Event::PointerGone, web_finger(TouchPhase::End, id, p)]);
}

fn two_finger_release(u: &mut Ui, a: Pos2, b: Pos2, primary_first: bool) {
    let (first, first_pos, remaining, remaining_pos) = if primary_first { (1, a, 2, b) } else { (2, b, 1, a) };
    web_finger_up(u, first, first_pos);
    // The remaining contact must still belong to navigation even though web emitted Up/Gone.
    u.frame(vec![web_finger(TouchPhase::Move, remaining, remaining_pos), Event::PointerMoved(remaining_pos)]);
    web_finger_up(u, remaining, remaining_pos);
    u.frame(vec![]);
}

fn assert_two_finger_preserved(u: &Ui, before: &std::sync::Arc<figcraft_doc::Document>, selection: &[figcraft_doc::NodeId], undo: usize) {
    let after = u.app.session.ds().unwrap();
    assert_eq!(&after.doc, before, "navigation does not change authored content");
    assert_eq!(u.app.session.selection(), selection, "neither contact replays a withheld click");
    assert_eq!(u.app.session.undo_depth().0, undo, "navigation creates no Undo entry");
    assert!(!u.app.session.tool.busy(), "the last real release leaves no tool gesture");
    assert!(u.app.ui.context_menu.is_none(), "multi-contact release opens no context menu");
    assert!(u.app.session.tool.overlay.marquee.is_none(), "multi-contact release creates no marquee");
}

#[test]
fn stationary_second_finger_with_repeated_primary_down_preserves_selection_in_both_release_orders() {
    for primary_first in [false, true] {
        let (mut u, on_rect) = selected_rect();
        let a = background(&u, on_rect) - vec2(50.0, 0.0);
        let b = a + vec2(100.0, 0.0);
        let before = u.app.session.ds().unwrap().doc.clone();
        let selection = u.app.session.selection().to_vec();
        let undo = u.app.session.undo_depth().0;
        let view = u.app.session.ds().unwrap().view.clone();
        web_finger_down(&mut u, 1, a);
        web_finger_down(&mut u, 2, b);
        assert_eq!(u.app.session.selection(), selection, "second web PrimaryDown is not a tool click");
        two_finger_release(&mut u, a, b, primary_first);
        assert_two_finger_preserved(&u, &before, &selection, undo);
        let after = &u.app.session.ds().unwrap().view;
        assert_eq!((after.cx, after.cy, after.zoom), (view.cx, view.cy, view.zoom));
        // A separate later ordinary tap still works after the navigation latch is released.
        for _ in 0..40 {
            u.frame(vec![]);
        }
        touch_down(&mut u, a);
        touch_up(&mut u, a);
        assert!(u.app.session.selection().is_empty(), "a fresh ordinary touch still delivers its click");
        assert!(!u.app.session.tool.busy());
    }
}

#[test]
fn concurrent_two_finger_pan_keeps_camera_navigation_and_never_replays_pointer_clicks() {
    for primary_first in [false, true] {
        let (mut u, on_rect) = selected_rect();
        let a = background(&u, on_rect) - vec2(50.0, 0.0);
        let b = a + vec2(100.0, 0.0);
        let before = u.app.session.ds().unwrap().doc.clone();
        let selection = u.app.session.selection().to_vec();
        let undo = u.app.session.undo_depth().0;
        let view = u.app.session.ds().unwrap().view.clone();
        web_finger_down(&mut u, 1, a);
        web_finger_down(&mut u, 2, b);
        for step in 1..=4 {
            let delta = vec2(step as f32 * 15.0, step as f32 * 5.0);
            u.frame(vec![web_finger(TouchPhase::Move, 1, a + delta), web_finger(TouchPhase::Move, 2, b + delta), Event::PointerMoved(b + delta)]);
        }
        let delta = vec2(60.0, 20.0);
        two_finger_release(&mut u, a + delta, b + delta, primary_first);
        assert_two_finger_preserved(&u, &before, &selection, undo);
        let after = &u.app.session.ds().unwrap().view;
        assert!((after.cx - view.cx).hypot(after.cy - view.cy) > 20.0, "both touches still pan the camera");
        assert!((after.zoom - view.zoom).abs() < 1e-6, "parallel translation does not become pinch");
    }
}

#[test]
fn concurrent_two_finger_pinch_keeps_zoom_in_both_release_orders() {
    for primary_first in [false, true] {
        let (mut u, on_rect) = selected_rect();
        let center = background(&u, on_rect);
        let a = center - vec2(50.0, 0.0);
        let b = center + vec2(50.0, 0.0);
        let before = u.app.session.ds().unwrap().doc.clone();
        let selection = u.app.session.selection().to_vec();
        let undo = u.app.session.undo_depth().0;
        let zoom = u.app.session.ds().unwrap().view.zoom;
        web_finger_down(&mut u, 1, a);
        web_finger_down(&mut u, 2, b);
        for step in 1..=4 {
            let delta = vec2(step as f32 * 10.0, 0.0);
            u.frame(vec![web_finger(TouchPhase::Move, 1, a - delta), web_finger(TouchPhase::Move, 2, b + delta), Event::PointerMoved(b + delta)]);
        }
        two_finger_release(&mut u, a - vec2(40.0, 0.0), b + vec2(40.0, 0.0), primary_first);
        assert_two_finger_preserved(&u, &before, &selection, undo);
        assert!(u.app.session.ds().unwrap().view.zoom > zoom * 1.3, "Navigation verdict must not swallow pinch zoom");
    }
}

#[test]
fn a_layer_owned_first_press_is_not_claimed_by_the_background_navigation_latch() {
    for primary_first in [false, true] {
        let (mut u, a) = selected_rect();
        let b = a + vec2(20.0, 0.0);
        let before = u.app.session.ds().unwrap().doc.clone();
        web_finger_down(&mut u, 1, a);
        assert!(u.app.session.tool.busy(), "the layer's ordinary Move tool owns its initial press");
        web_finger_down(&mut u, 2, b);
        two_finger_release(&mut u, a, b, primary_first);
        assert!(!u.app.session.tool.busy(), "unclaimed layer-owned input retains its normal release path");
        for _ in 0..40 {
            u.frame(vec![]);
        }
        let fresh = background(&u, a);
        touch_down(&mut u, fresh);
        touch_up(&mut u, fresh);
        assert!(u.app.session.selection().is_empty(), "a released layer gesture permits a fresh ordinary background tap");
        assert_eq!(u.app.session.ds().unwrap().doc, before, "stationary layer contacts preserve authored geometry");
    }
}

fn raw_web_move(u: &mut Ui, id: u64, p: Pos2) {
    u.frame(vec![Event::PointerMoved(p), web_finger(TouchPhase::Move, id, p)]);
}

#[derive(Clone, Copy, Debug)]
enum OwnerChange {
    Document,
    Page,
    Tool,
}

fn owner_round_trip(kind: OwnerChange) {
    let (mut u, on_rect) = selected_rect();
    let selected = u.app.session.selection().to_vec();
    let original_page = u.app.session.ds().unwrap().page;
    if matches!(kind, OwnerChange::Document) {
        u.app.session.execute("file.new", &json!({})).unwrap();
        // Initialize the secondary document's actual canvas viewport before any finger is
        // down. Its first layout normalization is setup, not leaked gesture navigation.
        u.frame(vec![]);
        u.app.session.execute("doc.switch", &json!({"index": 0})).unwrap();
    } else if matches!(kind, OwnerChange::Page) {
        u.app.session.execute("page.add", &json!({"name": "Other page"})).unwrap();
        u.app.session.execute("page.select", &json!({"id": original_page})).unwrap();
        u.app.session.execute("select.set", &json!({"ids": selected})).unwrap();
    }
    u.frame(vec![]);
    let a = background(&u, on_rect) - vec2(50.0, 0.0);
    let b = a + vec2(100.0, 0.0);
    web_finger_down(&mut u, 1, a);
    web_finger_down(&mut u, 2, b);
    match kind {
        OwnerChange::Document => {
            u.app.session.execute("doc.switch", &json!({"index": 1})).unwrap();
        }
        OwnerChange::Page => {
            u.app.session.execute("page.select", &json!({"index": 1})).unwrap();
        }
        OwnerChange::Tool => {
            u.app.session.execute("tool.scale", &json!({})).unwrap();
        }
    }
    let away_doc = u.app.session.ds().unwrap().doc.clone();
    let away_view = u.app.session.ds().unwrap().view.clone();
    raw_web_move(&mut u, 1, a + vec2(5.0, 0.0));
    assert_eq!(u.app.session.ds().unwrap().doc, away_doc);
    assert_eq!(u.app.session.ds().unwrap().view, away_view, "the mismatched owner receives no navigation");
    match kind {
        OwnerChange::Document => {
            u.app.session.execute("doc.switch", &json!({"index": 0})).unwrap();
        }
        OwnerChange::Page => {
            u.app.session.execute("page.select", &json!({"id": original_page})).unwrap();
        }
        OwnerChange::Tool => {
            u.app.session.execute("tool.move", &json!({})).unwrap();
        }
    }
    // Page switching normally changes its view/selection. Establish the return state first;
    // only the following still-held contact events are measured for leakage.
    u.app.session.execute("select.set", &json!({"ids": selected})).unwrap();
    let before = u.app.session.ds().unwrap().doc.clone();
    let view = u.app.session.ds().unwrap().view.clone();
    let undo = u.app.session.undo_depth().0;
    let delta = vec2(60.0, 20.0);
    u.frame(vec![Event::PointerMoved(a + delta), web_finger(TouchPhase::Move, 1, a + delta), web_finger(TouchPhase::Move, 2, b + delta)]);
    assert_eq!(u.app.session.ds().unwrap().view, view, "returning to the original {kind:?} must not resume a cancelled pinch/pan");
    two_finger_release(&mut u, a + delta, b + delta, true);
    assert_two_finger_preserved(&u, &before, &selected, undo);
    assert_eq!(u.app.session.ds().unwrap().view, view, "the last release cannot restart cancelled navigation");
}

#[test]
fn navigation_cancellation_stays_cancelled_after_document_away_and_back() {
    owner_round_trip(OwnerChange::Document);
}

#[test]
fn navigation_cancellation_stays_cancelled_after_page_away_and_back() {
    owner_round_trip(OwnerChange::Page);
}

#[test]
fn navigation_cancellation_stays_cancelled_after_tool_away_and_back() {
    owner_round_trip(OwnerChange::Tool);
}

#[test]
fn second_finger_start_and_end_in_one_frame_never_replays_the_first_withheld_press() {
    let (mut u, on_rect) = selected_rect();
    let a = background(&u, on_rect);
    let b = a + vec2(60.0, 0.0);
    let before = u.app.session.ds().unwrap().doc.clone();
    let selection = u.app.session.selection().to_vec();
    let undo = u.app.session.undo_depth().0;
    web_finger_down(&mut u, 1, a);
    u.frame(vec![button(a, true), web_finger(TouchPhase::Start, 2, b), button(a, false), Event::PointerGone, web_finger(TouchPhase::End, 2, b)]);
    assert_two_finger_preserved(&u, &before, &selection, undo);
    raw_web_move(&mut u, 1, a + vec2(1.0, 0.0));
    web_finger_up(&mut u, 1, a + vec2(1.0, 0.0));
    assert_two_finger_preserved(&u, &before, &selection, undo);
}

#[test]
fn fully_batched_two_contacts_are_consumed_without_swallowing_the_immediate_next_ordinary_tap() {
    let (mut u, on_rect) = selected_rect();
    let a = background(&u, on_rect);
    let b = a + vec2(60.0, 0.0);
    let before = u.app.session.ds().unwrap().doc.clone();
    let selection = u.app.session.selection().to_vec();
    let undo = u.app.session.undo_depth().0;
    u.frame(vec![
        Event::PointerMoved(a),
        button(a, true),
        web_finger(TouchPhase::Start, 1, a),
        button(a, true),
        web_finger(TouchPhase::Start, 2, b),
        button(a, false),
        Event::PointerGone,
        web_finger(TouchPhase::End, 2, b),
        button(a, false),
        Event::PointerGone,
        web_finger(TouchPhase::End, 1, a),
    ]);
    assert_two_finger_preserved(&u, &before, &selection, undo);
    // No idle reset frame: a fully released old episode cannot own this fresh touch.
    touch_down(&mut u, a + vec2(30.0, 0.0));
    touch_up(&mut u, a + vec2(30.0, 0.0));
    assert!(u.app.session.selection().is_empty(), "the immediate fresh ordinary tap still clicks");
    assert!(!u.app.session.tool.busy());
}

#[test]
fn touch_cancel_of_all_contacts_does_not_replay_or_leave_navigation_ownership() {
    let (mut u, on_rect) = selected_rect();
    let a = background(&u, on_rect);
    let b = a + vec2(60.0, 0.0);
    let before = u.app.session.ds().unwrap().doc.clone();
    let selection = u.app.session.selection().to_vec();
    let undo = u.app.session.undo_depth().0;
    web_finger_down(&mut u, 1, a);
    web_finger_down(&mut u, 2, b);
    // Web touchcancel emits only Touch Cancel, without pointer Up or Gone.
    u.frame(vec![web_finger(TouchPhase::Cancel, 1, a), web_finger(TouchPhase::Cancel, 2, b)]);
    u.frame(vec![Event::PointerMoved(a + vec2(10.0, 0.0))]);
    assert_two_finger_preserved(&u, &before, &selection, undo);
    touch_down(&mut u, a + vec2(30.0, 0.0));
    touch_up(&mut u, a + vec2(30.0, 0.0));
    assert!(u.app.session.selection().is_empty());
}

fn prototype_touch_scene() -> (Ui, figcraft_doc::NodeId, figcraft_doc::NodeId) {
    let (mut u, _) = selected_rect();
    let source = u.app.session.selection()[0];
    let dest = u.app.session.execute("frame.create", &json!({"x": 500, "y": 100, "width": 200, "height": 200})).unwrap();
    let dest = figcraft_engine::params::id_of(&dest["id"]).unwrap();
    u.app.session.execute("select.set", &json!({"ids": [source]})).unwrap();
    u.app.ui.right_tab = crate::RightTab::Prototype;
    u.frame(vec![]);
    (u, source, dest)
}

fn prototype_handle(u: &Ui, source: figcraft_doc::NodeId) -> Pos2 {
    let b = u.app.session.doc().unwrap().world_bounds(source).unwrap();
    u.app.to_screen(b.x1, (b.y0 + b.y1) * 0.5) + vec2(11.0, 0.0)
}

#[test]
fn acquired_navigation_blocks_a_third_web_primary_down_over_a_prototype_handle() {
    let (mut u, source, dest) = prototype_touch_scene();
    let a = background(&u, prototype_handle(&u, source));
    let b = a + vec2(60.0, 0.0);
    let before = u.app.session.ds().unwrap().doc.clone();
    let selection = u.app.session.selection().to_vec();
    let undo = u.app.session.undo_depth().0;
    web_finger_down(&mut u, 1, a);
    web_finger_down(&mut u, 2, b);
    let handle = prototype_handle(&u, source);
    // Actual web order in a coalesced frame: primary Move, then repeated primary Down
    // at that retained primary position, then the third contact's Touch Start.
    u.frame(vec![Event::PointerMoved(handle), web_finger(TouchPhase::Move, 1, handle), button(handle, true), web_finger(TouchPhase::Start, 3, b)]);
    let target = u.app.session.doc().unwrap().world_bounds(dest).unwrap().center();
    let target = u.app.to_screen(target.x, target.y);
    raw_web_move(&mut u, 1, target);
    web_finger_up(&mut u, 3, b);
    two_finger_release(&mut u, target, b, true);
    assert_two_finger_preserved(&u, &before, &selection, undo);
    assert!(crate::proto_panel::editor(&u.ctx).is_none(), "navigation did not start a prototype editor/connection");
}

#[test]
fn an_unclaimed_first_finger_on_a_prototype_handle_keeps_prototype_priority() {
    let (mut u, source, dest) = prototype_touch_scene();
    let handle = prototype_handle(&u, source);
    let count = u.app.session.doc().unwrap().node(source).unwrap().interactions.len();
    u.frame(vec![Event::PointerMoved(handle)]);
    web_finger_down(&mut u, 1, handle);
    let target = u.app.session.doc().unwrap().world_bounds(dest).unwrap().center();
    let target = u.app.to_screen(target.x, target.y);
    raw_web_move(&mut u, 1, target);
    web_finger_up(&mut u, 1, target);
    assert_eq!(u.app.session.doc().unwrap().node(source).unwrap().interactions.len(), count + 1);
    assert!(crate::proto_panel::editor(&u.ctx).is_some(), "the original prototype first-press path remains usable");
}

#[test]
fn an_existing_gap_handle_interaction_keeps_its_release_path_when_a_second_contact_joins() {
    let mut s = Session::new();
    let frame = s.execute("frame.create", &json!({"x": 100, "y": 100, "width": 300, "height": 300, "props": {"layout": "horizontal", "padding": 0, "gap": 10, "crossGap": 10, "wrap": true, "contentAlign": "spaceBetween"}})).unwrap();
    let frame = figcraft_engine::params::id_of(&frame["id"]).unwrap();
    for _ in 0..3 {
        s.execute("shape.rectangle", &json!({"parent": frame, "width": 120, "height": 60})).unwrap();
    }
    s.execute("select.set", &json!({"ids": [frame]})).unwrap();
    let mut u = Ui::new(s);
    let a = u.app.to_screen(250.0, 250.0);
    u.frame(vec![Event::PointerMoved(a)]);
    u.frame(vec![Event::PointerMoved(a)]);
    web_finger_down(&mut u, 1, a);
    assert!(u.app.session.ds().unwrap().interaction.is_some(), "the actual gap handle owns a UI interaction");
    let b = background(&u, a);
    web_finger_down(&mut u, 2, b);
    two_finger_release(&mut u, a, b, false);
    assert!(u.app.session.ds().unwrap().interaction.is_none(), "background takeover never swallows the handle's release");
    assert!(!u.app.session.tool.busy());
    for _ in 0..40 {
        u.frame(vec![]);
    }
    let fresh = background(&u, a);
    touch_down(&mut u, fresh);
    touch_up(&mut u, fresh);
    assert!(u.app.session.selection().is_empty(), "the released handle permits a fresh ordinary background tap");
    assert!(u.app.session.ds().unwrap().interaction.is_none());
}

#[test]
fn one_web_start_and_one_end_for_two_batched_contacts_preserve_the_selected_root() {
    let (mut u, on_rect) = selected_rect();
    let a = background(&u, on_rect);
    let b = a + vec2(60.0, 0.0);
    let before = u.app.session.ds().unwrap().doc.clone();
    let selection = u.app.session.selection().to_vec();
    let undo = u.app.session.undo_depth().0;
    // One DOM touchstart/touchend each has both contacts in changedTouches: one Down/Up.
    u.frame(vec![
        button(a, true),
        web_finger(TouchPhase::Start, 1, a),
        web_finger(TouchPhase::Start, 2, b),
        button(a, false),
        Event::PointerGone,
        web_finger(TouchPhase::End, 1, a),
        web_finger(TouchPhase::End, 2, b),
    ]);
    assert_two_finger_preserved(&u, &before, &selection, undo);
    touch_down(&mut u, a + vec2(30.0, 0.0));
    touch_up(&mut u, a + vec2(30.0, 0.0));
    assert!(u.app.session.selection().is_empty(), "the following ordinary touch is a separate episode");
}

#[test]
fn many_contact_overflow_and_ends_while_canvas_is_hidden_do_not_poison_the_next_touch() {
    let (mut u, on_rect) = selected_rect();
    let a = background(&u, on_rect);
    let before = u.app.session.ds().unwrap().doc.clone();
    let selection = u.app.session.selection().to_vec();
    let undo = u.app.session.undo_depth().0;
    web_finger_down(&mut u, 1, a);
    let mut starts = vec![button(a, true)];
    for id in 2..=40 {
        starts.push(web_finger(TouchPhase::Start, id, a + vec2(id as f32, 0.0)));
    }
    u.frame(starts);
    assert_two_finger_preserved(&u, &before, &selection, undo);
    // Canvas input does not run on Home, but egui still processes these real End events.
    u.app.ui.home = true;
    let mut ends = vec![button(a, false), Event::PointerGone];
    for id in 1..=40 {
        ends.push(web_finger(TouchPhase::End, id, a + vec2(id as f32, 0.0)));
    }
    u.frame(ends);
    u.app.ui.home = false;
    u.frame(vec![]);
    assert_two_finger_preserved(&u, &before, &selection, undo);
    touch_down(&mut u, a + vec2(50.0, 0.0));
    touch_up(&mut u, a + vec2(50.0, 0.0));
    assert!(u.app.session.selection().is_empty(), "authoritative no-touches reconciliation releases saturated/stale contacts");
    assert!(!u.app.session.tool.busy());
}

fn source12_badge_scene() -> (Ui, figcraft_doc::NodeId, Pos2) {
    let mut s = Session::new();
    let locked = s.execute("shape.rectangle", &json!({"x":100,"y":100,"width":200,"height":140})).unwrap();
    let locked = figcraft_engine::params::id_of(&locked["id"]).unwrap();
    let other = s.execute("shape.rectangle", &json!({"x":500,"y":100,"width":100,"height":100})).unwrap();
    s.execute("object.toggleLock", &json!({"ids":[locked],"locked":true})).unwrap();
    s.execute("select.set", &json!({"ids":[other["id"]]})).unwrap();
    let mut u = Ui::new(s);
    let bounds = u.app.session.doc().unwrap().world_bounds(locked).unwrap();
    let center = u.app.to_screen(bounds.center().x, bounds.center().y);
    u.frame(vec![Event::PointerMoved(center)]);
    u.frame(vec![]);
    let at = u.app.to_screen(bounds.x0, bounds.y0) + vec2(14.0, 14.0);
    assert!(u.app.canvas.rect.contains(at));
    u.frame(vec![Event::PointerMoved(at)]);
    u.frame(vec![]);
    assert!(u.app.session.ds().unwrap().hover.is_none(), "a locked badge sits over background for the ordinary tool hit test");
    assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked);
    (u, locked, at)
}

#[test]
fn source12_badge_two_stationary_contacts_never_activate_the_visible_unlock_button() {
    for primary_first in [false, true] {
        let (mut u, locked, a) = source12_badge_scene();
        let b = a + vec2(65.0, 0.0);
        let before = u.app.session.ds().unwrap().doc.clone();
        let selection = u.app.session.selection().to_vec();
        let undo = u.app.session.undo_depth().0;
        let view = u.app.session.ds().unwrap().view.clone();
        web_finger_down(&mut u, 1, a);
        web_finger_down(&mut u, 2, b);
        two_finger_release(&mut u, a, b, primary_first);
        assert!(
            u.app.session.doc().unwrap().node(locked).unwrap().locked,
            "acquired navigation cannot click the badge: primary_first={primary_first}"
        );
        assert_two_finger_preserved(&u, &before, &selection, undo);
        assert_eq!(u.app.session.ds().unwrap().view, view, "stationary contacts do not move the camera");
    }
}

#[test]
fn source12_badge_two_moving_contacts_change_only_the_camera() {
    for primary_first in [false, true] {
        let (mut u, locked, a) = source12_badge_scene();
        let b = a + vec2(65.0, 0.0);
        let before = u.app.session.ds().unwrap().doc.clone();
        let selection = u.app.session.selection().to_vec();
        let undo = u.app.session.undo_depth().0;
        let view = u.app.session.ds().unwrap().view.clone();
        web_finger_down(&mut u, 1, a);
        web_finger_down(&mut u, 2, b);
        for step in 1..=4 {
            let delta = vec2(step as f32 * 15.0, step as f32 * 5.0);
            u.frame(vec![web_finger(TouchPhase::Move, 1, a + delta), web_finger(TouchPhase::Move, 2, b + delta), Event::PointerMoved(a + delta)]);
        }
        let delta = vec2(60.0, 20.0);
        two_finger_release(&mut u, a + delta, b + delta, primary_first);
        assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked);
        assert_two_finger_preserved(&u, &before, &selection, undo);
        let after = &u.app.session.ds().unwrap().view;
        assert!((after.cx - view.cx).hypot(after.cy - view.cy) > 20.0, "the acquired navigation still pans");
        assert!((after.zoom - view.zoom).abs() < 1e-6);
    }
}

#[test]
fn source12_badge_single_finger_drag_is_a_pan_without_unlocking() {
    let (mut u, locked, at) = source12_badge_scene();
    let before = u.app.session.ds().unwrap().doc.clone();
    let selection = u.app.session.selection().to_vec();
    let undo = u.app.session.undo_depth().0;
    let view = u.app.session.ds().unwrap().view.clone();
    touch_down(&mut u, at);
    for step in 1..=6 {
        touch_move(&mut u, at + vec2(step as f32 * 12.0, 0.0));
    }
    touch_up(&mut u, at + vec2(72.0, 0.0));
    assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked);
    assert_two_finger_preserved(&u, &before, &selection, undo);
    assert!((u.app.session.ds().unwrap().view.cx - view.cx).abs() > 20.0);
}

#[test]
fn source12_badge_mouse_activation_still_unlocks_once_and_undo_restores_it() {
    let (mut u, locked, at) = source12_badge_scene();
    let selection = u.app.session.selection().to_vec();
    let undo = u.app.session.undo_depth().0;
    u.button(at, true);
    u.button(at, false);
    assert!(!u.app.session.doc().unwrap().node(locked).unwrap().locked);
    assert_eq!(u.app.session.selection(), selection);
    assert_eq!(u.app.session.undo_depth().0, undo + 1);
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked);
}

#[test]
fn source12_badge_control_an_actual_unlocked_layer_stays_tool_owned() {
    let (mut u, at) = selected_rect();
    let id = u.app.session.selection()[0];
    let before = figcraft_geom::translation(u.app.session.doc().unwrap().world_transform(id));
    let view = u.app.session.ds().unwrap().view.clone();
    touch_down(&mut u, at);
    for step in 1..=6 {
        touch_move(&mut u, at + vec2(step as f32 * 10.0, 0.0));
    }
    touch_up(&mut u, at + vec2(60.0, 0.0));
    let after = figcraft_geom::translation(u.app.session.doc().unwrap().world_transform(id));
    assert!(after.x - before.x > 30.0);
    assert_eq!(u.app.session.ds().unwrap().view, view);
    assert!(!u.app.session.tool.busy());
}

fn source12_acquired_background_beside_badge() -> (Ui, figcraft_doc::NodeId, Pos2, Pos2) {
    let (mut u, locked, badge) = source12_badge_scene();
    // The registered button spans badge.x +/-10. Start2px outside that real widget,
    // still over the locked artwork which the ordinary tool sees as background.
    let start = badge - vec2(12.0, 0.0);
    u.frame(vec![Event::PointerMoved(start)]);
    let doc = u.app.session.ds().unwrap().doc.clone();
    let selection = u.app.session.selection().to_vec();
    let view = u.app.session.ds().unwrap().view.clone();
    web_finger_down(&mut u, 1, start);
    assert!(!u.app.session.tool.busy(), "the actual background Down is withheld, not a tool drag");
    assert_eq!(u.app.session.selection(), selection);
    let primary = badge - vec2(8.0, 0.0);
    raw_web_move(&mut u, 1, primary); //4px: inside the button, below the pan threshold.
    assert_eq!(u.app.session.ds().unwrap().view, view);
    assert_eq!(u.app.session.ds().unwrap().doc, doc);
    let second = primary + vec2(65.0, 0.0);
    web_finger_down(&mut u, 2, second); //duplicate web PrimaryDown at primary.
    assert!(!u.app.session.tool.busy());
    (u, locked, primary, second)
}

#[test]
fn source12_badge_acquired_background_release_cannot_click_a_button_it_later_crossed() {
    for primary_first in [false, true] {
        let (mut u, locked, a, b) = source12_acquired_background_beside_badge();
        let doc = u.app.session.ds().unwrap().doc.clone();
        let selection = u.app.session.selection().to_vec();
        let undo = u.app.session.undo_depth().0;
        let view = u.app.session.ds().unwrap().view.clone();
        two_finger_release(&mut u, a, b, primary_first);
        assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked);
        assert_two_finger_preserved(&u, &doc, &selection, undo);
        assert_eq!(u.app.session.ds().unwrap().view, view);
        // Touch End sends PointerGone. Rediscover and display the badge before a new mouse press.
        u.frame(vec![Event::PointerMoved(a)]);
        u.frame(vec![]);
        // Navigation ownership ends with its contacts: a later real mouse click still works.
        u.button(a, true);
        u.button(a, false);
        assert!(!u.app.session.doc().unwrap().node(locked).unwrap().locked);
        assert_eq!(u.app.session.undo_depth().0, undo + 1);
        assert_eq!(u.app.session.selection(), selection);
    }
}

#[test]
fn source12_badge_acquired_background_keeps_panning_while_primary_remains_over_badge() {
    for primary_first in [false, true] {
        let (mut u, locked, a, b) = source12_acquired_background_beside_badge();
        let doc = u.app.session.ds().unwrap().doc.clone();
        let selection = u.app.session.selection().to_vec();
        let undo = u.app.session.undo_depth().0;
        let view = u.app.session.ds().unwrap().view.clone();
        // All primary positions remain strictly inside the old badge. This catches a
        // blocked camera, rather than merely moving far enough to leave the widget.
        for step in 1..=3 {
            let delta = vec2(step as f32 * 3.0, 0.0);
            u.frame(vec![web_finger(TouchPhase::Move, 1, a + delta), web_finger(TouchPhase::Move, 2, b + delta), Event::PointerMoved(a + delta)]);
        }
        let after = &u.app.session.ds().unwrap().view;
        assert!((after.cx - view.cx).abs() > 6.0 / view.zoom, "acquired navigation cannot be blocked by a late canvas button");
        assert!((after.zoom - view.zoom).abs() < 1e-6);
        two_finger_release(&mut u, a + vec2(9.0, 0.0), b + vec2(9.0, 0.0), primary_first);
        assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked);
        assert_two_finger_preserved(&u, &doc, &selection, undo);
    }
}
