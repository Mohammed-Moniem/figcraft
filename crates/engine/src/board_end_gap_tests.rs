//! Observable automatic connector clearance and the editor behaviors that depend on it.

use base64::Engine as _;
use figcraft_doc::{BoardItem, End, NodeId, Side};
use figcraft_geom::Point;
use serde_json::json;

use crate::Session;
use crate::board_tests::{board, connector, drawn, id, node};
use crate::snap::Targets;
use crate::tools::{PointerEvent, PointerKind};

fn shape(s: &mut Session, x: f64, y: f64) -> NodeId {
    id(&s.execute("board.shape", &json!({"x": x, "y": y, "width": 100, "height": 100})).unwrap(), "id")
}

fn only_connector(s: &Session) -> NodeId {
    s.doc().unwrap().nodes().find(|n| matches!(n.board, Some(BoardItem::Connector(_)))).unwrap().id
}

fn doc_points(doc: &figcraft_doc::Document, id: NodeId) -> Vec<Point> {
    let figcraft_doc::NodeKind::Vector { network } = &doc.node(id).unwrap().kind else { panic!("connector vector") };
    let world = doc.world_transform(id);
    network.vertices.iter().map(|v| world * v.point()).collect()
}

fn outline_drop(hidden: bool, bounds_only: bool) -> (End, NodeId) {
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0);
    let b = if bounds_only {
        let page = s.page().unwrap();
        let doc = s.doc_mut().unwrap();
        let id = doc.alloc_id();
        let mut n = figcraft_doc::Node::new(
            id,
            figcraft_doc::NodeKind::Vector {
                network: figcraft_doc::VectorNetwork::from_points(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 100.0)], false),
            },
            "Bounds-only target",
            100.0,
            100.0,
        );
        n.transform = figcraft_geom::Affine::translate((400.0, 200.0));
        n.fills.clear();
        n.strokes = vec![figcraft_color::Paint::solid(figcraft_color::Color::new(0.0, 0.0, 0.0, 1.0))];
        doc.insert(n, page, None).unwrap();
        id
    } else {
        shape(&mut s, 400.0, 200.0)
    };
    if hidden {
        s.execute("node.set", &json!({"id":b,"props":{"visible":false}})).unwrap();
    }
    let c = id(&s.execute("board.connector", &json!({"from":a,"to":{"x":700,"y":300},"style":"elbow"})).unwrap(), "id");
    s.execute("tool.move", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids":[c]})).unwrap();
    {
        let view = &mut s.ds_mut().unwrap().view;
        view.outlines = true;
        view.outline_hidden = hidden;
        view.outline_bounds = bounds_only;
    }
    let first = drawn(&s, c)[0];
    let ev = |kind, p: Point| PointerEvent::at(kind, p.x, p.y, 1.0);
    s.pointer(&ev(PointerKind::Down, first)).unwrap();
    s.pointer(&ev(PointerKind::Move, Point::new(200.0, 150.0))).unwrap();
    let drop = Point::new(396.0, 250.0);
    assert_eq!(s.hit(s.doc().unwrap(), s.page().unwrap(), drop, 6.0), Some(c), "ordinary outline hit is occluded by the dragged connector");
    s.pointer(&ev(PointerKind::Move, drop)).unwrap();
    s.pointer(&ev(PointerKind::Up, drop)).unwrap();
    (connector(&s, c).from, b)
}

#[test]
fn board_end_gap_reattach_outline_hidden_target_behind_itself() {
    let (actual, b) = outline_drop(true, false);
    assert_eq!(actual, End::Node { node: b, side: Side::Left, at: None }, "outline-hidden targets remain available behind the dragged connector");
}

#[test]
fn board_end_gap_reattach_outline_bounds_target_behind_itself() {
    let (actual, b) = outline_drop(false, true);
    assert_eq!(
        actual,
        End::Node { node: b, side: Side::Left, at: None },
        "a displayed bounds-only target remains available behind the dragged connector"
    );
}

#[test]
fn board_end_gap_preview_commit_and_border_indicator_agree() {
    for style in ["straight", "elbow", "curved"] {
        let mut s = board();
        let a = shape(&mut s, 0.0, 0.0);
        let b = shape(&mut s, 300.0, 200.0);
        s.execute("tool.connector", &json!({"style": style})).unwrap();
        let ev = |kind, x, y| PointerEvent::at(kind, x, y, 1.0);
        let undo = s.undo_depth().0;
        s.pointer(&ev(PointerKind::Down, 97.0, 50.0)).unwrap();
        s.pointer(&ev(PointerKind::Move, 302.0, 250.0)).unwrap();
        let preview = only_connector(&s);
        assert_eq!(drawn(&s, preview).first(), Some(&Point::new(104.0, 50.0)), "{style}");
        assert_eq!(drawn(&s, preview).last(), Some(&Point::new(296.0, 250.0)), "{style}");
        let lit: Vec<_> = s.tool.overlay.attach.iter().filter(|(_, lit)| *lit).map(|(point, _)| *point).collect();
        assert_eq!(lit, vec![[300.0, 250.0]], "the border's left indicator remains lit: {style}");
        let before = node(&s, preview);
        s.pointer(&ev(PointerKind::Up, 302.0, 250.0)).unwrap();
        let made = only_connector(&s);
        assert_eq!(node(&s, made).kind, before.kind, "committed curve handles as well as anchors match the preview");
        assert_eq!(connector(&s, made).from, End::Node { node: a, side: Side::Right, at: None });
        assert_eq!(connector(&s, made).to, End::Node { node: b, side: Side::Left, at: None });
        assert_eq!(s.undo_depth().0, undo + 1);
        s.undo().unwrap();
        assert!(!s.doc().unwrap().contains(made));
    }
}

#[test]
fn board_end_gap_high_zoom_drop_at_displayed_offset_keeps_attachment() {
    for zoom in [1.0, 4.0] {
        let mut s = board();
        s.ds_mut().unwrap().view.zoom = zoom;
        let a = shape(&mut s, 0.0, 0.0);
        let b = shape(&mut s, 400.0, 200.0);
        let c = id(&s.execute("board.connector", &json!({"from": a, "to": {"x": 700, "y": 300}, "style": "elbow"})).unwrap(), "id");
        // Automatic routes now avoid B. Explicitly place a manual run over it to retain
        // this test's own-connector occlusion precondition at both zoom levels.
        s.execute("board.reshape", &json!({"id":c,"portion":1,"x":396,"y":250})).unwrap();
        assert!(connector(&s, c).path.is_some());
        s.execute("tool.move", &json!({})).unwrap();
        s.execute("select.set", &json!({"ids": [c]})).unwrap();
        let first = drawn(&s, c)[0];
        let ev = |kind, p: Point| PointerEvent::at(kind, p.x, p.y, zoom);
        s.pointer(&ev(PointerKind::Down, first)).unwrap();
        s.pointer(&ev(PointerKind::Move, Point::new(200.0, 150.0))).unwrap();
        let drop = Point::new(396.0, 250.0);
        assert_eq!(
            s.hit(s.doc().unwrap(), s.page().unwrap(), drop, 4.0 + 2.0 / zoom),
            Some(c),
            "the existing connector occludes this target in ordinary hit testing"
        );
        s.pointer(&ev(PointerKind::Move, drop)).unwrap();
        s.pointer(&ev(PointerKind::Up, drop)).unwrap();
        assert_eq!(connector(&s, c).from, End::Node { node: b, side: Side::Left, at: None }, "zoom {zoom}: an offset endpoint is still attached");
        s.execute("transform.move", &json!({"ids": [b], "dx": 40, "dy": 20})).unwrap();
        assert_eq!(drawn(&s, c).first(), Some(&Point::new(436.0, 270.0)), "the reattached end follows its shape exactly once");
    }
}

#[test]
fn board_end_gap_dependent_connector_and_label_are_not_self_snap_targets() {
    for inside_section in [false, true] {
        let mut s = board();
        let section =
            inside_section.then(|| id(&s.execute("node.create", &json!({"type":"section","x":0,"y":0,"width":300,"height":300})).unwrap(), "id"));
        let mut params = json!({"x":20,"y":20,"width":100,"height":100});
        if let Some(parent) = section {
            params["parent"] = json!(parent);
        }
        let a = id(&s.execute("board.shape", &params).unwrap(), "id");
        let b = shape(&mut s, 600.0, 200.0);
        let r = s.execute("board.connector", &json!({"from":a,"to":b,"style":"straight","label":"Joined"})).unwrap();
        let (c, label) = (id(&r, "id"), id(&r, "label"));
        let other = shape(&mut s, 900.0, 600.0);
        let doc = s.doc().unwrap();
        let (connector_box, label_box) = (doc.world_bounds(c).unwrap(), doc.world_bounds(label).unwrap());
        let unrelated = Targets::gather(doc, s.page().unwrap(), &[other]);
        assert!(
            unrelated.boxes.contains(&connector_box) && unrelated.boxes.contains(&label_box),
            "independent layers still snap to connectors and their labels"
        );
        let moving = section.unwrap_or(a);
        let own = Targets::gather(doc, s.page().unwrap(), &[moving]);
        assert!(!own.boxes.contains(&connector_box), "a moved shape/ancestor carries this connector: section={inside_section}");
        assert!(!own.boxes.contains(&label_box), "the connector's moving label is excluded too: section={inside_section}");
    }
}

#[test]
fn board_end_gap_cmd_drop_outside_the_shape_remains_an_exact_free_point() {
    let mut s = board();
    s.ds_mut().unwrap().view.zoom = 4.0;
    let a = shape(&mut s, 0.0, 0.0);
    shape(&mut s, 400.0, 200.0);
    let c = id(&s.execute("board.connector", &json!({"from":a,"to":{"x":700,"y":300},"style":"elbow"})).unwrap(), "id");
    s.execute("tool.move", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids":[c]})).unwrap();
    let ev = |kind, p: Point| PointerEvent { cmd: true, ..PointerEvent::at(kind, p.x, p.y, 4.0) };
    let first = drawn(&s, c)[0];
    s.pointer(&ev(PointerKind::Down, first)).unwrap();
    s.pointer(&ev(PointerKind::Move, Point::new(200.0, 150.0))).unwrap();
    let drop = Point::new(396.0, 250.0);
    s.pointer(&ev(PointerKind::Move, drop)).unwrap();
    s.pointer(&ev(PointerKind::Up, drop)).unwrap();
    assert_eq!(connector(&s, c).from, End::Point { x: drop.x, y: drop.y }, "Cmd/Ctrl does not enlarge manual placement to the automatic gap");
    assert_eq!(drawn(&s, c).first(), Some(&drop));
}

struct SavedBoard(std::path::PathBuf);

#[test]
fn board_end_gap_live_legacy_welcome_keeps_the_authoritative_geometry() {
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0);
    let b = shape(&mut s, 300.0, 0.0);
    let c = id(&s.execute("board.connector", &json!({"from":a,"fromSide":"right","to":b,"toSide":"left","style":"straight"})).unwrap(), "id");
    let mut legacy = s.doc().unwrap().clone();
    let n = legacy.node_mut(c).unwrap();
    n.transform = figcraft_geom::Affine::translate((100.0, 50.0));
    n.width = 200.0;
    n.height = 0.0;
    n.kind =
        figcraft_doc::NodeKind::Vector { network: figcraft_doc::VectorNetwork::from_points(&[Point::new(0.0, 0.0), Point::new(200.0, 0.0)], false) };
    let mut room = crate::collab_live_tests::Room::new(legacy, 2, 192_016);
    assert_eq!(doc_points(room.server.document(), c), vec![Point::new(100.0, 50.0), Point::new(300.0, 50.0)]);
    for i in 0..2 {
        assert_eq!(
            doc_points(room.s(i).live().client().unwrap().document(), c),
            vec![Point::new(100.0, 50.0), Point::new(300.0, 50.0)],
            "the authoritative client replica stays exact"
        );
    }
    for i in 0..2 {
        assert_eq!(
            drawn(room.s(i), c),
            vec![Point::new(100.0, 50.0), Point::new(300.0, 50.0)],
            "a live welcome must show the server's saved geometry, not a native-file migration"
        );
    }
    room.check();
    room.s(0).execute("transform.move", &json!({"ids":[a],"dx":0,"dy":30})).unwrap();
    room.settle();
    room.check();
    for i in 0..2 {
        assert_eq!(
            drawn(room.s(i), c),
            vec![Point::new(104.0, 80.0), Point::new(296.0, 50.0)],
            "a subsequent real edit reroutes through synchronized changes"
        );
    }
    assert_eq!(room.s(1).undo_depth().0, 0, "remote rerouting never becomes the other participant's undo step");
}

impl SavedBoard {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("figcraft-board-end-gap-{}-{n}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for SavedBoard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn board_end_gap_real_archive_reopen_reroute_undo_and_svg_keep_geometry() {
    for style in ["straight", "elbow", "curved"] {
        let scratch = SavedBoard::new();
        let path = scratch.0.join("Synthetic board.figcraft");
        let mut s = board();
        let a = shape(&mut s, 0.0, 0.0);
        let b = shape(&mut s, 300.0, 200.0);
        let c = id(
            &s.execute(
                "board.connector",
                &json!({"from":a,"fromSide":"right","to":b,"toSide":"left","style":style,"startCap":"none","endCap":"none"}),
            )
            .unwrap(),
            "id",
        );
        let manual = id(&s.execute("board.connector", &json!({"from":a,"fromAt":[0.75,0.25],"to":{"x":600,"y":50},"style":style})).unwrap(), "id");
        s.execute("file.saveAs", &json!({"path":path.to_string_lossy()})).unwrap();
        let mut reopened = Session::empty();
        reopened.execute("file.open", &json!({"path":path.to_string_lossy()})).unwrap();
        assert!(reopened.doc().unwrap().validate().is_empty());
        assert_eq!(connector(&reopened, c), connector(&s, c), "automatic attachment metadata survived: {style}");
        assert_eq!(connector(&reopened, manual), connector(&s, manual), "manual attachment metadata survived: {style}");
        assert_eq!(drawn(&reopened, c), drawn(&s, c), "saved derived route survived: {style}");
        assert_eq!(drawn(&reopened, c).first(), Some(&Point::new(104.0, 50.0)));
        assert_eq!(drawn(&reopened, c).last(), Some(&Point::new(296.0, 250.0)));
        assert_eq!(drawn(&reopened, manual).first(), Some(&Point::new(75.0, 25.0)));
        reopened.execute("transform.move", &json!({"ids":[a],"dx":20,"dy":10})).unwrap();
        assert_eq!(drawn(&reopened, c).first(), Some(&Point::new(124.0, 60.0)), "one offset after restore and reroute");
        assert_eq!(drawn(&reopened, manual).first(), Some(&Point::new(95.0, 35.0)));
        reopened.undo().unwrap();
        assert_eq!(drawn(&reopened, c).first(), Some(&Point::new(104.0, 50.0)));
        assert_eq!(drawn(&reopened, manual).first(), Some(&Point::new(75.0, 25.0)));
        reopened.execute("edit.redo", &json!({})).unwrap();
        assert_eq!(drawn(&reopened, c).first(), Some(&Point::new(124.0, 60.0)));
        assert_eq!(drawn(&reopened, manual).first(), Some(&Point::new(95.0, 35.0)));
        reopened.undo().unwrap();
        assert_eq!(drawn(&reopened, c).first(), Some(&Point::new(104.0, 50.0)));

        let original = node(&reopened, c);
        let figcraft_doc::NodeKind::Vector { network: expected } = &original.kind else { panic!("connector vector") };
        let points = drawn(&reopened, c);
        let span = *points.last().unwrap() - points[0];
        let svg = figcraft_svg::export_svg(reopened.doc().unwrap(), &[c], &Default::default()).unwrap();
        assert!(svg.contains("<path") && !svg.contains("<image"));
        let mut back = figcraft_doc::Document::new("Exported connector");
        let page = back.first_page().unwrap();
        let imported = figcraft_svg::import_svg_with_notes(&mut back, svg.as_bytes(), page, 0.0, 0.0, "Connector").unwrap();
        assert!(imported.notes.is_empty(), "{style}: {:?}", imported.notes);
        assert!(back.validate().is_empty());
        let kept = back.nodes().any(|n| {
            let world = back.world_transform(n.id);
            let vector = |x, y| world * Point::new(x, y) - world * Point::ZERO;
            match &n.kind {
                figcraft_doc::NodeKind::Vector { network }
                    if network.vertices.len() == expected.vertices.len() && network.segments.len() == expected.segments.len() =>
                {
                    let first = world * network.vertices.first().unwrap().point();
                    let last = world * network.vertices.last().unwrap().point();
                    (last - first - span).hypot() < 0.001
                        && expected.segments.iter().zip(&network.segments).all(|(a, b)| {
                            [(a.ta, b.ta), (a.tb, b.tb)]
                                .into_iter()
                                .all(|(a, b)| (figcraft_geom::Vec2::new(a.0, a.1) - vector(b.0, b.1)).hypot() < 0.001)
                        })
                }
                figcraft_doc::NodeKind::Line if style == "straight" => (vector(n.width, 0.0) - span).hypot() < 0.001,
                _ => false,
            }
        });
        assert!(kept, "{style}: editable exported geometry retains the gap-adjusted endpoint span and cubic handles");
    }
}

#[test]
fn board_end_gap_legacy_native_open_refreshes_only_automatic_routes_cleanly() {
    let scratch = SavedBoard::new();
    let path = scratch.0.join("Legacy synthetic board.figcraft");
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0);
    let b = shape(&mut s, 300.0, 0.0);
    let c = id(&s.execute("board.connector", &json!({"from":a,"fromSide":"right","to":b,"toSide":"left","style":"straight"})).unwrap(), "id");
    let on = id(&s.execute("board.connector", &json!({"from":a,"fromAt":[0.75,0.25],"to":b,"toAt":[0.25,0.75],"style":"curved"})).unwrap(), "id");
    let free = id(&s.execute("board.connector", &json!({"from":{"x":20,"y":150},"to":{"x":300,"y":170},"style":"curved"})).unwrap(), "id");
    let mut old = s.doc().unwrap().clone();
    // An older native board stores both attachment metadata and its derived border-start vector.
    {
        let n = old.node_mut(c).unwrap();
        n.transform = figcraft_geom::Affine::translate((100.0, 50.0));
        n.width = 200.0;
        n.height = 0.0;
        n.kind = figcraft_doc::NodeKind::Vector {
            network: figcraft_doc::VectorNetwork::from_points(&[Point::new(0.0, 0.0), Point::new(200.0, 0.0)], false),
        };
    }
    let plain = old.alloc_id();
    let mut vector = old.node(c).unwrap().clone();
    vector.id = plain;
    vector.name = "Saved vector without board connector metadata".into();
    vector.board = None;
    vector.transform = figcraft_geom::Affine::translate((500.0, 100.0));
    old.insert(vector, old.pages[0], None).unwrap();
    assert!(old.validate().is_empty());
    let original_controls: Vec<_> = [on, free, plain].into_iter().map(|id| (id, old.node(id).unwrap().clone())).collect();
    let bytes = figcraft_format::save_bytes(&old, None).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let mut reopened = Session::empty();
    reopened.execute("file.open", &json!({"path":path.to_string_lossy()})).unwrap();
    assert_eq!(
        drawn(&reopened, c),
        vec![Point::new(104.0, 50.0), Point::new(296.0, 50.0)],
        "automatic legacy geometry is refreshed when opened, without needing an edit"
    );
    for (id, expected) in original_controls {
        let actual = node(&reopened, id);
        assert_eq!(actual.kind, expected.kind, "manual or imported vector geometry: {id}");
        assert_eq!(actual.transform, expected.transform);
        assert_eq!((actual.width, actual.height), (expected.width, expected.height));
        assert_eq!(actual.board, expected.board);
    }
    assert!(!reopened.ds().unwrap().is_dirty(), "initial derived settlement belongs to the clean loaded baseline");
    assert_eq!(reopened.undo_depth(), (0, 0), "opening makes no edit to undo");
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "opening does not rewrite the file on disk");

    let mut data = Session::empty();
    data.execute("file.open", &json!({"data":base64::engine::general_purpose::STANDARD.encode(&bytes),"name":"Legacy synthetic.figcraft"})).unwrap();
    assert_eq!(drawn(&data, c), vec![Point::new(104.0, 50.0), Point::new(296.0, 50.0)], "native data input opts into the same migration");
    assert!(!data.ds().unwrap().is_dirty());
    assert_eq!(data.undo_depth(), (0, 0));
    let recovered = crate::cmd::file::recovered_from(&bytes).unwrap();
    assert_eq!(doc_points(&recovered.doc, c), vec![Point::new(104.0, 50.0), Point::new(296.0, 50.0)]);
    assert!(recovered.is_dirty(), "recovery retains its existing unsaved semantics");
    reopened.execute("transform.move", &json!({"ids":[a],"dx":20,"dy":0})).unwrap();
    reopened.execute("file.revert", &json!({})).unwrap();
    assert_eq!(drawn(&reopened, c), vec![Point::new(104.0, 50.0), Point::new(296.0, 50.0)], "native revert also adopts a clean refreshed route");
    assert!(!reopened.ds().unwrap().is_dirty());
    assert_eq!(reopened.undo_depth(), (0, 0));
}
