use super::*;
use crate::present_tests::{Ui, id};
use egui::{Event, Key, Modifiers};

fn setup(zoom: f64) -> (Ui, NodeId) {
    let mut s = figcraft_engine::Session::new();
    let source = id(&s.execute("frame.create", &json!({"x":0,"y":0,"width":120,"height":80,"name":"Original source"})).unwrap());
    s.execute("select.set", &json!({"ids":[source]})).unwrap();
    let mut u = Ui::new(s);
    u.app.ui.right_tab = RightTab::Prototype;
    {
        let ds = u.app.session.ds_mut().unwrap();
        ds.view.zoom = zoom;
        ds.view.cx = 60.0;
        ds.view.cy = 40.0;
    }
    u.frames(3, Vec::new);
    (u, source)
}

fn begin(u: &mut Ui, source: NodeId) {
    let doc = u.app.session.doc().unwrap().clone();
    let (_, point) = *handles(&u.app, &doc, &[source]).first().unwrap();
    assert!(u.app.canvas.rect.contains(point));
    u.button(point, true);
    assert!(u.ctx.data(|d| d.get_temp::<Drag>(drag_id())).is_some(), "the actual prototype handle acquired its press");
}

fn camera(u: &Ui) -> (f64, f64) {
    let view = &u.app.session.ds().unwrap().view;
    (view.cx, view.cy)
}

#[test]
fn connection_edge_pan_held_pointer_moves_all_four_sides_in_screen_space_without_document_edits() {
    for zoom in [0.5, 1.0, 2.0] {
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let (mut u, source) = setup(zoom);
            let doc = u.app.session.doc().unwrap().clone();
            let history = u.app.session.undo_depth();
            let was_dirty = u.app.session.ds().unwrap().is_dirty();
            begin(&mut u, source);
            let r = u.app.canvas.rect;
            let point = pos2(
                if dx > 0.0 {
                    r.right() - 3.0
                } else if dx < 0.0 {
                    r.left() + 3.0
                } else {
                    r.center().x
                },
                if dy > 0.0 {
                    r.bottom() - 3.0
                } else if dy < 0.0 {
                    r.top() + 3.0
                } else {
                    r.center().y
                },
            );
            let before = camera(&u);
            u.frames(12, || vec![Event::PointerMoved(point)]);
            let after = camera(&u);
            let distance = ((after.0 - before.0) * dx + (after.1 - before.1) * dy) * zoom;
            assert!(distance > 1.0 && distance < 200.0, "held edge must smoothly pan toward ({dx},{dy}) at zoom {zoom}: {before:?} -> {after:?}");
            let cross = ((after.0 - before.0) * dy - (after.1 - before.1) * dx) * zoom;
            assert!(cross.abs() < 1e-6);
            let d = u.ctx.data(|data| data.get_temp::<Drag>(drag_id())).unwrap();
            let actual_source = handles(&u.app, u.app.session.doc().unwrap(), &[source])[0].1;
            assert!(d.from.distance(actual_source) < 0.01, "camera movement must not strand the source anchor");
            assert_eq!(d.pos, point);
            assert_eq!(u.app.session.doc().unwrap(), &doc);
            assert_eq!(u.app.session.undo_depth(), history);
            assert_eq!(u.app.session.ds().unwrap().is_dirty(), was_dirty);
            u.button(point, false);
            let stopped = camera(&u);
            u.frames(8, Vec::new);
            assert_eq!(camera(&u), stopped, "release stops camera motion");
        }
    }
}

#[test]
fn connection_edge_pan_reveals_offscreen_destination_and_drop_creates_one_undoable_link() {
    let (mut u, source) = setup(1.0);
    let canvas = u.app.canvas.rect;
    let at = u.app.to_page(pos2(canvas.right() + 100.0, canvas.center().y));
    let dest = id(&u.run("frame.create", json!({"x":at.x-40.0,"y":at.y-40.0,"width":80,"height":80,"name":"Original offscreen target"})));
    u.run("select.set", json!({"ids":[source]}));
    let before = u.app.session.doc().unwrap().clone();
    let history = u.app.session.undo_depth();
    assert!(screen_rect(&u.app, &before, dest).unwrap().left() > canvas.right());
    begin(&mut u, source);
    let edge = pos2(canvas.right() - 3.0, canvas.center().y);
    for _ in 0..180 {
        u.frame(vec![Event::PointerMoved(edge)]);
        if screen_rect(&u.app, u.app.session.doc().unwrap(), dest).unwrap().center().x <= edge.x {
            break;
        }
    }
    let shown = screen_rect(&u.app, u.app.session.doc().unwrap(), dest).unwrap();
    assert!(shown.contains(edge), "held edge must reveal the target under the pointer: {shown:?}, {edge:?}");
    assert_eq!(u.app.session.doc().unwrap(), &before, "panning previews no document edit");
    assert_eq!(u.app.session.undo_depth(), history);
    u.button(edge, false);
    let interactions = &u.app.session.doc().unwrap().node(source).unwrap().interactions;
    assert_eq!(interactions.len(), 1);
    assert_eq!(figcraft_doc::proto::walk(&interactions[0].actions).find_map(|a| a.dest()), Some(dest));
    assert_eq!(u.app.session.undo_depth(), (history.0 + 1, history.1));
    u.run("edit.undo", json!({}));
    assert_eq!(u.app.session.doc().unwrap(), &before);
}

#[test]
fn connection_edge_pan_no_gesture_center_and_sidebar_controls_keep_the_camera_and_document() {
    let (mut u, source) = setup(1.0);
    let before = u.app.session.doc().unwrap().clone();
    let history = u.app.session.undo_depth();
    let canvas = u.app.canvas.rect;
    let camera_before = camera(&u);
    let edge = pos2(canvas.right() - 3.0, canvas.center().y);
    u.frames(8, || vec![Event::PointerMoved(edge)]);
    assert_eq!(camera(&u), camera_before, "hovering the edge acquires no gesture");
    begin(&mut u, source);
    u.frames(8, || vec![Event::PointerMoved(canvas.center())]);
    assert_eq!(camera(&u), camera_before, "the center is outside the pan margin");
    let sidebar = pos2(canvas.right() + 20.0, canvas.center().y);
    u.frames(8, || vec![Event::PointerMoved(sidebar)]);
    assert!(camera(&u).0 > camera_before.0, "the owned drag pans beyond the canvas edge");
    assert_eq!(camera(&u).1, camera_before.1);
    u.button(sidebar, false);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), history);
}

#[test]
fn connection_edge_pan_escape_retires_the_owned_gesture_without_a_link_or_later_motion() {
    let (mut u, source) = setup(1.0);
    let before = u.app.session.doc().unwrap().clone();
    let history = u.app.session.undo_depth();
    begin(&mut u, source);
    u.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    assert!(u.ctx.data(|d| d.get_temp::<Drag>(drag_id())).is_none(), "Escape retires the actual connection drag");
    let stopped = camera(&u);
    let point = pos2(u.app.canvas.rect.right() - 3.0, u.app.canvas.rect.center().y);
    u.frames(8, || vec![Event::PointerMoved(point)]);
    u.button(point, false);
    assert_eq!(camera(&u), stopped);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), history);
}

#[test]
fn connection_edge_pan_retires_on_focus_pointer_mode_document_page_source_and_viewport_changes() {
    #[derive(Clone, Copy, Debug)]
    enum Retirement {
        Focus,
        Pointer,
        Mode,
        Document,
        Page,
        Source,
        Viewport,
    }
    use Retirement::*;
    for reason in [Focus, Pointer, Mode, Document, Page, Source, Viewport] {
        let (mut u, source) = setup(1.0);
        begin(&mut u, source);
        let events = match reason {
            Focus => vec![Event::WindowFocused(false)],
            Pointer => vec![Event::PointerGone],
            Mode => {
                u.app.ui.right_tab = RightTab::Design;
                vec![]
            }
            Document => {
                u.app.session.execute("file.new", &json!({"name":"New owner"})).unwrap();
                vec![]
            }
            Page => {
                u.app.session.execute("page.add", &json!({"name":"New page"})).unwrap();
                vec![]
            }
            Source => {
                u.app.session.execute("edit.delete", &json!({"ids":[source]})).unwrap();
                vec![]
            }
            Viewport => {
                // A stale stored gesture from another native window must never acquire this one.
                let mut d = u.ctx.data(|m| m.get_temp::<Drag>(drag_id())).unwrap();
                d.viewport = egui::ViewportId::from_hash_of("retired prototype window");
                u.ctx.data_mut(|m| m.insert_temp(drag_id(), d));
                vec![]
            }
        };
        let before = u.app.session.doc().unwrap().clone();
        let history = u.app.session.undo_depth();
        u.frame(events);
        assert!(u.ctx.data(|m| m.get_temp::<Drag>(drag_id())).is_none(), "{reason:?} retires the acquired gesture");
        u.app.ui.right_tab = RightTab::Prototype;
        u.frame(vec![Event::WindowFocused(true)]);
        let stopped = camera(&u);
        let edge = pos2(u.app.canvas.rect.right() - 3.0, u.app.canvas.rect.center().y);
        u.frames(8, || vec![Event::PointerMoved(edge)]);
        u.button(edge, false);
        assert_eq!(camera(&u), stopped, "{reason:?} cannot resume the stale press");
        assert_eq!(u.app.session.doc().unwrap(), &before, "{reason:?} adds no interaction");
        assert_eq!(u.app.session.undo_depth(), history, "{reason:?} adds no Undo step");
    }
}

#[test]
fn connection_edge_pan_touch_release_pointer_gone_keeps_the_valid_drop_position() {
    let (mut u, source) = setup(1.0);
    let dest = id(&u.run("frame.create", json!({"x":240,"y":0,"width":120,"height":80})));
    u.run("select.set", json!({"ids":[source]}));
    let before = u.app.session.doc().unwrap().clone();
    let history = u.app.session.undo_depth();
    begin(&mut u, source);
    let point = screen_rect(&u.app, &before, dest).unwrap().center();
    u.frame(vec![Event::PointerMoved(point)]);
    u.frame(vec![
        Event::PointerButton { pos: point, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
        Event::PointerGone,
    ]);
    assert!(u.ctx.data(|m| m.get_temp::<Drag>(drag_id())).is_none());
    let interactions = &u.app.session.doc().unwrap().node(source).unwrap().interactions;
    assert_eq!(interactions.len(), 1);
    assert_eq!(figcraft_doc::proto::walk(&interactions[0].actions).find_map(|a| a.dest()), Some(dest));
    assert_eq!(u.app.session.undo_depth(), (history.0 + 1, history.1));
    u.run("edit.undo", json!({}));
    assert_eq!(u.app.session.doc().unwrap(), &before);
}

#[test]
fn connection_edge_pan_stationary_hold_requests_frames_and_keeps_moving_without_new_pointer_events() {
    let (mut u, source) = setup(1.0);
    let before = u.app.session.doc().unwrap().clone();
    let history = u.app.session.undo_depth();
    begin(&mut u, source);
    let edge = pos2(u.app.canvas.rect.right() - 3.0, u.app.canvas.rect.center().y);
    u.frame(vec![Event::PointerMoved(edge)]);
    let start = camera(&u);
    let out = u.frame_output(vec![]);
    assert_eq!(out.viewport_output[&egui::ViewportId::ROOT].repaint_delay, std::time::Duration::ZERO);
    u.frames(10, Vec::new);
    assert!(camera(&u).0 - start.0 > 1.0, "the stationary held pointer needs no new move events");
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), history);
    u.button(edge, false);
    let stopped = camera(&u);
    u.frames(8, Vec::new);
    assert_eq!(camera(&u), stopped);
}
