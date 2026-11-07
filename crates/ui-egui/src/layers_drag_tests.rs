//! Dragging rows in the layers panel, driven through egui headless: holding over a collapsed
//! frame opens it, a layer can't be dropped into itself, Escape cancels, and the list scrolls at
//! its edges.

use egui::{Event, Pos2, RawInput, Rect, vec2};
use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::json;

use crate::{FigApp, NoServices};

pub(crate) struct Ui {
    pub(crate) ctx: egui::Context,
    pub(crate) app: FigApp,
    time: f64,
}

impl Ui {
    pub(crate) fn new(s: Session) -> Self {
        let mut u = Self { ctx: egui::Context::default(), app: FigApp::new(s, Box::new(NoServices)), time: 0.0 };
        for _ in 0..3 {
            u.frame(vec![]);
        }
        u
    }

    pub(crate) fn frame(&mut self, events: Vec<Event>) {
        self.frame_output(events);
    }

    /// [`Self::frame`], returning what egui output (the AccessKit tree when it's enabled).
    pub(crate) fn frame_output(&mut self, events: Vec<Event>) -> egui::FullOutput {
        self.frame_with(events, Default::default())
    }

    /// [`Self::frame_output`] with `modifiers` held.
    pub(crate) fn frame_with(&mut self, mut events: Vec<Event>, modifiers: egui::Modifiers) -> egui::FullOutput {
        events.insert(0, Event::ModifiersChanged(modifiers));
        self.frame_raw(RawInput { events, ..Default::default() })
    }

    /// One frame of `raw` input (files dropped on or hovered over the window, say), on the test
    /// screen and clock.
    pub(crate) fn frame_raw(&mut self, raw: RawInput) -> egui::FullOutput {
        self.time += 1.0 / 60.0;
        self.app.clock.set_test_now(Some(self.time));
        let screen_rect = raw.screen_rect.or(Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))));
        let input = RawInput { screen_rect, time: Some(self.time), ..raw };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| app.ui(ui));
        out.textures_delta.clear();
        out
    }

    /// One frame that egui reruns as a discarded pass (the returned output is the presented pass).
    ///
    /// The rerun sees the same `RawInput`; the custom text editor must not apply its events twice,
    /// and the IME descriptor of the presented pass must still describe the first pass's result.
    pub(crate) fn frame_raw_with_discard(&mut self, raw: RawInput) -> egui::FullOutput {
        self.time += 1.0 / 60.0;
        self.app.clock.set_test_now(Some(self.time));
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))), time: Some(self.time), ..raw };
        let Ui { ctx, app, .. } = self;
        let mut out = ctx.run_ui(input, |ui| {
            app.ui(ui);
            if ui.ctx().current_pass_index() == 0 {
                ui.ctx().request_discard("ime test: exercise a discarded pass");
            }
        });
        out.textures_delta.clear();
        out
    }

    /// Move the harness time (and the app's monotonic clock) forward, e.g. past a deadline; the
    /// next frame continues from here.
    pub(crate) fn advance_clock(&mut self, secs: f64) {
        self.time = secs;
        self.app.clock.set_test_now(Some(secs));
    }

    /// The middle of `id`'s row in the layers panel (found by hovering down the panel).
    pub(crate) fn row_at(&mut self, id: NodeId) -> Pos2 {
        let mut hits = Vec::new();
        for y in (60..900).step_by(2) {
            let p = Pos2::new(150.0, y as f32);
            // The panel only sets the hover when a row is under the pointer.
            self.app.session.ds_mut().unwrap().hover = None;
            self.frame(vec![Event::PointerMoved(p)]);
            if self.app.session.ds().unwrap().hover == Some(id) {
                hits.push(p.y);
            } else if !hits.is_empty() {
                break;
            }
        }
        assert!(!hits.is_empty(), "no row for {id:?}");
        Pos2::new(150.0, (hits[0] + hits[hits.len() - 1]) / 2.0)
    }

    pub(crate) fn button(&mut self, p: Pos2, pressed: bool) {
        self.frame(vec![
            Event::PointerMoved(p),
            Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() },
        ]);
    }

    /// Press on `from` and move to `to` in small steps (a drag, not yet released).
    pub(crate) fn drag_to(&mut self, from: Pos2, to: Pos2) {
        self.button(from, true);
        for k in 1..=10 {
            self.frame(vec![Event::PointerMoved(from + (to - from) * (k as f32 / 10.0))]);
        }
    }
}

fn id(v: serde_json::Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

/// A frame with one child (collapsed in the panel) and a rectangle beside it.
fn setup() -> (Ui, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    let frame = id(s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 100, "height": 100})).unwrap());
    let child = id(s.execute("node.create", &json!({"type": "ellipse", "parent": frame, "x": 10, "y": 10, "width": 10, "height": 10})).unwrap());
    let rect = id(s.execute("node.create", &json!({"type": "rectangle", "x": 200, "y": 0, "width": 10, "height": 10})).unwrap());
    s.execute("select.none", &json!({})).unwrap();
    let mut u = Ui::new(s);
    u.app.ui.expanded.clear();
    u.frame(vec![]);
    (u, frame, child, rect)
}

fn finger(u: &mut Ui, phase: egui::TouchPhase, p: Pos2) {
    let mut events =
        vec![Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(1), phase, pos: p, force: Some(0.5) }, Event::PointerMoved(p)];
    if matches!(phase, egui::TouchPhase::Start | egui::TouchPhase::End) {
        events.push(Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: phase == egui::TouchPhase::Start,
            modifiers: Default::default(),
        });
    }
    if phase == egui::TouchPhase::End {
        events.push(Event::PointerGone);
    }
    u.frame(events);
}

#[test]
fn a_finger_scrolls_the_actual_virtualized_layers_without_reordering_or_sweeping() {
    let mut s = Session::new();
    let mut middle = NodeId(0);
    for i in 0..40 {
        let created = id(s
            .execute("node.create", &json!({"type":"rectangle","name":format!("Synthetic row {i}"),"x":i*20,"y":0,"width":10,"height":10}))
            .unwrap());
        // Layers are displayed newest first; choose a middle visible row, not one below
        // the viewport that this test is about to scroll into view.
        if i == 30 {
            middle = created;
        }
    }
    s.execute("select.none", &json!({})).unwrap();
    let mut u = Ui::new(s);
    let at = u.row_at(middle);
    let before = u.app.session.ds().unwrap().doc.clone();
    finger(&mut u, egui::TouchPhase::Start, at);
    for k in 1..=6 {
        finger(&mut u, egui::TouchPhase::Move, at - vec2(0.0, 15.0 * k as f32));
    }
    finger(&mut u, egui::TouchPhase::End, at - vec2(0.0, 90.0));
    u.app.session.ds_mut().unwrap().hover = None;
    u.frame(vec![Event::PointerMoved(at)]);
    assert!(u.app.session.ds().unwrap().hover.is_some_and(|now| now != middle), "different rows now occupy the original screen position");
    assert!(std::sync::Arc::ptr_eq(&before, &u.app.session.ds().unwrap().doc), "scrolling changes neither layer order nor eye/lock properties");
    assert!(u.app.session.selection().is_empty(), "scrolling never selects a row");
}

#[test]
fn a_finger_hold_then_drag_reparents_in_the_actual_layers_panel_on_pointer_gone_release() {
    let (mut u, frame, child, rect) = setup();
    let (from, to) = (u.row_at(rect), u.row_at(frame));
    finger(&mut u, egui::TouchPhase::Start, from);
    for _ in 0..36 {
        finger(&mut u, egui::TouchPhase::Move, from);
    }
    for k in 1..=10 {
        finger(&mut u, egui::TouchPhase::Move, from + (to - from) * (k as f32 / 10.0));
    }
    finger(&mut u, egui::TouchPhase::End, to);
    let doc = u.app.session.doc().unwrap();
    assert_eq!(doc.node(rect).unwrap().parent, Some(frame));
    assert!(doc.node(frame).unwrap().children.contains(&child));
}

#[test]
fn a_held_layer_drop_cannot_reuse_its_node_id_in_a_new_document() {
    let (mut u, frame, _, rect) = setup();
    let (from, to) = (u.row_at(rect), u.row_at(frame));
    finger(&mut u, egui::TouchPhase::Start, from);
    for _ in 0..36 {
        finger(&mut u, egui::TouchPhase::Move, from);
    }
    finger(&mut u, egui::TouchPhase::Move, from + vec2(0.0, 10.0));
    u.app.session.execute("file.new", &json!({})).unwrap();
    for i in 0..5 {
        u.app.session.execute("node.create", &json!({"type":"frame","name":format!("New target {i}"),"width":100,"height":100})).unwrap();
    }
    u.frame(vec![]);
    let before = u.app.session.ds().unwrap().doc.clone();
    finger(&mut u, egui::TouchPhase::Move, to);
    finger(&mut u, egui::TouchPhase::End, to);
    assert!(std::sync::Arc::ptr_eq(&before, &u.app.session.ds().unwrap().doc), "a carried layer ID never mutates a new document");
    assert!(u.app.ui.last_error.is_none(), "a cancelled cross-document drop runs no command");
}

#[test]
fn holding_over_a_collapsed_frame_opens_it_and_dropping_moves_the_layer_in() {
    let (mut u, frame, child, rect) = setup();
    let (r, f) = (u.row_at(rect), u.row_at(frame));
    u.drag_to(r, f);
    assert!(!u.app.ui.expanded.contains(&frame));
    // Hold for a second over the middle of the frame's row.
    for _ in 0..60 {
        u.frame(vec![Event::PointerMoved(f)]);
    }
    assert!(u.app.ui.expanded.contains(&frame), "spring-loaded open");
    u.button(f, false);
    let doc = u.app.session.doc().unwrap();
    assert_eq!(doc.node(rect).unwrap().parent, Some(frame), "dropped into the frame");
    assert!(doc.node(frame).unwrap().children.contains(&child));
}

#[test]
fn a_layer_cant_drop_into_its_own_child_and_escape_cancels() {
    let (mut u, frame, child, rect) = setup();
    u.app.ui.expanded.insert(frame);
    u.frame(vec![]);
    let (f, c) = (u.row_at(frame), u.row_at(child));
    let parent_before = u.app.session.doc().unwrap().node(frame).unwrap().parent;
    u.drag_to(f, c);
    u.button(c, false);
    assert_eq!(u.app.session.doc().unwrap().node(frame).unwrap().parent, parent_before, "not moved into itself");
    assert!(u.app.ui.last_error.is_none(), "{:?}", u.app.ui.last_error);
    // Escape during a drag: releasing over a target then does nothing.
    let (r, f) = (u.row_at(rect), u.row_at(frame));
    u.drag_to(r, f);
    u.frame(vec![Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }]);
    u.button(f, false);
    assert_ne!(u.app.session.doc().unwrap().node(rect).unwrap().parent, Some(frame));
}

#[test]
fn dropping_layers_on_another_page_moves_them_there() {
    let (mut u, _frame, _child, rect) = setup();
    let p1 = u.app.session.page().unwrap();
    let p2 = id(u.app.session.execute("page.add", &json!({})).unwrap());
    u.app.session.execute("page.select", &json!({"id": p1})).unwrap();
    u.frame(vec![]);
    // Find Page 2's row in the Pages list: the click that switches to it.
    let mut at = None;
    for y in (90..400).step_by(4) {
        let p = Pos2::new(150.0, y as f32);
        u.button(p, true);
        u.button(p, false);
        if u.app.session.page().unwrap() == p2 {
            // Rows are 32 high: aim at the middle, past the hit tolerance at its edge.
            at = Some(p + vec2(0.0, 12.0));
            u.app.session.execute("page.select", &json!({"id": p1})).unwrap();
            u.frame(vec![]);
            break;
        }
    }
    let page_row = at.expect("page 2 row");
    let r = u.row_at(rect);
    u.drag_to(r, page_row);
    u.button(page_row, false);
    let doc = u.app.session.doc().unwrap();
    assert_eq!(doc.ancestors(rect).last().copied(), Some(p2), "moved to page 2");
}

#[test]
fn dropping_a_layer_one_place_over_moves_it_one_place() {
    // #992: the panel drops on a gap between rows; the layer ends up in that gap.
    let mut s = Session::new();
    let rect =
        |s: &mut Session, x: i32| id(s.execute("node.create", &json!({"type": "rectangle", "x": x, "y": 0, "width": 10, "height": 10})).unwrap());
    let (a, b, c) = (rect(&mut s, 0), rect(&mut s, 20), rect(&mut s, 40));
    s.execute("select.none", &json!({})).unwrap();
    let page = s.page().unwrap();
    let mut u = Ui::new(s);
    let kids = |u: &Ui| u.app.session.doc().unwrap().node(page).unwrap().children.clone();
    // The list shows c, b, a top to bottom: `a` dragged onto the top edge of `b` goes just above it.
    let (from, to) = (u.row_at(a), u.row_at(b) - vec2(0.0, 12.0));
    u.drag_to(from, to);
    u.button(to, false);
    assert_eq!(kids(&u), [b, a, c], "dropped between b and c");
    // And `c` dragged onto the bottom edge of `a` goes just below it.
    let (from, to) = (u.row_at(c), u.row_at(a) + vec2(0.0, 12.0));
    u.drag_to(from, to);
    u.button(to, false);
    assert_eq!(kids(&u), [b, c, a], "dropped between a and b");
}
