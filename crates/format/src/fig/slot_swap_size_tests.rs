//! Original issue #2098 wire controls. No .fig exporter or reporter document is used.

use figcraft_doc::color::{Color, Paint};
use figcraft_doc::{Document, HConstraint, Node, NodeId, NodeKind, VConstraint};

use super::kiwi::Kv;
use super::network::{Network, Region, Segment, Vertex};
use super::tests::{arr, at, e, g, node, o, schema, solid, u, xy};

fn icon_blob(square: bool) -> Vec<u8> {
    let points = if square { vec![(0.0, 0.0), (24.0, 0.0), (24.0, 24.0), (0.0, 24.0)] } else { vec![(0.0, 0.0), (24.0, 0.0), (0.0, 24.0)] };
    let count = points.len();
    let network = Network {
        vertices: points.into_iter().map(|(x, y)| Vertex { style: 0, x, y }).collect(),
        segments: (0..count).map(|a| Segment { style: 0, a, ta: (0.0, 0.0), b: (a + 1) % count, tb: (0.0, 0.0) }).collect(),
        regions: vec![Region { style: 0, non_zero: true, loops: vec![(0..count).collect()] }],
    };
    super::network::encode(&network)
}

fn symbol(id: u64, name: &str) -> Kv<'static> {
    node(id, Some((1, if id == 10 { "!" } else { "\"" })), "SYMBOL", name, vec![("size", xy(24.0, 24.0)), ("transform", at(0.0, 0.0))])
}

fn vector(id: u64, parent: u64, blob: u64, name: &str, blue: bool) -> Kv<'static> {
    node(
        id,
        Some((parent, "!")),
        "VECTOR",
        name,
        vec![
            ("size", xy(24.0, 24.0)),
            ("transform", at(0.0, 0.0)),
            ("horizontalConstraint", e("SCALE")),
            ("verticalConstraint", e("SCALE")),
            ("fillPaints", arr(vec![if blue { solid(0.0, 0.0, 1.0) } else { solid(1.0, 0.0, 0.0) }])),
            ("vectorData", o(vec![("vectorNetworkBlob", u(blob)), ("normalizedSize", xy(24.0, 24.0))])),
        ],
    )
}

fn fixture(explicit_size: Option<(f64, f64)>) -> Vec<u8> {
    let mut row = vec![("guidPath", o(vec![("guids", arr(vec![g(30)]))])), ("symbolData", o(vec![("symbolID", g(12))]))];
    if let Some((w, h)) = explicit_size {
        row.push(("size", xy(w, h)));
    }
    // The missing-size case is the original report, not an exporter's derived-size row.
    assert_eq!(row.iter().any(|(name, _)| *name == "size"), explicit_size.is_some());
    let nodes = vec![
        node(0, None, "DOCUMENT", "Original slot swap", vec![]),
        node(1, Some((0, "!")), "CANVAS", "Page", vec![]),
        symbol(10, "Alpha"),
        vector(11, 10, 0, "Alpha triangle", false),
        symbol(12, "Beta"),
        vector(13, 12, 1, "Beta square", true),
        node(20, Some((1, "#")), "SYMBOL", "Outer", vec![("size", xy(420.0, 420.0)), ("transform", at(100.0, 100.0))]),
        node(
            30,
            Some((20, "!")),
            "INSTANCE",
            "Icon slot",
            vec![("size", xy(420.0, 420.0)), ("transform", at(0.0, 0.0)), ("symbolData", o(vec![("symbolID", g(10))]))],
        ),
        node(
            40,
            Some((1, "$")),
            "INSTANCE",
            "Outer copy",
            vec![
                ("size", xy(420.0, 420.0)),
                ("transform", at(600.0, 100.0)),
                ("symbolData", o(vec![("symbolID", g(20)), ("symbolOverrides", arr(vec![o(row)]))])),
            ],
        ),
    ];
    assert!(nodes.iter().all(|node| node.get("derivedSymbolData").is_none()));
    let message = o(vec![
        ("nodeChanges", arr(nodes)),
        ("blobs", arr([false, true].into_iter().map(|square| o(vec![("bytes", Kv::Bytes(icon_blob(square)))])).collect())),
    ]);
    super::tests::canvas(&schema(), &message)
}

fn named<'a>(doc: &'a Document, name: &str) -> &'a Node {
    doc.nodes().find(|node| node.name == name).unwrap_or_else(|| panic!("missing original fixture layer {name}"))
}

fn check(doc: &Document, size: (f64, f64)) -> Vec<NodeId> {
    let outer = named(doc, "Outer copy");
    assert_eq!(outer.children.len(), 1);
    let slot = doc.get(outer.children[0]).unwrap();
    let beta = named(doc, "Beta");
    assert!(matches!(slot.kind, NodeKind::Instance { component, .. } if component == beta.id), "the override selects Beta");
    assert_eq!((slot.width, slot.height), size, "swapped root keeps slot/explicit size");
    assert_eq!(slot.children.len(), 1);
    let vector = doc.get(slot.children[0]).unwrap();
    assert_eq!(vector.name, "Beta square");
    assert_eq!((vector.width, vector.height), size, "Scale-constrained child follows the swapped root");
    assert_eq!((vector.constraints.horizontal, vector.constraints.vertical), (HConstraint::Scale, VConstraint::Scale));
    let NodeKind::Vector { network } = &vector.kind else { panic!("replacement's vector is editable") };
    assert_eq!(network.vertices.len(), 4, "replacement geometry, not Alpha's triangle");
    assert_eq!(network.vertices.iter().map(|v| v.x).fold(0.0, f64::max), size.0);
    assert_eq!(network.vertices.iter().map(|v| v.y).fold(0.0, f64::max), size.1);
    assert_eq!(vector.fills, [Paint::solid(Color::new(0.0, 0.0, 1.0, 1.0))]);
    for name in ["Alpha", "Beta"] {
        let main = named(doc, name);
        assert_eq!((main.width, main.height), (24.0, 24.0));
        let child = doc.get(main.children[0]).unwrap();
        assert_eq!((child.width, child.height), (24.0, 24.0), "mains remain at native size");
    }
    vec![outer.id, slot.id, vector.id, beta.id]
}

fn imported(explicit: Option<(f64, f64)>) -> Document {
    let (doc, warnings) = super::import(&fixture(explicit), "Original slot swap").unwrap();
    assert!(warnings.is_empty(), "original supported fixture has no import warnings: {warnings:?}");
    doc
}

#[test]
fn slot_swap_size_symbol_only_override_preserves_the_420_slot_and_vector() {
    check(&imported(None), (420.0, 420.0));
}

#[test]
fn slot_swap_size_an_explicit_nonuniform_size_override_wins() {
    check(&imported(Some((84.0, 126.0))), (84.0, 126.0));
}

#[test]
fn slot_swap_size_native_bytes_reopen_retains_component_source_geometry_and_style() {
    for explicit in [None, Some((84.0, 126.0))] {
        let doc = imported(explicit);
        let size = explicit.unwrap_or((420.0, 420.0));
        let ids = check(&doc, size);
        let bytes = crate::save_bytes(&doc, None).unwrap();
        let (back, warnings) = crate::load_bytes(&bytes).unwrap();
        assert!(warnings.is_empty(), "native reopen warnings: {warnings:?}");
        assert_eq!(check(&back, size), ids);
        for id in ids {
            assert_eq!(back.node(id), doc.node(id), "native serialization keeps source/geometry/style at {id}");
        }
    }
}
