//! Locked layers can be unlocked in place without changing a separate selection or document.
use egui::{Event, PointerButton, Pos2, vec2};
use figcraft_doc::NodeId;
use figcraft_engine::{DocState, Session};
use serde_json::{Value, json};

use crate::layers_drag_tests::Ui;
use crate::live_tests::{flat, text_center, texts};

fn ex(s: &mut Session, command: &str, params: Value) -> Value {
    s.execute(command, &params).unwrap_or_else(|e| panic!("{command}: {e}"))
}

fn node(s: &mut Session, params: Value) -> NodeId {
    NodeId(ex(s, "node.create", params)["id"].as_u64().unwrap())
}

fn locked_scene() -> (Session, NodeId, NodeId) {
    let mut s = Session::new();
    let locked = node(&mut s, json!({"type":"rectangle","name":"Locked rectangle","x":100,"y":100,"width":200,"height":140}));
    let other = node(&mut s, json!({"type":"rectangle","name":"Other","x":500,"y":100,"width":100,"height":100}));
    ex(&mut s, "object.toggleLock", json!({"ids":[locked.0],"locked":true}));
    ex(&mut s, "select.set", json!({"ids":[other.0]}));
    (s, locked, other)
}

fn centre(u: &Ui, id: NodeId) -> Pos2 {
    let p = u.app.session.doc().unwrap().world_bounds(id).unwrap().center();
    u.app.to_screen(p.x, p.y)
}

fn badge(u: &Ui, id: NodeId) -> Pos2 {
    let r = u.app.session.doc().unwrap().world_bounds(id).unwrap();
    let p = u.app.to_screen(r.x0, r.y0) + vec2(14.0, 14.0);
    assert!(u.app.canvas.rect.contains(p), "the locked layer's upper-left is visible");
    p
}

fn click(u: &mut Ui, p: Pos2) {
    u.button(p, true);
    u.button(p, false);
}

fn context(u: &mut Ui, p: Pos2) {
    for pressed in [true, false] {
        u.frame(vec![
            Event::PointerMoved(p),
            Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed, modifiers: Default::default() },
        ]);
    }
}

#[test]
fn canvas_unlock_badge_unlocks_only_the_hovered_owner_and_one_undo_restores_it() {
    let (s, locked, other) = locked_scene();
    let mut u = Ui::new(s);
    let before = u.app.session.undo_depth().0;
    u.frame(vec![Event::PointerMoved(centre(&u, locked))]);
    u.frame(vec![]);
    let at = badge(&u, locked);
    click(&mut u, at);
    assert!(!u.app.session.doc().unwrap().node(locked).unwrap().locked, "the canvas lock control unlocks its hovered layer");
    assert_eq!(u.app.session.selection(), [other]);
    assert!(!u.app.session.doc().unwrap().node(other).unwrap().locked);
    assert_eq!(u.app.session.undo_depth().0, before + 1);
    ex(&mut u.app.session, "edit.undo", json!({}));
    assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked);
    assert_eq!(u.app.session.selection(), [other]);
}

#[test]
fn canvas_unlock_context_menu_names_and_unlocks_the_pointed_layer_without_replacing_selection() {
    let (s, locked, other) = locked_scene();
    let mut u = Ui::new(s);
    let at = centre(&u, locked);
    context(&mut u, at);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let unlock = text_center(&shapes, "Unlock Locked rectangle");
    click(&mut u, unlock);
    assert!(!u.app.session.doc().unwrap().node(locked).unwrap().locked);
    assert_eq!(u.app.session.selection(), [other]);
    assert!(!u.app.session.doc().unwrap().node(other).unwrap().locked);
}

#[test]
fn canvas_unlock_inherited_lock_targets_the_parent_and_never_locks_its_child() {
    let mut s = Session::new();
    let parent = node(&mut s, json!({"type":"frame","name":"Locked parent","x":100,"y":100,"width":260,"height":180}));
    let child = node(&mut s, json!({"type":"rectangle","parent":parent.0,"name":"Child","x":40,"y":40,"width":80,"height":60}));
    ex(&mut s, "object.toggleLock", json!({"ids":[parent.0],"locked":true}));
    let mut u = Ui::new(s);
    u.frame(vec![Event::PointerMoved(centre(&u, child))]);
    u.frame(vec![]);
    let at = badge(&u, parent);
    click(&mut u, at);
    let doc = u.app.session.doc().unwrap();
    assert!(!doc.node(parent).unwrap().locked, "unlock the owner of the effective lock");
    assert!(!doc.node(child).unwrap().locked, "an unlocked child must never be toggled to locked");
}

#[test]
fn canvas_unlock_a_normal_drag_keeps_the_locked_artwork_unchanged() {
    let (s, locked, _) = locked_scene();
    let mut u = Ui::new(s);
    let before = u.app.session.doc().unwrap().node(locked).unwrap().clone();
    let from = centre(&u, locked);
    u.drag_to(from, from + vec2(80.0, 20.0));
    u.button(from + vec2(80.0, 20.0), false);
    assert_eq!(u.app.session.doc().unwrap().node(locked).unwrap(), &before);
}

#[test]
fn canvas_unlock_badge_press_cannot_unlock_another_document_with_the_same_node_ids() {
    let (s, locked, _) = locked_scene();
    let mut b = s.doc().unwrap().clone();
    b.name = "B".into();
    let mut u = Ui::new(s);
    u.app.session.docs.push(DocState::new(b, None));
    assert_ne!(u.app.session.docs[0].uid, u.app.session.docs[1].uid);
    u.frame(vec![Event::PointerMoved(centre(&u, locked))]);
    u.frame(vec![]);
    let at = badge(&u, locked);
    u.button(at, true);
    assert!(u.app.session.docs[0].doc.node(locked).unwrap().locked);
    u.app.session.active = 1;
    u.button(at, false);
    assert_eq!(u.app.session.active, 1);
    assert!(u.app.session.docs[0].doc.node(locked).unwrap().locked, "A's captured press was cancelled");
    assert!(u.app.session.docs[1].doc.node(locked).unwrap().locked, "B's identical NodeId is not the press owner");
}

#[test]
fn canvas_unlock_context_cannot_unlock_another_document_with_the_same_node_ids() {
    let (s, locked, _) = locked_scene();
    let mut b = s.doc().unwrap().clone();
    b.name = "B".into();
    let mut u = Ui::new(s);
    u.app.session.docs.push(DocState::new(b, None));
    let at = centre(&u, locked);
    context(&mut u, at);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let old_row = text_center(&shapes, "Unlock Locked rectangle");
    u.app.session.active = 1;
    click(&mut u, old_row);
    assert!(u.app.session.docs[0].doc.node(locked).unwrap().locked);
    assert!(u.app.session.docs[1].doc.node(locked).unwrap().locked);
    assert!(u.app.ui.context_menu.is_none(), "retire the old popup before legacy rows can run");
}

#[test]
fn canvas_unlock_stationary_hover_reuses_the_existing_hover_cache() {
    let (s, locked, _) = locked_scene();
    let mut u = Ui::new(s);
    let at = centre(&u, locked);
    u.frame(vec![Event::PointerMoved(at)]);
    let queries = crate::canvas::locked::hover_queries(&u.app);
    assert_eq!(queries, 1);
    for _ in 0..20 {
        u.frame(vec![]);
    }
    assert_eq!(crate::canvas::locked::hover_queries(&u.app), queries, "idle frames never walk locked geometry again");
    u.frame(vec![Event::PointerMoved(at + vec2(2.0, 2.0))]);
    assert_eq!(crate::canvas::locked::hover_queries(&u.app), queries + 1);
}

#[test]
fn canvas_unlock_unlocked_foreground_prevents_a_button_for_the_locked_layer_behind_it() {
    let (mut s, locked, other) = locked_scene();
    let cover = node(&mut s, json!({"type":"rectangle","name":"Cover","x":100,"y":100,"width":200,"height":140}));
    ex(&mut s, "select.set", json!({"ids":[other.0]}));
    let mut u = Ui::new(s);
    u.frame(vec![Event::PointerMoved(centre(&u, locked))]);
    let at = badge(&u, locked);
    click(&mut u, at);
    assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked);
    assert!(!u.app.session.doc().unwrap().node(cover).unwrap().locked);
}

#[test]
fn canvas_unlock_read_only_version_preview_keeps_the_lock_and_history() {
    let (mut s, locked, _) = locked_scene();
    ex(&mut s, "file.saveVersion", json!({"name":"Locked version"}));
    ex(&mut s, "version.preview", json!({"index":0}));
    let mut u = Ui::new(s);
    let before = u.app.session.undo_depth();
    u.frame(vec![Event::PointerMoved(centre(&u, locked))]);
    let at = badge(&u, locked);
    click(&mut u, at);
    assert!(u.app.session.ds().unwrap().preview.is_some());
    assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked);
    assert_eq!(u.app.session.undo_depth(), before);
}

#[test]
fn canvas_unlock_badge_is_a_named_accessible_button() {
    let (s, locked, _) = locked_scene();
    let mut u = Ui::new(s);
    u.ctx.enable_accesskit();
    u.frame(vec![Event::PointerMoved(centre(&u, locked))]);
    let out = u.frame_output(vec![]);
    let tree = out.platform_output.accesskit_update.unwrap();
    assert!(tree.nodes.iter().any(|(_, node)| node.role() == egui::accesskit::Role::Button && node.label() == Some("Unlock Locked rectangle")));
}

#[test]
fn canvas_unlock_regression_context_replacement_in_the_same_document_retires_the_old_target() {
    let (s, locked, other) = locked_scene();
    let mut u = Ui::new(s);
    let at = centre(&u, locked);
    context(&mut u, at);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    text_center(&shapes, "Unlock Locked rectangle");
    let row = u.row_at(other);
    context(&mut u, row);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(texts(&shapes).iter().all(|(text, _, _)| text != "Unlock Locked rectangle"), "the new layer menu cannot inherit the old locked owner");
    assert!(u.app.ui.context_menu.is_some(), "keep the new legitimate menu");
    assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked);
}

#[test]
fn canvas_unlock_regression_document_edit_occluding_the_badge_retires_its_cached_target() {
    let (s, locked, other) = locked_scene();
    let mut u = Ui::new(s);
    u.frame(vec![Event::PointerMoved(centre(&u, locked))]);
    let at = badge(&u, locked);
    u.frame(vec![Event::PointerMoved(at)]);
    u.frame(vec![]);
    let cover = node(&mut u.app.session, json!({"type":"rectangle","name":"New cover","x":100,"y":100,"width":200,"height":140}));
    ex(&mut u.app.session, "select.set", json!({"ids":[other.0]}));
    u.frame(vec![]);
    click(&mut u, at);
    assert!(u.app.session.doc().unwrap().node(locked).unwrap().locked, "the old badge cannot unlock geometry now hidden by a new foreground layer");
    assert!(!u.app.session.doc().unwrap().node(cover).unwrap().locked);
}

#[test]
fn canvas_unlock_regression_viewport_resize_retires_an_offscreen_clamped_badge() {
    let (s, locked, _) = locked_scene();
    let mut u = Ui::new(s);
    let (vw, vh) = {
        let view = &u.app.session.ds().unwrap().view;
        (view.vw, view.vh)
    };
    ex(&mut u.app.session, "view.set", json!({"zoom":1,"cx":100.0+vw/2.0+50.0,"cy":100.0+vh/2.0+50.0}));
    u.frame(vec![]);
    u.ctx.enable_accesskit();
    let visible = u.app.canvas.rect.min + vec2(70.0, 50.0);
    u.frame(vec![Event::PointerMoved(visible)]);
    let out = u.frame_output(vec![]);
    assert!(
        out.platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .any(|(_, n)| n.role() == egui::accesskit::Role::Button && n.label() == Some("Unlock Locked rectangle")),
        "the partly visible layer starts with its actual lock button"
    );
    let at = u.app.canvas.rect.min + vec2(14.0, 14.0);
    u.frame(vec![Event::PointerMoved(at)]);
    u.frame(vec![]);
    let revision = u.app.session.ds().unwrap().revision;
    let screen = egui::Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 900.0));
    u.frame_raw(egui::RawInput { screen_rect: Some(screen), ..Default::default() });
    assert_eq!(u.app.session.ds().unwrap().revision, revision, "a real viewport resize keeps document revision");
    let bounds = u.app.session.doc().unwrap().world_bounds(locked).unwrap();
    assert!(u.app.to_screen(bounds.x1, bounds.y1).x < u.app.canvas.rect.left(), "the locked owner is entirely offscreen after shrinking");
    for pressed in [true, false] {
        u.frame_raw(egui::RawInput {
            screen_rect: Some(screen),
            events: vec![
                Event::PointerMoved(at),
                Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() },
            ],
            ..Default::default()
        });
    }
    assert!(
        u.app.session.doc().unwrap().node(locked).unwrap().locked,
        "the stationary clamped badge cannot unlock an owner moved entirely offscreen by resizing"
    );
}
