//! Native PATTERN controls for #2421. The small Kiwi schema is independent of the .fig exporter.

use figcraft_doc::color::{BlendMode, PaintKind, PatternAlign, PatternTile};

use super::kiwi::{Kv, Schema};
use super::tests::{arr, at, canvas, e, f, g, node, o, solid, xy};

const SCHEMA: &str = "
enum NodeType DOCUMENT CANVAS FRAME RECTANGLE
enum PaintType SOLID PATTERN
enum PatternTileType RECTANGULAR HORIZONTAL_HEXAGONAL VERTICAL_HEXAGONAL
enum PatternAlignment START CENTER END
enum BlendMode NORMAL LIGHTEN
struct GUID uint sessionID, uint localID
struct Color float r, float g, float b, float a
struct Vector float x, float y
struct Matrix float m00, float m01, float m02, float m10, float m11, float m12
message Paint PaintType type, Color color, float opacity, bool visible, GUID sourceNodeId, PatternTileType patternTileType, float scale, Vector patternSpacing, PatternAlignment horizontalAlignment, BlendMode blendMode
message ParentIndex GUID guid, string position
message NodeChange GUID guid, ParentIndex parentIndex, NodeType type, string name, Vector size, Matrix transform, Paint[] fillPaints, Paint[] strokePaints, float strokeWeight, bool internalOnly
message Message NodeChange[] nodeChanges
";

fn pattern(tile: u64) -> Kv<'static> {
    o(vec![("type", e("PATTERN")), ("sourceNodeId", g(tile))])
}

fn message(fills: Vec<Kv<'static>>, strokes: Vec<Kv<'static>>) -> Kv<'static> {
    let rect = |id, parent, name, x, fill| {
        node(id, Some((parent, "!")), "RECTANGLE", name, vec![("size", xy(4.0, 8.0)), ("transform", at(x, 0.0)), ("fillPaints", arr(vec![fill]))])
    };
    o(vec![(
        "nodeChanges",
        arr(vec![
            node(0, None, "DOCUMENT", "Native pattern", vec![]),
            node(1, Some((0, "!")), "CANVAS", "Page", vec![]),
            node(
                2,
                Some((1, "!")),
                "RECTANGLE",
                "Painted",
                vec![
                    ("size", xy(36.0, 24.0)),
                    ("transform", at(0.0, 0.0)),
                    ("fillPaints", arr(fills)),
                    ("strokePaints", arr(strokes)),
                    ("strokeWeight", f(1.0)),
                ],
            ),
            node(8, Some((0, "#")), "CANVAS", "Internal", vec![("internalOnly", Kv::Bool(true))]),
            node(9, Some((8, "!")), "FRAME", "Tile shelf", vec![("size", xy(60.0, 40.0))]),
            node(10, Some((9, "!")), "FRAME", "Tile", vec![("size", xy(8.0, 8.0)), ("transform", at(20.0, 20.0))]),
            rect(11, 10, "Red half", 0.0, solid(1.0, 0.0, 0.0)),
            rect(12, 10, "Blue half", 4.0, pattern(13)),
            rect(13, 8, "Blue tile", 0.0, solid(0.0, 0.0, 1.0)),
            rect(14, 9, "Unused sibling", 0.0, solid(0.0, 1.0, 0.0)),
        ]),
    )])
}

fn fixture(fills: Vec<Kv<'static>>, strokes: Vec<Kv<'static>>) -> Vec<u8> {
    canvas(&Schema::parse(SCHEMA).unwrap(), &message(fills, strokes))
}

fn named<'a>(doc: &'a figcraft_doc::Document, name: &str) -> &'a figcraft_doc::Node {
    doc.nodes().find(|node| node.name == name).unwrap_or_else(|| panic!("missing layer {name}"))
}

// The renderer imports the checked-in ZIPs. Compare decoded messages so that codec updates
// do not require regenerating the archives.
fn assert_archived_message(canvas: &[u8], archive: &[u8]) {
    let mut packaged = zip::ZipArchive::new(std::io::Cursor::new(archive)).unwrap();
    let mut wire = Vec::new();
    std::io::Read::read_to_end(&mut packaged.by_name("canvas.fig").unwrap(), &mut wire).unwrap();
    assert_eq!(super::decode_canvas(canvas).unwrap().1, super::decode_canvas(&wire).unwrap().1);
}

#[test]
fn native_pattern_resolves_hidden_fill_and_nested_sources() {
    let fill = o(vec![
        ("type", e("PATTERN")),
        ("sourceNodeId", g(10)),
        ("patternTileType", e("RECTANGULAR")),
        ("scale", f(1.0)),
        ("patternSpacing", xy(4.0, 4.0)),
        ("horizontalAlignment", e("START")),
    ]);
    let bytes = fixture(vec![fill], vec![]);
    assert_archived_message(&bytes, include_bytes!("../../tests/fixtures/native-pattern.fig"));
    let (doc, warnings) = super::import(&bytes, "pattern.fig").unwrap();
    assert!(!warnings.iter().any(|w| w.contains("pattern")), "{warnings:?}");
    let PaintKind::Pattern(p) = &named(&doc, "Painted").fills[0].kind else { panic!("missing pattern fill") };
    assert_eq!(p.source, named(&doc, "Tile").id.0);
    assert_eq!((p.tile, p.align, p.scale, p.spacing), (PatternTile::Grid, PatternAlign::Start, 1.0, (4.0, 4.0)));
    assert!(matches!(&named(&doc, "Blue half").fills[0].kind, PaintKind::Pattern(p) if p.source == named(&doc, "Blue tile").id.0));
    assert_eq!(named(&doc, "Tile").children.len(), 2);
    assert!(!doc.nodes().any(|node| node.name == "Unused sibling"));
    assert_eq!(doc.get(doc.first_page().unwrap()).unwrap().children.len(), 1);
}

#[test]
fn native_pattern_stroke_only_imports_its_hidden_source_with_hex_settings() {
    let stroke = o(vec![
        ("type", e("PATTERN")),
        ("sourceNodeId", g(13)),
        ("patternTileType", e("VERTICAL_HEXAGONAL")),
        ("scale", f(0.5)),
        ("patternSpacing", xy(2.0, 3.0)),
        ("horizontalAlignment", e("END")),
        ("opacity", f(0.25)),
        ("visible", Kv::Bool(true)),
        ("blendMode", e("LIGHTEN")),
    ]);
    let (doc, warnings) = super::import(&fixture(vec![], vec![stroke]), "stroke.fig").unwrap();
    assert!(!warnings.iter().any(|w| w.contains("pattern")), "{warnings:?}");
    let stroke = &named(&doc, "Painted").strokes[0];
    let PaintKind::Pattern(p) = &stroke.kind else { panic!("missing pattern stroke") };
    assert_eq!((p.source, p.tile, p.align), (named(&doc, "Blue tile").id.0, PatternTile::VerticalHex, PatternAlign::End));
    assert_eq!((p.scale, p.spacing, stroke.opacity, stroke.blend), (0.5, (2.0, 3.0), 0.25, BlendMode::Lighten));
    assert!(!doc.nodes().any(|node| matches!(node.name.as_str(), "Tile" | "Tile shelf" | "Unused sibling")));
}

#[test]
fn native_pattern_missing_sources_keep_solid_paint_and_skip_hidden_branches() {
    let paints = vec![o(vec![("type", e("PATTERN"))]), pattern(999), solid(0.0, 1.0, 0.0)];
    let bytes = fixture(paints, vec![]);
    assert_archived_message(&bytes, include_bytes!("../../tests/fixtures/native-pattern-missing.fig"));
    let (doc, warnings) = super::import(&bytes, "missing.fig").unwrap();
    let fills = &named(&doc, "Painted").fills;
    assert_eq!(fills.len(), 1);
    assert!(matches!(fills[0].kind, PaintKind::Solid { .. }));
    assert!(warnings.iter().any(|w| w.contains("without a source layer")), "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("tile a layer that wasn't imported")), "{warnings:?}");
    assert!(!doc.nodes().any(|node| matches!(node.name.as_str(), "Tile" | "Blue tile" | "Unused sibling")));
}

#[test]
fn native_pattern_sources_share_the_library_layer_budget() {
    let msg = message(vec![pattern(10), pattern(11), pattern(12), pattern(13), pattern(14)], vec![]);
    let (rest, warnings, _) = super::map::to_rest(&msg, "cap", 3);
    assert!(warnings.iter().any(|w| w.contains("component reference scan reached the 3-layer limit")), "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("internal library scan reached the 3-layer limit")), "{warnings:?}");
    assert_eq!(rest["document"]["children"][0]["children"][0]["name"], "Painted");
}

#[test]
fn native_pattern_source_below_internal_depth_limit_leaves_the_fallback_fill() {
    let mut nodes = vec![
        node(0, None, "DOCUMENT", "Depth control", vec![]),
        node(1, Some((0, "!")), "CANVAS", "Page", vec![]),
        node(2, Some((1, "!")), "RECTANGLE", "Painted", vec![("fillPaints", arr(vec![pattern(300), solid(1.0, 0.0, 0.0)]))]),
        node(3, Some((0, "#")), "CANVAS", "Internal", vec![("internalOnly", Kv::Bool(true))]),
    ];
    for id in 4..300 {
        nodes.push(node(id, Some((id - 1, "!")), "FRAME", "Internal ancestor", vec![]));
    }
    nodes.push(node(300, Some((299, "!")), "RECTANGLE", "Too deep", vec![]));
    let bytes = canvas(&Schema::parse(SCHEMA).unwrap(), &o(vec![("nodeChanges", arr(nodes))]));
    let (doc, warnings) = super::import(&bytes, "depth.fig").unwrap();
    assert!(warnings.iter().any(|w| w.contains("internal library branches deeper than 256 levels")), "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("tile a layer that wasn't imported")), "{warnings:?}");
    assert!(!doc.nodes().any(|node| node.name == "Too deep" || node.name == "Internal ancestor"));
    assert_eq!(named(&doc, "Painted").fills.len(), 1);
    assert!(matches!(named(&doc, "Painted").fills[0].kind, PaintKind::Solid { .. }));
}
