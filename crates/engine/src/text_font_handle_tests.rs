//! Original #1563 pointer contract; no incumbent assets or implementation.
use crate::{
    Session,
    params::id_of,
    tools::{PointerEvent, PointerKind},
};
use figcraft_doc::{LineHeight, NodeId, TextData};
use figcraft_geom::Point;
use serde_json::json;

fn scene(line: serde_json::Value) -> (Session, NodeId) {
    let mut s = Session::new();
    let out = s.execute("text.create", &json!({"text":"Original size handle\nTwo lines","x":100,"y":100,"width":240,"props":{"height":80,"fontFamily":"Inter","fontSize":20,"lineHeight":line,"textAutoResize":"none"}})).unwrap();
    (s, id_of(&out["id"]).unwrap())
}
fn data(s: &Session, id: NodeId) -> &TextData {
    s.doc().unwrap().node(id).unwrap().kind.text().unwrap()
}
fn point(s: &Session, id: NodeId, x: f64, y: f64) -> Point {
    s.doc().unwrap().world_transform(id) * Point::new(x, y)
}
fn pointer(s: &mut Session, kind: PointerKind, p: Point) {
    s.pointer(&PointerEvent::at(kind, p.x, p.y, 1.0)).unwrap();
}
fn drag(s: &mut Session, a: Point, b: Point) {
    pointer(s, PointerKind::Down, a);
    pointer(s, PointerKind::Move, b);
    pointer(s, PointerKind::Up, b);
}

#[test]
fn bottom_center_text_handle_changes_font_size_and_scales_pixel_line_height() {
    for (dy, font, line) in [(40.0, 30.0, 45.0), (-40.0, 10.0, 15.0)] {
        let (mut s, id) = scene(json!(30));
        let before = s.doc().unwrap().node(id).unwrap().clone();
        let a = point(&s, id, 120.0, 80.0);
        let b = point(&s, id, 120.0, 80.0 + dy);
        drag(&mut s, a, b);
        assert_eq!(data(&s, id).style.font_size, font, "bottom-center circle changes font size, not box height");
        assert_eq!(data(&s, id).style.line_height, LineHeight::Pixels(line));
        let after = s.doc().unwrap().node(id).unwrap();
        assert_eq!((after.width, after.height), (before.width, before.height));
        assert_eq!(after.transform, before.transform);
        assert_eq!(data(&s, id).characters, before.kind.text().unwrap().characters);
    }
}
#[test]
fn text_font_handle_keeps_auto_and_percent_line_height_units() {
    for (line, expected) in [(json!("auto"), LineHeight::Auto), (json!("150%"), LineHeight::Percent(150.0))] {
        let (mut s, id) = scene(line);
        let a = point(&s, id, 120.0, 80.0);
        let b = point(&s, id, 120.0, 120.0);
        drag(&mut s, a, b);
        assert_eq!(data(&s, id).style.font_size, 30.0);
        assert_eq!(data(&s, id).style.line_height, expected);
        assert!((expected.resolve(30.0, 1.2) / expected.resolve(20.0, 1.2) - 1.5).abs() < 1e-9);
    }
}
#[test]
fn ordinary_text_bottom_edge_away_from_center_keeps_box_resize_semantics() {
    let (mut s, id) = scene(json!(30));
    let a = point(&s, id, 40.0, 80.0);
    let b = point(&s, id, 40.0, 120.0);
    drag(&mut s, a, b);
    assert_eq!(data(&s, id).style.font_size, 20.0);
    assert_eq!(data(&s, id).style.line_height, LineHeight::Pixels(30.0));
    assert_eq!(s.doc().unwrap().node(id).unwrap().height, 120.0);
}

#[test]
fn text_font_handle_keeps_rich_run_proportions_and_unrelated_properties() {
    let (mut s, id) = scene(json!(30));
    s.execute("text.style", &json!({"id":id,"start":0,"end":8,"fontSize":40,"lineHeight":60,"letterSpacing":2,"textFills":["#FF7700"]})).unwrap();
    let before = s.doc().unwrap().node(id).unwrap().clone();
    let old = before.kind.text().unwrap();
    let a = point(&s, id, 120.0, 80.0);
    let b = point(&s, id, 120.0, 160.0);
    drag(&mut s, a, b);
    let after = s.doc().unwrap().node(id).unwrap();
    let now = data(&s, id);
    assert_eq!(now.characters, old.characters);
    assert_eq!(now.runs.len(), old.runs.len());
    for (a, b) in
        std::iter::once(&old.style).chain(old.runs.iter().map(|r| &r.style)).zip(std::iter::once(&now.style).chain(now.runs.iter().map(|r| &r.style)))
    {
        let mut expected = a.clone();
        expected.font_size *= 2.0;
        if let LineHeight::Pixels(v) = &mut expected.line_height {
            *v *= 2.0;
        }
        assert_eq!(b, &expected, "only font size and absolute line height scale");
    }
    for (a, b) in old.runs.iter().zip(&now.runs) {
        assert_eq!((a.start, a.end), (b.start, b.end));
    }
    assert_eq!(after.transform, before.transform);
    assert_eq!(after.fills, before.fills);
    assert_eq!(after.strokes, before.strokes);
    assert_eq!(now.paragraph_spacing, old.paragraph_spacing);
    assert_eq!(now.paragraph_indent, old.paragraph_indent);
}
#[test]
fn a_text_font_drag_is_one_undo_and_escape_restores_the_exact_document() {
    let (mut s, id) = scene(json!(30));
    let original = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let a = point(&s, id, 120.0, 80.0);
    pointer(&mut s, PointerKind::Down, a);
    pointer(&mut s, PointerKind::Move, a + figcraft_geom::Vec2::new(0.0, 20.0));
    pointer(&mut s, PointerKind::Move, a + figcraft_geom::Vec2::new(0.0, 40.0));
    pointer(&mut s, PointerKind::Up, a + figcraft_geom::Vec2::new(0.0, 40.0));
    assert_eq!(data(&s, id).style.font_size, 30.0);
    assert_eq!(s.undo_depth().0, history.0 + 1);
    let resized = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &original);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &resized);
    s.undo().unwrap();
    pointer(&mut s, PointerKind::Down, a);
    pointer(&mut s, PointerKind::Move, a + figcraft_geom::Vec2::new(0.0, 40.0));
    s.escape().unwrap();
    pointer(&mut s, PointerKind::Up, a + figcraft_geom::Vec2::new(0.0, 40.0));
    assert_eq!(s.doc().unwrap(), &original);
    assert_eq!(s.undo_depth().0, history.0);
    assert!(!s.tool.busy());
}
#[test]
fn a_text_handle_click_or_tiny_move_is_not_an_edit() {
    let (mut s, id) = scene(json!(30));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let a = point(&s, id, 120.0, 80.0);
    pointer(&mut s, PointerKind::Down, a);
    pointer(&mut s, PointerKind::Move, a + figcraft_geom::Vec2::new(0.0, 2.0));
    pointer(&mut s, PointerKind::Up, a);
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), history);
}
#[test]
fn text_font_handle_tracks_the_rotated_local_axis_and_zoom() {
    for (rotation, zoom) in [(30.0, 0.5), (135.0, 2.0), (270.0, 1.0)] {
        let (mut s, id) = scene(json!(30));
        s.execute("node.set", &json!({"id":id,"props":{"rotation":rotation}})).unwrap();
        s.ds_mut().unwrap().view.zoom = zoom;
        let a = point(&s, id, 120.0, 80.0);
        let b = point(&s, id, 120.0, 120.0);
        let handle = s.text_font_handle().unwrap();
        assert!((Point::new(handle.at[0], handle.at[1]) - a).hypot() < 1e-6);
        for (kind, p) in [(PointerKind::Down, a), (PointerKind::Move, b), (PointerKind::Up, b)] {
            s.pointer(&PointerEvent::at(kind, p.x, p.y, zoom)).unwrap();
        }
        assert!((data(&s, id).style.font_size - 30.0).abs() < 1e-6, "rotation{rotation}/zoom{zoom}");
    }
}
#[test]
fn text_font_handle_hides_for_unsupported_selections_and_modes() {
    let (mut s, id) = scene(json!(30));
    assert!(s.text_font_handle().is_some());
    s.tool.dev_mode = true;
    assert!(s.text_font_handle().is_none());
    s.tool.dev_mode = false;
    s.set_tool(crate::tools::ToolId::Scale);
    assert!(s.text_font_handle().is_none());
    s.set_tool(crate::tools::ToolId::Move);
    s.execute("text.edit.begin", &json!({"id":id})).unwrap();
    assert!(s.text_font_handle().is_none());
    s.execute("text.edit.end", &json!({})).unwrap();
    s.execute("node.set", &json!({"id":id,"props":{"locked":true}})).unwrap();
    assert!(s.text_font_handle().is_none());
    s.execute("node.set", &json!({"id":id,"props":{"locked":false,"width":20}})).unwrap();
    assert!(s.text_font_handle().is_none());
    s.execute("node.set", &json!({"id":id,"props":{"width":240,"visible":false}})).unwrap();
    assert!(s.text_font_handle().is_none());
    s.execute("node.set", &json!({"id":id,"props":{"visible":true}})).unwrap();
    let r = id_of(&s.execute("shape.rectangle", &json!({"x":400,"y":100,"width":100,"height":100})).unwrap()["id"]).unwrap();
    assert!(s.text_font_handle().is_none());
    s.execute("select.set", &json!({"ids":[id,r]})).unwrap();
    assert!(s.text_font_handle().is_none());
}
#[test]
fn text_font_size_clamps_all_rich_sizes_without_changing_their_ratio() {
    for (dy, expected) in [(1e8, (2500.0, 5000.0)), (-1e8, (1.0, 2.0))] {
        let (mut s, id) = scene(json!(30));
        s.execute("text.style", &json!({"id":id,"start":0,"end":8,"fontSize":40})).unwrap();
        let a = point(&s, id, 120.0, 80.0);
        let b = point(&s, id, 120.0, 80.0 + dy);
        drag(&mut s, a, b);
        assert_eq!(data(&s, id).style.font_size, expected.0);
        assert_eq!(data(&s, id).runs[0].style.font_size, expected.1);
    }
}
#[test]
fn modifier_border_drags_and_scale_tool_keep_their_existing_meaning() {
    for which in 0..4 {
        let (mut s, id) = scene(json!(30));
        let a = point(&s, id, 120.0, 80.0);
        let b = point(&s, id, 120.0, 120.0);
        for (kind, p) in [(PointerKind::Down, a), (PointerKind::Move, b), (PointerKind::Up, b)] {
            let mut e = PointerEvent::at(kind, p.x, p.y, 1.0);
            match which {
                0 => e.shift = true,
                1 => e.alt = true,
                2 => e.cmd = true,
                _ => e.ctrl = true,
            };
            s.pointer(&e).unwrap();
        }
        assert_eq!(data(&s, id).style.font_size, 20.0, "modifier{which} keeps the ordinary box resize");
    }
    let (mut s, id) = scene(json!(30));
    s.set_tool(crate::tools::ToolId::Scale);
    // A centered Scale drag doubles the whole box here.
    s.tool.scale_anchor = (1, 1);
    let a = point(&s, id, 120.0, 80.0);
    let b = point(&s, id, 120.0, 120.0);
    drag(&mut s, a, b);
    assert_eq!(data(&s, id).style.font_size, 40.0);
    assert_eq!(s.doc().unwrap().node(id).unwrap().height, 160.0);
}
#[test]
fn resized_typography_round_trips_in_a_native_archive() {
    use base64::Engine as _;
    let keep = |s: &mut Session, id: NodeId, name: &str| {
        let Some(dir) = std::env::var_os("FIGCRAFT_TEXT_FONT_HANDLE_EVIDENCE_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = s.execute("file.export", &json!({"ids":[id],"format":"png"})).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(out["data"].as_str().unwrap()).unwrap();
        std::fs::write(dir.join(format!("{name}.png")), bytes).unwrap();
        std::fs::write(dir.join(format!("{name}.figcraft")), figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap()).unwrap();
    };
    let (mut s, id) = scene(json!(30));
    keep(&mut s, id, "before");
    let a = point(&s, id, 120.0, 80.0);
    let b = point(&s, id, 120.0, 120.0);
    drag(&mut s, a, b);
    keep(&mut s, id, "after");
    let (loaded, _) = figcraft_format::load_bytes(&figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap()).unwrap();
    assert_eq!(loaded.node(id), s.doc().unwrap().node(id));
}

#[test]
fn text_font_drag_recomputes_auto_width_and_auto_height_bounds() {
    for fixed_width in [false, true] {
        let mut s = Session::new();
        let mut params = json!({"text":"Original\nTwo lines","x":100,"y":100,"props":{"fontFamily":"Inter","fontSize":20,"lineHeight":30}});
        if fixed_width {
            params["width"] = json!(240);
        }
        let id = id_of(&s.execute("text.create", &params).unwrap()["id"]).unwrap();
        let before = s.doc().unwrap().node(id).unwrap().clone();
        let a = point(&s, id, before.width / 2.0, before.height);
        let b = point(&s, id, before.width / 2.0, before.height * 1.5);
        drag(&mut s, a, b);
        let n = s.doc().unwrap().node(id).unwrap();
        assert!((data(&s, id).style.font_size - 30.0).abs() < 1e-6);
        assert!((n.height - before.height * 1.5).abs() < 1.1, "auto height grows at fixed line count: {}→{}", before.height, n.height);
        if fixed_width {
            assert_eq!(n.width, before.width);
        } else {
            assert!((n.width - before.width * 1.5).abs() < 1.1, "auto width follows font metrics");
        }
        assert_eq!(n.transform, before.transform);
        assert_eq!(data(&s, id).characters, before.kind.text().unwrap().characters);
    }
}
#[test]
fn text_font_drag_reflows_its_auto_layout_parent_and_keeps_sibling_identity() {
    let mut s = Session::new();
    let frame = id_of(
        &s.execute(
            "frame.create",
            &json!({"x":100,"y":100,"width":400,"height":100,"props":{"layout":"vertical","padding":0,"gap":8,"sizingV":"hug","fill":"#FFFFFF"}}),
        )
        .unwrap()["id"],
    )
    .unwrap();
    let id = id_of(
        &s.execute(
            "text.create",
            &json!({"parent":frame,"text":"Original\nTwo lines","width":240,"props":{"fontFamily":"Inter","fontSize":20,"lineHeight":30}}),
        )
        .unwrap()["id"],
    )
    .unwrap();
    let sibling = id_of(&s.execute("shape.rectangle", &json!({"parent":frame,"width":20,"height":10})).unwrap()["id"]).unwrap();
    s.execute("select.set", &json!({"ids":[id]})).unwrap();
    let before = s.doc().unwrap().clone();
    let text = before.node(id).unwrap();
    let a = point(&s, id, text.width / 2.0, text.height);
    let b = point(&s, id, text.width / 2.0, text.height * 1.5);
    drag(&mut s, a, b);
    let doc = s.doc().unwrap();
    let delta = doc.node(id).unwrap().height - text.height;
    assert!((delta - 30.0).abs() < 1.1);
    assert!((doc.node(frame).unwrap().height - before.node(frame).unwrap().height - delta).abs() < 1e-6);
    assert!((doc.node(sibling).unwrap().transform.as_coeffs()[5] - before.node(sibling).unwrap().transform.as_coeffs()[5] - delta).abs() < 1e-6);
    assert_eq!(doc.node(sibling).unwrap().kind, before.node(sibling).unwrap().kind);
    assert_eq!(doc.node(frame).unwrap().children, before.node(frame).unwrap().children);
}
