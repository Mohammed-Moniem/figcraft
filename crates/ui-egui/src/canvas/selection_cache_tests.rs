//! The selection overlay's geometry and the Design panel's values are kept for the frames that
//! show the same (#1571), made again on every input they show (an edit, the selection, zoom, pan,
//! the canvas's place, the theme's overlay green), and the same as when made afresh; what they
//! don't keep (the hover and drop-target highlight) shows on the next frame.

use std::sync::Arc;

use egui::{Color32, Context, LayerId, Painter, Rect, pos2, vec2};
use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::{Value, json};

use super::overlays::{SelectionGeometry, overlays, selection_geometry};
use crate::theme::{Tokens, set_canvas_page};
use crate::{FigApp, NoServices};
use figcraft_engine::tools::{PointerEvent, PointerKind};

fn id(v: Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

/// Three rectangles (the middle one rounded, outlined along its corners) selected, and a fourth
/// that isn't, on a light page.
fn setup() -> (FigApp, Context, Vec<NodeId>) {
    let mut s = Session::new();
    let mut ids = Vec::new();
    for x in [0, 80, 160, 240] {
        ids.push(id(s.execute("shape.rectangle", &json!({"x": x, "y": 0, "width": 50, "height": 50})).unwrap()));
    }
    let radii = json!({"topLeftRadius": 12, "topRightRadius": 12, "bottomRightRadius": 12, "bottomLeftRadius": 12});
    s.execute("node.set", &json!({"id": ids[1], "props": radii})).unwrap();
    s.execute("select.set", &json!({"ids": ids[..3]})).unwrap();
    let mut app = FigApp::new(s, Box::new(NoServices));
    app.canvas.rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
    let ctx = Context::default();
    set_canvas_page(&ctx, Color32::WHITE);
    (app, ctx, ids)
}

fn geometry(app: &FigApp, ctx: &Context) -> SelectionGeometry {
    let ds = app.session.ds().unwrap();
    selection_geometry(app, &Painter::new(ctx.clone(), LayerId::background(), Rect::EVERYTHING), &ds.doc.clone(), &ds.selection.clone())
}

/// The geometry made with nothing kept, on a page of colour `page`.
fn fresh(app: &FigApp, page: Color32) -> SelectionGeometry {
    let ctx = Context::default();
    set_canvas_page(&ctx, page);
    geometry(app, &ctx)
}

fn kept(a: &SelectionGeometry, b: &SelectionGeometry) -> bool {
    Arc::ptr_eq(&a.outlines, &b.outlines)
}

#[test]
fn an_unchanged_frame_reuses_the_selection_geometry() {
    let (app, ctx, _) = setup();
    let first = geometry(&app, &ctx);
    assert_eq!(first.outlines.len(), 3);
    assert!(kept(&first, &geometry(&app, &ctx)));
    assert_eq!(first, fresh(&app, Color32::WHITE));
}

#[test]
fn a_kept_value_does_not_keep_its_document_alive() {
    let (app, ctx, _) = setup();
    let doc = app.session.ds().unwrap().doc.clone();
    let holders = Arc::strong_count(&doc);
    geometry(&app, &ctx);
    assert_eq!(Arc::strong_count(&doc), holders);
}

type Change = Box<dyn Fn(&mut FigApp, &Context)>;

#[test]
fn every_input_the_geometry_shows_makes_it_again() {
    let (mut app, ctx, ids) = setup();
    let (a, b) = (ids[0], ids[1]);
    let changes: Vec<(&str, Change)> = vec![
        (
            "an edit",
            Box::new(move |app: &mut FigApp, _: &Context| {
                app.set_props(&[b], json!({"x": 90}));
            }),
        ),
        (
            "the selection",
            Box::new(move |app: &mut FigApp, _: &Context| {
                app.session.execute("select.set", &json!({"ids": [a, b]})).unwrap();
            }),
        ),
        ("zoom", Box::new(|app: &mut FigApp, _: &Context| app.session.ds_mut().unwrap().view.zoom *= 2.0)),
        ("pan", Box::new(|app: &mut FigApp, _: &Context| app.session.ds_mut().unwrap().view.cx += 30.0)),
        ("the canvas moving", Box::new(|app: &mut FigApp, _: &Context| app.canvas.rect = app.canvas.rect.translate(vec2(40.0, 0.0)))),
        ("the theme", Box::new(|_: &mut FigApp, ctx: &Context| set_canvas_page(ctx, Color32::BLACK))),
    ];
    let mut page = Color32::WHITE;
    for (what, change) in changes {
        let before = geometry(&app, &ctx);
        change(&mut app, &ctx);
        if what == "the theme" {
            page = Color32::BLACK;
            assert_ne!(crate::theme::canvas_accent(&ctx), before.col, "a dark page takes the other green");
        }
        let after = geometry(&app, &ctx);
        assert!(!kept(&before, &after), "{what}");
        assert_ne!(before, after, "{what}");
        assert_eq!(after, fresh(&app, page), "{what}: as if nothing were kept");
        assert!(kept(&after, &geometry(&app, &ctx)), "{what}: kept again");
    }
}

/// How many shapes one frame of the canvas overlays draws.
pub(super) fn drawn(app: &FigApp, ctx: &Context) -> usize {
    let mut out = ctx.run_ui(Default::default(), |ui| {
        overlays(app, &ui.ctx().layer_painter(LayerId::background()), &Tokens::light());
    });
    out.textures_delta.clear();
    out.shapes.len()
}

fn selection_outline_opacity_colors(app: &FigApp, ctx: &Context) -> Vec<Color32> {
    let mut out = ctx.run_ui(egui::RawInput { time: Some(0.0), ..Default::default() }, |ui| {
        overlays(app, &ui.ctx().layer_painter(LayerId::background()), &Tokens::light());
    });
    out.textures_delta.clear();
    let mut colors = Vec::new();
    for clipped in ctx.tessellate(out.shapes, out.pixels_per_point) {
        if let egui::epaint::Primitive::Mesh(mesh) = clipped.primitive {
            colors.extend(mesh.vertices.into_iter().map(|v| v.color).filter(|c| c.a() > 0));
        }
    }
    colors.sort_by_key(|c| c.to_array());
    colors
}

#[test]
fn selection_outline_opacity_saved_percentage_fades_actual_cached_selection_paint() {
    let (mut app, ctx, _) = setup();
    let full = selection_outline_opacity_colors(&app, &ctx);
    assert!(full.iter().any(|c| c.a() == 255));
    let geometry_before = geometry(&app, &ctx);
    // Use a saved preference rather than a new field/API so the original baseline compiles and
    // genuinely shows its unchanged opaque selection paint before the implementation.
    app.session.prefs = serde_json::from_value(json!({"selection_outline_opacity":25.0})).unwrap();
    let faded = selection_outline_opacity_colors(&app, &ctx);
    assert!(!faded.is_empty());
    assert!(faded.iter().all(|c| c.a() <= 64), "all selected geometry, handles and labels fade; max alpha {:?}", faded.iter().map(|c| c.a()).max());
    assert!(kept(&geometry_before, &geometry(&app, &ctx)), "opacity does not invalidate or recolor cached geometry");
    app.session.prefs = serde_json::from_value(json!({"selection_outline_opacity":100.0})).unwrap();
    assert_eq!(selection_outline_opacity_colors(&app, &ctx), full, "100% restores the exact original paint");
}

#[test]
fn selection_outline_opacity_zero_hides_only_the_selection_and_default_stays_visible() {
    let (mut app, ctx, ids) = setup();
    let default = selection_outline_opacity_colors(&app, &ctx);
    app.session.prefs = serde_json::from_value(json!({"selection_outline_opacity":100.0})).unwrap();
    assert_eq!(selection_outline_opacity_colors(&app, &ctx), default, "the new default preserves paint");
    app.session.prefs = serde_json::from_value(json!({"selection_outline_opacity":0.0})).unwrap();
    assert!(selection_outline_opacity_colors(&app, &ctx).is_empty(), "zero opacity paints no normal selection chrome");
    assert_eq!(app.session.selection(), ids[..3]);
    app.session.ds_mut().unwrap().hover = Some(ids[3]);
    let hover = selection_outline_opacity_colors(&app, &ctx);
    assert!(hover.iter().any(|c| c.a() == 255), "ordinary unselected hover remains visible");
}

#[test]
fn selection_outline_opacity_default_keeps_the_existing_visibility_toggle() {
    let (mut app, ctx, _) = setup();
    let full = selection_outline_opacity_colors(&app, &ctx);
    app.session.prefs = serde_json::from_value(json!({"selection_outline_opacity":100.0})).unwrap();
    assert_eq!(selection_outline_opacity_colors(&app, &ctx), full);
    crate::menus::invoke(&mut app, "view.selectionOutlines");
    assert!(selection_outline_opacity_colors(&app, &ctx).is_empty());
    crate::menus::invoke(&mut app, "view.selectionOutlines");
    assert_eq!(selection_outline_opacity_colors(&app, &ctx), full);
}

#[test]
fn selection_outline_opacity_does_not_fade_text_carets_or_vector_editing_points() {
    for vector in [false, true] {
        let mut s = Session::new();
        if vector {
            let path = s.execute("vector.create", &json!({"points":[[100,100],[200,100],[300,100]],"closed":false})).unwrap()["id"].clone();
            s.execute("vector.edit.begin", &json!({"id":path})).unwrap();
            s.execute("vector.edit.select", &json!({"vertices":[1]})).unwrap();
        } else {
            let text = s.execute("node.create", &json!({"type":"text","text":"Editable","x":0,"y":0})).unwrap()["id"].clone();
            s.execute("text.edit.begin", &json!({"id":text,"caret":3})).unwrap();
        }
        let mut app = FigApp::new(s, Box::new(NoServices));
        app.canvas.rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let ctx = Context::default();
        set_canvas_page(&ctx, Color32::WHITE);
        let full = selection_outline_opacity_colors(&app, &ctx);
        assert!(!full.is_empty());
        app.session.prefs = serde_json::from_value(json!({"selection_outline_opacity":0.0})).unwrap();
        assert_eq!(selection_outline_opacity_colors(&app, &ctx), full, "active editing aids are unaffected (vector={vector})");
    }
}

#[test]
fn the_hover_and_drop_highlights_show_over_kept_geometry() {
    let (mut app, ctx, ids) = setup();
    let other = ids[3];
    let plain = drawn(&app, &ctx);
    let first = geometry(&app, &ctx);
    app.session.ds_mut().unwrap().hover = Some(other);
    assert_eq!(drawn(&app, &ctx), plain + 1, "the hovered layer's outline");
    assert!(kept(&first, &geometry(&app, &ctx)), "the hover isn't part of the selection's geometry");
    app.session.ds_mut().unwrap().hover = None;
    app.session.tool.overlay.drop_target = Some(other);
    assert_eq!(drawn(&app, &ctx), plain + 1, "the drop target's outline");
    app.session.tool.overlay.drop_target = None;
    assert_eq!(drawn(&app, &ctx), plain);
}

#[test]
fn the_selection_outlines_menu_hides_chrome_while_properties_and_hit_tests_keep_working() {
    let (mut app, ctx, ids) = setup();
    let selected = app.session.selection();
    let document = app.session.ds().unwrap().doc.clone();
    let undo = app.session.ds().unwrap().history.undo.len();
    let visible = drawn(&app, &ctx);
    crate::menus::invoke(&mut app, "view.selectionOutlines");
    let hidden = drawn(&app, &ctx);
    assert!(hidden < visible, "hiding selection outlines removes its outlines, transform handles and size label");
    assert_eq!(app.session.selection(), selected);
    assert!(Arc::ptr_eq(&document, &app.session.ds().unwrap().doc));
    assert_eq!(app.session.ds().unwrap().history.undo.len(), undo);
    crate::menus::invoke(&mut app, "view.selectionOutlines");
    assert_eq!(drawn(&app, &ctx), visible, "the same menu action restores the overlay immediately");
    crate::menus::invoke(&mut app, "view.selectionOutlines");

    // Changing a visual property while the overlay is hidden still edits the selected objects,
    // and Undo restores the property without switching the overlay back on.
    app.set_props(&[ids[1]], json!({"topLeftRadius": 20}));
    assert_eq!(app.session.doc().unwrap().node(ids[1]).unwrap().corners.radii[0], 20.0);
    assert_eq!(app.session.selection(), selected);
    assert_eq!(drawn(&app, &ctx), hidden);
    app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(app.session.doc().unwrap().node(ids[1]).unwrap().corners.radii[0], 12.0);
    assert!(!app.session.ds().unwrap().view.selection_outlines);

    // A hidden outline changes only the paint, not the layer or transform hit geometry.
    app.session.execute("select.none", &json!({})).unwrap();
    let ev = |kind, x, y| PointerEvent { cmd: true, ..PointerEvent::at(kind, x, y, 1.0) };
    app.session.pointer(&ev(PointerKind::Down, 250.0, 10.0)).unwrap();
    app.session.pointer(&ev(PointerKind::Move, 270.0, 30.0)).unwrap();
    app.session.pointer(&ev(PointerKind::Up, 270.0, 30.0)).unwrap();
    assert_eq!(app.session.selection(), vec![ids[3]]);
    let moved = app.session.doc().unwrap().node(ids[3]).unwrap();
    assert!((moved.transform.translation().x - 260.0).abs() < 1e-6);
    assert!((moved.transform.translation().y - 20.0).abs() < 1e-6);
    assert!(!app.session.ds().unwrap().view.selection_outlines);
}

#[test]
fn the_design_panels_instance_facts_follow_edits_and_the_selection() {
    let mut s = Session::new();
    let f = id(s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 100, "height": 100})).unwrap());
    id(s.execute("node.create", &json!({"type": "rectangle", "x": 10, "y": 10, "width": 20, "height": 20, "parent": f})).unwrap());
    let main = id(s.execute("component.create", &json!({"ids": [f]})).unwrap());
    let inst = id(s.execute("instance.create", &json!({"component": main, "x": 300, "y": 0})).unwrap());
    let child = s.doc().unwrap().node(inst).unwrap().children[0];
    let ctx = Context::default();
    let facts = |s: &Session| {
        let ds = s.ds().unwrap();
        crate::right::in_instance(&ctx, &ds.doc.clone(), &ds.selection.clone())
    };
    let facts_of = |s: &mut Session, ids: Value| {
        s.execute("select.set", &json!({"ids": ids})).unwrap();
        let f = facts(s);
        (f.any, f.nested)
    };
    assert_eq!(facts_of(&mut s, json!([main])), (false, false));
    assert_eq!(facts_of(&mut s, json!([inst])), (true, false));
    assert_eq!(facts_of(&mut s, json!([inst, child])), (true, true));
    assert_eq!(facts_of(&mut s, json!([inst])), (true, false));
    s.execute("component.detach", &json!({"ids": [inst]})).unwrap();
    assert!(s.doc().unwrap().node(inst).is_some(), "detaching keeps the layer");
    assert_eq!(s.ds().unwrap().selection, vec![inst], "and the selection");
    assert_eq!(facts(&s), crate::right::InInstance { any: false, nested: false }, "an edit");
}

#[test]
fn the_position_fields_follow_edits_and_the_selection() {
    let (mut app, ctx, ids) = setup();
    let x = |app: &FigApp| {
        let ds = app.session.ds().unwrap();
        crate::right::position_values(&ctx, &ds.doc.clone(), &ds.selection.clone()).x
    };
    assert_eq!(x(&app), None, "mixed");
    app.session.execute("select.set", &json!({"ids": [ids[1]]})).unwrap();
    assert_eq!(x(&app), Some(80.0), "the selection");
    app.set_props(&[ids[1]], json!({"x": 90}));
    assert_eq!(x(&app), Some(90.0), "an edit");
}

/// The visual box of every shape one frame of the canvas overlays draws, in drawing order.
fn boxes(app: &FigApp, ctx: &Context) -> Vec<Rect> {
    let mut out = ctx.run_ui(Default::default(), |ui| {
        overlays(app, &ui.ctx().layer_painter(LayerId::background()), &Tokens::light());
    });
    out.textures_delta.clear();
    out.shapes.iter().map(|s| s.shape.visual_bounding_rect()).collect()
}

/// A drag of the selection held at (`x`, `y`) on the page, from (10, 10); snapping is off (Cmd).
fn drag_to(app: &mut FigApp, x: f64, y: f64) {
    let ev = |kind, x, y| PointerEvent { cmd: true, ..PointerEvent::at(kind, x, y, 1.0) };
    if app.session.ds().unwrap().interaction.is_none() {
        app.session.pointer(&ev(PointerKind::Down, 10.0, 10.0)).unwrap();
    }
    app.session.pointer(&ev(PointerKind::Move, x, y)).unwrap();
}

#[test]
fn a_move_shifts_the_geometry_kept_from_where_it_started() {
    let (mut app, ctx, _) = setup();
    let start = app.session.ds().unwrap().doc.clone();
    drag_to(&mut app, 47.0, 71.0);
    assert_eq!(app.session.ds().unwrap().interaction.as_ref().and_then(|i| i.moved_by), Some((37.0, 61.0)));
    let moving = boxes(&app, &ctx);
    let first = selection_geometry(&app, &Painter::new(ctx.clone(), LayerId::background(), Rect::EVERYTHING), &start, &app.session.selection());
    drag_to(&mut app, 57.0, 91.0);
    let again = boxes(&app, &ctx);
    let kept_since = selection_geometry(&app, &Painter::new(ctx.clone(), LayerId::background(), Rect::EVERYTHING), &start, &app.session.selection());
    assert!(kept(&first, &kept_since), "the geometry where the move started is kept for it");
    // What shows is what the moved document shows when nothing is kept, to a hundredth of a point.
    let mut fresh = FigApp::new(Session::new(), Box::new(NoServices));
    fresh.canvas.rect = app.canvas.rect;
    {
        let (ds, now) = (fresh.session.ds_mut().unwrap(), app.session.ds().unwrap());
        (ds.doc, ds.selection, ds.view) = (now.doc.clone(), now.selection.clone(), now.view.clone());
    }
    let ctx2 = Context::default();
    set_canvas_page(&ctx2, Color32::WHITE);
    let want = boxes(&fresh, &ctx2);
    assert_eq!(again.len(), want.len());
    for (got, want) in again.iter().zip(&want) {
        assert!((got.min - want.min).length() < 0.01 && (got.max - want.max).length() < 0.01, "{got:?} != {want:?}");
    }
    assert_ne!(moving, again, "the second move shows further on");
}

#[test]
fn the_gesture_start_stands_in_only_while_the_selection_is_only_moved() {
    let (mut app, _, _) = setup();
    let start = app.session.ds().unwrap().doc.clone();
    assert!(crate::selection_cache::moved(app.session.ds().unwrap()).is_none(), "no gesture");
    drag_to(&mut app, 30.0, 10.0);
    let (base, d) = crate::selection_cache::moved(app.session.ds().unwrap()).expect("a plain move");
    assert!(Arc::ptr_eq(&base, &start) && d == (20.0, 0.0));
    // The selection changing under the gesture: the start no longer stands in.
    app.session.ds_mut().unwrap().selection.pop();
    assert!(crate::selection_cache::moved(app.session.ds().unwrap()).is_none());
}
