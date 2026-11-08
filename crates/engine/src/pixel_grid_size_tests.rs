//! Custom pixel-grid sizes are a saved view preference, independent of artwork and snapping toggles.
use crate::Session;
use crate::params::id_of;
use crate::tools::{PointerEvent, PointerKind};
use figcraft_geom::Point;
use serde_json::{Value, json};

fn ex(s: &mut Session, command: &str, params: Value) -> Value {
    s.execute(command, &params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

#[test]
fn custom_pixel_grid_size_is_saved_for_open_and_future_tabs_without_a_document_edit() {
    let mut s = Session::new();
    let made = ex(&mut s, "shape.rectangle", json!({"width":20,"height":20}));
    ex(&mut s, "select.set", json!({"ids":[made["id"]]}));
    ex(&mut s, "file.new", json!({}));
    s.active = 0;
    let before = s.doc().unwrap().clone();
    let selection = s.selection();
    let undo = s.undo_depth();
    assert_eq!(ex(&mut s, "pref.pixelGridSize", json!({"size":8}))["size"], 8.0);
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.selection(), selection);
    assert_eq!(s.undo_depth(), undo);
    assert_eq!(serde_json::to_value(&s.prefs).unwrap()["pixel_grid_size"], 8.0);
    for tab in &s.docs {
        assert_eq!(serde_json::to_value(&tab.view).unwrap()["pixel_grid_size"], 8.0);
    }
    ex(&mut s, "file.new", json!({}));
    assert_eq!(serde_json::to_value(&s.ds().unwrap().view).unwrap()["pixel_grid_size"], 8.0);
    assert_eq!(ex(&mut s, "pref.pixelGridSize", json!({}))["size"], 8.0);
    let prefs = serde_json::to_value(&s.prefs).unwrap();
    for bad in [json!(0), json!(-1), json!(0.01), json!(10001), json!("8"), json!({})] {
        assert!(s.execute("pref.pixelGridSize", &json!({"size":bad})).is_err());
        assert_eq!(serde_json::to_value(&s.prefs).unwrap(), prefs);
    }
    assert_eq!(ex(&mut s, "pref.pixelGridSize", json!({"reset":true}))["size"], 1.0);
}

#[test]
fn custom_pixel_grid_visibility_tracks_cell_screen_size_and_default_stays_at_four_hundred_percent() {
    for (size, zoom, shown) in [(1.0, 3.99, false), (1.0, 4.0, true), (8.0, 0.49, false), (8.0, 0.5, true), (0.125, 31.0, false), (0.125, 32.0, true)]
    {
        let view: crate::ViewState = serde_json::from_value(json!({"zoom":zoom,"pixel_grid_size":size})).unwrap();
        assert_eq!(view.pixel_grid_shown(), shown, "size={size}, zoom={zoom}");
    }
}

#[test]
fn custom_pixel_grid_unit_grid_keeps_fractional_object_snap_on_the_held_axis() {
    let mut s = Session::new();
    let _ = ex(&mut s, "shape.rectangle", json!({"x":0,"y":0,"width":11,"height":11}));
    let b = id_of(&ex(&mut s, "shape.rectangle", json!({"x":200,"y":300,"width":100,"height":100}))["id"]).unwrap();
    ex(&mut s, "select.set", json!({"ids":[b]}));
    for (kind, x, y) in [(PointerKind::Down, 250.0, 350.0), (PointerKind::Move, 6.0, 352.4), (PointerKind::Up, 6.0, 352.4)] {
        s.pointer(&PointerEvent::at(kind, x, y, 1.0)).unwrap();
    }
    assert_eq!(
        s.doc().unwrap().world_bounds(b).unwrap().origin(),
        Point::new(-44.5, 302.0),
        "object-snapped centre stays at5.5; free Y stays pixel-aligned"
    );
}

fn drag(s: &mut Session, from: Point, to: Point) {
    for (kind, p) in [(PointerKind::Down, from), (PointerKind::Move, to), (PointerKind::Up, to)] {
        s.pointer(&PointerEvent::at(kind, p.x, p.y, 1.0)).unwrap();
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

#[test]
fn custom_pixel_grid_saved_size_is_lenient_but_keeps_other_preferences() {
    for bad in [json!(null), json!("bad"), json!(0), json!(-1), json!(10001), json!(true), json!({})] {
        let prefs: crate::Prefs = serde_json::from_value(json!({"pixel_grid_size":bad,"nudge_small":2.5,"theme":"light"})).unwrap();
        assert_eq!(prefs.pixel_grid_size, 1.0);
        assert_eq!(prefs.nudge_small, 2.5);
        assert_eq!(prefs.theme, "light");
    }
    assert_eq!(serde_json::from_value::<crate::Prefs>(json!({})).unwrap().pixel_grid_size, 1.0);
    let mut s = Session::new();
    ex(&mut s, "pref.pixelGridSize", json!({"size":0.125}));
    let kept: crate::Prefs = serde_json::from_value(serde_json::to_value(&s.prefs).unwrap()).unwrap();
    let mut restored = Session::new();
    restored.prefs = kept;
    restored.apply_view_prefs();
    assert_eq!(restored.ds().unwrap().view.pixel_grid_step(), 0.125);
    restored.prefs_unsaved = false;
    ex(&mut restored, "pref.pixelGridSize", json!({}));
    assert!(!restored.prefs_unsaved, "query does not request a save");
    assert!(restored.execute("pref.pixelGridSize", &json!({"size":8,"reset":true})).is_err());
}

#[test]
fn custom_pixel_grid_large_cells_hold_exact_object_axes_for_move_resize_and_draw() {
    let mut s = Session::new();
    ex(&mut s, "pref.pixelGridSize", json!({"size":16}));
    ex(&mut s, "shape.rectangle", json!({"x":200.7,"y":0,"width":100,"height":100}));
    let b = id_of(&ex(&mut s, "shape.rectangle", json!({"x":0,"y":300,"width":100,"height":100}))["id"]).unwrap();
    ex(&mut s, "select.set", json!({"ids":[b]}));
    drag(&mut s, Point::new(50.0, 350.0), Point::new(250.7, 352.4));
    let r = s.doc().unwrap().world_bounds(b).unwrap();
    close(r.x0, 200.7);
    close(r.y0, 304.0);
    ex(&mut s, "edit.undo", json!({}));
    drag(&mut s, Point::new(100.0, 400.0), Point::new(200.7, 453.2));
    let r = s.doc().unwrap().world_bounds(b).unwrap();
    close(r.x1, 200.7);
    close(r.y1, 448.0);
    ex(&mut s, "edit.undo", json!({}));
    s.set_tool(crate::tools::ToolId::Rectangle);
    drag(&mut s, Point::new(20.3, 500.5), Point::new(200.7, 601.2));
    let id = *s.selection().first().unwrap();
    let r = s.doc().unwrap().world_bounds(id).unwrap();
    close(r.x0, 16.0);
    close(r.y0, 496.0);
    close(r.x1, 200.7);
    close(r.y1, 608.0);
}

#[test]
fn custom_pixel_grid_snap_toggle_is_independent_of_visibility_and_cell_size() {
    for pixel in [false, true] {
        let mut s = Session::new();
        ex(&mut s, "pref.pixelGridSize", json!({"size":16}));
        ex(&mut s, "view.snapObjects", json!({"on":false}));
        ex(&mut s, "view.snapPixel", json!({"on":pixel}));
        let b = id_of(&ex(&mut s, "shape.rectangle", json!({"x":0,"y":300,"width":100,"height":100}))["id"]).unwrap();
        ex(&mut s, "select.set", json!({"ids":[b]}));
        drag(&mut s, Point::new(50.0, 350.0), Point::new(97.3, 398.7));
        let r = s.doc().unwrap().world_bounds(b).unwrap();
        close(r.x0, if pixel { 48.0 } else { 47.3 });
        close(r.y0, if pixel { 352.0 } else { 348.7 });
        assert!(s.ds().unwrap().view.pixel_grid_shown(), "visibility is independent");
    }
}

#[test]
fn custom_pixel_grid_pen_pointer_points_follow_the_selected_size_and_snap_toggle() {
    for pixel in [false, true] {
        let mut s = Session::new();
        ex(&mut s, "pref.pixelGridSize", json!({"size":16}));
        ex(&mut s, "view.snapObjects", json!({"on":false}));
        ex(&mut s, "view.snapPixel", json!({"on":pixel}));
        s.set_tool(crate::tools::ToolId::Pen);
        for p in [Point::new(21.2, 19.3), Point::new(69.1, 51.3)] {
            for kind in [PointerKind::Down, PointerKind::Up] {
                s.pointer(&PointerEvent::at(kind, p.x, p.y, 1.0)).unwrap();
            }
        }
        let id = s.tool.pen.as_ref().and_then(|p| p.id).unwrap();
        s.pen_end().unwrap();
        let doc = s.doc().unwrap();
        let w = doc.world_transform(id);
        let figcraft_doc::NodeKind::Vector { network } = &doc.node(id).unwrap().kind else { panic!("pen vector") };
        let points: Vec<_> = network.vertices.iter().map(|v| w * Point::new(v.x, v.y)).collect();
        let expected = if pixel { [Point::new(16.0, 16.0), Point::new(64.0, 48.0)] } else { [Point::new(21.2, 19.3), Point::new(69.1, 51.3)] };
        assert_eq!(points.len(), 2);
        for (a, b) in points.iter().zip(expected) {
            close(a.x, b.x);
            close(a.y, b.y);
        }
    }
}

#[test]
fn custom_pixel_grid_moves_use_a_rotated_parent_frames_own_axes() {
    let mut s = Session::new();
    ex(&mut s, "pref.pixelGridSize", json!({"size":16}));
    ex(&mut s, "view.snapObjects", json!({"on":false}));
    let frame = id_of(&ex(&mut s, "frame.create", json!({"width":300,"height":300}))["id"]).unwrap();
    ex(&mut s, "node.set", json!({"id":frame,"props":{"rotation":30}}));
    let child = id_of(&ex(&mut s, "shape.rectangle", json!({"parent":frame,"x":10,"y":10,"width":20,"height":20}))["id"]).unwrap();
    ex(&mut s, "select.set", json!({"ids":[child]}));
    let w = s.doc().unwrap().world_transform(frame);
    drag(&mut s, w * Point::new(20.0, 20.0), w * Point::new(43.4, 38.9));
    let [_, _, _, _, x, y] = s.doc().unwrap().node(child).unwrap().transform.as_coeffs();
    close(x, 32.0);
    close(y, 32.0);
}

fn shift_grid_move(rotated: bool, horizontal: bool) {
    let mut s = Session::new();
    ex(&mut s, "pref.pixelGridSize", json!({"size":16}));
    ex(&mut s, "view.snapObjects", json!({"on":false}));
    let parent = if rotated {
        let id = id_of(&ex(&mut s, "frame.create", json!({"width":300,"height":300}))["id"]).unwrap();
        ex(&mut s, "node.set", json!({"id":id,"props":{"rotation":30}}));
        Some(id)
    } else {
        None
    };
    let mut params = json!({"x":10,"y":10,"width":20,"height":20});
    if let Some(parent) = parent {
        params["parent"] = json!(parent);
    }
    let id = id_of(&ex(&mut s, "shape.rectangle", params)["id"]).unwrap();
    ex(&mut s, "select.set", json!({"ids":[id]}));
    let before = s.doc().unwrap().world_bounds(id).unwrap();
    let from = before.center();
    let to = if horizontal { Point::new(from.x + 35.3, from.y) } else { Point::new(from.x, from.y + 35.3) };
    for (kind, p) in [(PointerKind::Down, from), (PointerKind::Move, to), (PointerKind::Up, to)] {
        let mut event = PointerEvent::at(kind, p.x, p.y, 1.0);
        event.shift = kind != PointerKind::Down;
        s.pointer(&event).unwrap();
    }
    let after = s.doc().unwrap().world_bounds(id).unwrap();
    if horizontal {
        close(after.y0, before.y0);
        assert!(after.x0 > before.x0 + 20.0);
    } else {
        close(after.x0, before.x0);
        assert!(after.y0 > before.y0 + 20.0);
    }
}

#[test]
fn custom_pixel_grid_regression_shift_horizontal_preserves_the_page_y_axis() {
    shift_grid_move(false, true);
}
#[test]
fn custom_pixel_grid_regression_shift_vertical_preserves_the_page_x_axis() {
    shift_grid_move(false, false);
}
#[test]
fn custom_pixel_grid_regression_shift_in_a_rotated_parent_preserves_the_page_axis() {
    shift_grid_move(true, true);
    shift_grid_move(true, false);
}
#[test]
fn custom_pixel_grid_regression_precision_boundary_never_emits_duplicate_grid_lines() {
    let boundary = 9_007_199_254_740_992.0;
    let lines = crate::pixel_grid::positions(boundary - 1.0, boundary + 2.0, 1.0);
    assert!(lines.windows(2).all(|pair| pair[0] < pair[1]), "{lines:?}");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn custom_pixel_grid_saved_preference_reaches_actual_restored_windows_without_changing_saved_views() {
    let dir = std::env::temp_dir().join(format!("figcraft-grid-restore-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("first.figcraft");
    let b = dir.join("second.figcraft");
    let mut writer = Session::empty();
    for (name, path) in [("First", &a), ("Second", &b)] {
        ex(&mut writer, "file.new", json!({"name":name}));
        ex(&mut writer, "file.saveAs", json!({"path":path}));
    }
    let mut restored = Session::empty();
    restored.prefs = serde_json::from_value(json!({"pixel_grid_size":16,"session":{"windows":[
        {"tabs":[{"path":a,"view":[12,24,0.25]}]}, {"tabs":[{"path":b,"view":[32,48,2]}]}
    ]}}))
    .unwrap();
    let out = ex(&mut restored, "session.restore", json!({}));
    assert_eq!(out["windows"], 2);
    assert_eq!(out["opened"], 2);
    assert_eq!(out["missing"], json!([]));
    for d in &restored.docs {
        assert_eq!(d.view.pixel_grid_step(), 16.0);
        assert!(!d.is_dirty());
    }
    assert_eq!((restored.docs[0].view.cx, restored.docs[0].view.cy, restored.docs[0].view.zoom), (12.0, 24.0, 0.25));
    assert_eq!((restored.docs[1].view.cx, restored.docs[1].view.cy, restored.docs[1].view.zoom), (32.0, 48.0, 2.0));
    let (placed, missing) = restored.restore.placing.take().unwrap();
    assert_eq!(missing, 0);
    assert_eq!(placed.len(), 2);
    assert!(placed.iter().all(|w| w.docs.len() == 1));
    ex(&mut restored, "pref.pixelGridSize", json!({"size":8}));
    assert!(restored.docs.iter().all(|d| d.view.pixel_grid_step() == 8.0));
    let _ = std::fs::remove_dir_all(dir);
}
