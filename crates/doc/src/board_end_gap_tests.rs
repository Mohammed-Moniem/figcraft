//! Automatic connector clearance, with exact hand-placed and free endpoint controls.

use figcraft_geom::{Point, Rect};

use crate::{ConnectorPath, RouteEnd, Routing, Side, route_along, route_points};

const GAP: f64 = 4.0;

fn hit_scene() -> (crate::Document, crate::NodeId, crate::NodeId, crate::NodeId, crate::NodeId) {
    use figcraft_color::{Color, Paint};
    let mut doc = crate::Document::new("Original hit-query controls");
    doc.kind = crate::FileKind::Board;
    let page = doc.first_page().unwrap();
    let root = doc.alloc_id();
    let mut frame = crate::Node::new(root, crate::NodeKind::Frame, "Focused frame", 100.0, 100.0);
    frame.layout = Some(crate::AutoLayout { reverse_z: true, ..Default::default() });
    doc.insert(frame, page, None).unwrap();
    let connector = doc.alloc_id();
    let mut line = crate::Node::new(
        connector,
        crate::NodeKind::Vector { network: crate::VectorNetwork::from_points(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0)], false) },
        "Dragged connector",
        100.0,
        0.0,
    );
    line.transform = figcraft_geom::Affine::translate((0.0, 50.0));
    line.fills.clear();
    line.strokes = vec![Paint::solid(Color::new(0.0, 0.0, 0.0, 1.0))];
    line.board = Some(crate::BoardItem::Connector(crate::Connector {
        from: crate::End::Point { x: 0.0, y: 50.0 },
        to: crate::End::Point { x: 100.0, y: 50.0 },
        routing: Routing::Straight,
        path: None,
    }));
    doc.insert(line, root, None).unwrap();
    let label = doc.alloc_id();
    let mut n = crate::Node::new(label, crate::NodeKind::Frame, "Own label", 100.0, 100.0);
    n.board = Some(crate::BoardItem::Label { connector });
    doc.insert(n, root, None).unwrap();
    let a = doc.alloc_id();
    doc.insert(crate::Node::new(a, crate::NodeKind::Rectangle, "Front by reverse paint order", 100.0, 100.0), root, None).unwrap();
    let b = doc.alloc_id();
    doc.insert(crate::Node::new(b, crate::NodeKind::Rectangle, "Behind", 100.0, 100.0), root, None).unwrap();
    (doc, root, connector, a, b)
}

#[test]
fn board_end_gap_hit_query_keeps_focused_root_reverse_order_and_own_label() {
    let (mut doc, root, c, a, b) = hit_scene();
    let p = Point::new(50.0, 50.0);
    assert_eq!(crate::geometry::hit_in(&doc, root, p, 2.0, None), Some(c), "the original connector is in front");
    assert_eq!(
        crate::geometry::hit_without_connector(&doc, root, p, 2.0, None, None, c),
        Some(a),
        "skip connector/label and retain reverse paint order"
    );
    doc.node_mut(a).unwrap().visible = false;
    doc.node_mut(b).unwrap().visible = false;
    assert_eq!(crate::geometry::hit_without_connector(&doc, root, p, 2.0, None, None, c), Some(root), "the focused root itself remains a target");
}

#[test]
fn board_end_gap_hit_query_keeps_the_focused_roots_clip() {
    let (mut doc, root, c, a, b) = hit_scene();
    doc.node_mut(root).unwrap().clip = true;
    doc.node_mut(a).unwrap().transform = figcraft_geom::Affine::translate((120.0, 10.0));
    doc.node_mut(b).unwrap().visible = false;
    let p = Point::new(150.0, 50.0);
    assert_eq!(crate::geometry::hit_in(&doc, root, p, 2.0, None), None);
    assert_eq!(
        crate::geometry::hit_without_connector(&doc, root, p, 2.0, None, None, c),
        None,
        "skipping the connector cannot bypass its focused root's clip"
    );
}

#[test]
fn board_end_gap_offsets_each_automatic_side_for_every_route_style() {
    let b = Rect::new(10.0, 20.0, 110.0, 100.0);
    for side in [Side::Top, Side::Right, Side::Bottom, Side::Left] {
        let border = side.point(b);
        let expected = border + side.normal() * GAP;
        let free = border + side.normal() * 300.0;
        for routing in [Routing::Straight, Routing::Elbow, Routing::Curved] {
            let points = route_points(RouteEnd::Box(b, side), RouteEnd::Point(free), routing).unwrap();
            assert_eq!(points.first(), Some(&expected), "{side:?} {routing:?}");
            assert_eq!(points.last(), Some(&free), "the free end stays exact");
            let reverse = route_points(RouteEnd::Point(free), RouteEnd::Box(b, side), routing).unwrap();
            assert_eq!(reverse.first(), Some(&free));
            assert_eq!(reverse.last(), Some(&expected), "destination {side:?} {routing:?}");
        }
    }
    let (a, b) = (Rect::new(0.0, 0.0, 100.0, 100.0), Rect::new(500.0, 300.0, 600.0, 400.0));
    for routing in [Routing::Straight, Routing::Elbow, Routing::Curved] {
        let points = route_points(RouteEnd::Box(a, Side::Auto), RouteEnd::Box(b, Side::Auto), routing).unwrap();
        assert_eq!(points.first(), Some(&Point::new(104.0, 50.0)));
        assert_eq!(points.last(), Some(&Point::new(496.0, 350.0)));
    }
}

#[test]
fn board_end_gap_retains_elbow_runs_and_curved_outward_tangents() {
    let b = Rect::new(0.0, 0.0, 100.0, 100.0);
    for destination in [Point::new(500.0, -120.0), Point::new(500.0, 220.0)] {
        for routing in [Routing::Elbow, Routing::Curved] {
            let points = route_points(RouteEnd::Box(b, Side::Right), RouteEnd::Point(destination), routing).unwrap();
            assert_eq!(points[0], Point::new(104.0, 50.0));
            assert!(points[1].x > points[0].x, "leave outward: {points:?}");
            assert_eq!(points[1].y, points[0].y, "initial elbow run or cubic tangent follows the normal");
        }
    }
}

#[test]
fn board_end_gap_preserves_hand_placed_and_free_points_exactly() {
    let b = Rect::new(0.0, 0.0, 100.0, 100.0);
    let manual = Point::new(21.25, 13.5);
    let free = Point::new(450.75, 260.125);
    for routing in [Routing::Straight, Routing::Elbow, Routing::Curved] {
        let points = route_points(RouteEnd::On(b, Side::Auto, manual), RouteEnd::Point(free), routing).unwrap();
        assert_eq!(points.first(), Some(&manual));
        assert_eq!(points.last(), Some(&free));
        let points = route_points(RouteEnd::Point(free), RouteEnd::On(b, Side::Left, manual), routing).unwrap();
        assert_eq!(points.first(), Some(&free));
        assert_eq!(points.last(), Some(&manual));
    }
    assert!(route_points(RouteEnd::Point(Point::new(f64::NAN, 0.0)), RouteEnd::Point(free), Routing::Curved).is_none());
}

#[test]
fn board_end_gap_reshaped_routes_use_the_same_single_endpoint_offset() {
    let from = RouteEnd::Box(Rect::new(0.0, 0.0, 100.0, 100.0), Side::Right);
    let to = RouteEnd::Box(Rect::new(500.0, 300.0, 600.0, 400.0), Side::Left);
    for (routing, path) in
        [(Routing::Curved, ConnectorPath::Bow { x: 15.0, y: -20.0 }), (Routing::Elbow, ConnectorPath::Runs { across: true, at: vec![30.0, 50.0] })]
    {
        let points = route_along(from, to, routing, Some(&path)).unwrap();
        assert_eq!(points.first(), Some(&Point::new(104.0, 50.0)));
        assert_eq!(points.last(), Some(&Point::new(496.0, 350.0)));
        assert_eq!(points, route_along(from, to, routing, Some(&path)).unwrap(), "routing is deterministic");
    }
}
