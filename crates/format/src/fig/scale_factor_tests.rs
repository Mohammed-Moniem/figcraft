//! An instance scaled with the Scale tool stores `symbolData.uniformScaleFactor` (#2091 item 1):
//! its layers draw the main's lengths times the factor (composed through nested instances),
//! unless an override or derived row gives the value, which is at the instance's scale already.
//! Fixtures built in code with the parent module's helpers.

use figcraft_doc::{Document, EffectKind, LetterSpacing, LineHeight, Node, NodeKind};

use super::kiwi::{Kv, Schema};
use super::tests::{arr, at, canvas, e, f, g, node, o, s, xy};
use super::*;

const SCHEMA: &str = "
enum NodeType NONE DOCUMENT CANVAS FRAME ROUNDED_RECTANGLE TEXT SYMBOL INSTANCE VECTOR
enum StackMode NONE HORIZONTAL VERTICAL
enum EffectType DROP_SHADOW FOREGROUND_BLUR
enum BlurOpType NORMAL PROGRESSIVE
enum StyleType NONE FILL TEXT EFFECT GRID
enum NumberUnits RAW PIXELS PERCENT
struct GUID uint sessionID, uint localID
struct Vector float x, float y
struct Matrix float m00, float m01, float m02, float m10, float m11, float m12
message Number float value, NumberUnits units
message ParentIndex GUID guid, string position
message FontName string family, string style, string postscript
message TextData string characters
message Effect EffectType type, float radius, float spread, Vector offset, float startRadius, Vector startOffset, Vector endOffset, BlurOpType blurOpType
message StyleId GUID guid
message GUIDPath GUID[] guids
message SymbolData GUID symbolID, NodeChange[] symbolOverrides, float uniformScaleFactor
message VectorData uint vectorNetworkBlob, Vector normalizedSize, NodeChange[] styleOverrideTable
message NodeChange GUID guid, ParentIndex parentIndex, NodeType type, string name, Vector size, Matrix transform, float cornerRadius, float strokeWeight, float[] dashPattern, float fontSize, FontName fontName, TextData textData, SymbolData symbolData, StackMode stackMode, float stackSpacing, float stackHorizontalPadding, float stackVerticalPadding, float stackCounterSpacing, Effect[] effects, Number lineHeight, Number letterSpacing, NodeChange[] derivedSymbolData, GUIDPath guidPath, StyleType styleType, StyleId styleIdForEffect, StyleId styleIdForText, VectorData vectorData, uint styleID
message Blob byte[] bytes
message Message NodeChange[] nodeChanges, Blob[] blobs
";

fn schema() -> Schema {
    Schema::parse(SCHEMA).unwrap()
}

fn import_nodes(nodes: Vec<Kv<'static>>) -> Document {
    import(&canvas(&schema(), &o(vec![("nodeChanges", arr(nodes))])), "x").unwrap().0
}

fn page() -> Vec<Kv<'static>> {
    vec![node(0, None, "DOCUMENT", "D", vec![]), node(1, Some((0, "!")), "CANVAS", "P", vec![])]
}

/// A copy of `n` with field `key` set to `value`.
fn set(n: Kv<'static>, key: &'static str, value: Kv<'static>) -> Kv<'static> {
    let Kv::Object(mut fields) = n else { panic!("not a node") };
    fields.retain(|(k, _)| *k != key);
    fields.push((key, value));
    Kv::Object(fields)
}

/// An instance of main `main` with `factor` (none: the field is missing) and extra fields.
fn instance(
    id: u64,
    parent: (u64, &str),
    name: &str,
    main: u64,
    size: (f64, f64),
    factor: Option<f64>,
    mut more: Vec<(&'static str, Kv<'static>)>,
) -> Kv<'static> {
    let mut symbol = vec![("symbolID", g(main))];
    if let Some(k) = factor {
        symbol.push(("uniformScaleFactor", f(k)));
    }
    let mut fields = vec![("size", xy(size.0, size.1)), ("transform", at(300.0, 0.0)), ("symbolData", o(symbol))];
    fields.append(&mut more);
    node(id, Some(parent), "INSTANCE", name, fields)
}

/// A 256 px component "Icon" holding a 200 px rectangle at (20, 40) with radius 64 and a stroke,
/// dashes and a shadow.
fn icon() -> Vec<Kv<'static>> {
    let shadow = o(vec![("type", e("DROP_SHADOW")), ("radius", f(20.0)), ("spread", f(4.0)), ("offset", xy(10.0, 6.0))]);
    vec![
        node(5, Some((1, "!")), "SYMBOL", "Icon", vec![("size", xy(256.0, 256.0)), ("transform", at(0.0, 0.0))]),
        node(
            6,
            Some((5, "!")),
            "ROUNDED_RECTANGLE",
            "Mark",
            vec![
                ("size", xy(200.0, 200.0)),
                ("transform", at(20.0, 40.0)),
                ("cornerRadius", f(64.0)),
                ("strokeWeight", f(8.0)),
                ("dashPattern", arr(vec![f(16.0), f(8.0)])),
                ("effects", arr(vec![shadow])),
            ],
        ),
    ]
}

fn find<'d>(doc: &'d Document, name: &str) -> &'d Node {
    doc.nodes().find(|n| n.name == name).unwrap_or_else(|| panic!("no layer {name}"))
}

/// The first child of the layer named `name`.
fn first_child<'d>(doc: &'d Document, name: &str) -> &'d Node {
    doc.node(find(doc, name).children[0]).unwrap()
}

fn mark(doc: &Document, name: &str) -> ([f64; 2], [f64; 2], [f64; 4]) {
    let m = first_child(doc, name);
    let t = m.transform.translation();
    ([m.width, m.height], [t.x, t.y], m.corners.radii)
}

#[test]
fn a_scaled_instance_scales_the_lengths_of_a_layer_it_gives_no_row_for() {
    let mut nodes = page();
    nodes.extend(icon());
    nodes.push(instance(9, (1, "\""), "Half", 5, (128.0, 128.0), Some(0.5), vec![]));
    let doc = import_nodes(nodes);
    let (size, place, radii) = mark(&doc, "Half");
    assert_eq!((size, place, radii), ([100.0, 100.0], [10.0, 20.0], [32.0; 4]));
    let m = first_child(&doc, "Half");
    assert_eq!(m.stroke.weight, 4.0);
    assert_eq!(m.stroke.dashes, [8.0, 4.0]);
    let EffectKind::DropShadow(sh) = &m.effects[0].kind else { panic!("not a drop shadow") };
    assert_eq!((sh.radius, sh.spread, sh.offset), (10.0, 2.0, (5.0, 3.0)));
    // The instance itself is at the size the file gives it.
    let inst = find(&doc, "Half");
    assert_eq!([inst.width, inst.height], [128.0, 128.0]);
    // The main is untouched.
    assert_eq!(first_child(&doc, "Icon").corners.radii, [64.0; 4]);
}

#[test]
fn a_value_a_derived_row_gives_is_not_scaled_twice() {
    let mut nodes = page();
    nodes.extend(icon());
    let row = o(vec![("guidPath", o(vec![("guids", arr(vec![g(6)]))])), ("size", xy(100.0, 100.0))]);
    nodes.push(instance(9, (1, "\""), "Half", 5, (128.0, 128.0), Some(0.5), vec![("derivedSymbolData", arr(vec![row]))]));
    let doc = import_nodes(nodes);
    let (size, place, radii) = mark(&doc, "Half");
    // The derived size is taken as written; the radius and place, which no row gives, scale.
    assert_eq!((size, place, radii), ([100.0, 100.0], [10.0, 20.0], [32.0; 4]));
}

#[test]
fn an_override_row_is_not_scaled_either() {
    let mut nodes = page();
    nodes.extend(icon());
    let row = o(vec![("guidPath", o(vec![("guids", arr(vec![g(6)]))])), ("cornerRadius", f(7.0)), ("strokeWeight", f(3.0))]);
    let symbol = o(vec![("symbolID", g(5)), ("uniformScaleFactor", f(0.5)), ("symbolOverrides", arr(vec![row]))]);
    nodes.push(node(9, Some((1, "\"")), "INSTANCE", "Half", vec![("size", xy(128.0, 128.0)), ("transform", at(300.0, 0.0)), ("symbolData", symbol)]));
    let doc = import_nodes(nodes);
    let m = first_child(&doc, "Half");
    assert_eq!(m.corners.radii, [7.0; 4]);
    assert_eq!(m.stroke.weight, 3.0);
    // What the override leaves alone still scales.
    assert_eq!(m.stroke.dashes, [8.0, 4.0]);
}

#[test]
fn a_scaled_auto_layout_instance_scales_its_gap_padding_and_text() {
    let mut nodes = page();
    nodes.push(node(
        5,
        Some((1, "!")),
        "SYMBOL",
        "Row",
        vec![
            ("size", xy(200.0, 60.0)),
            ("transform", at(0.0, 0.0)),
            ("stackMode", e("HORIZONTAL")),
            ("stackSpacing", f(20.0)),
            ("stackCounterSpacing", f(6.0)),
            ("stackHorizontalPadding", f(10.0)),
            ("stackVerticalPadding", f(10.0)),
        ],
    ));
    let font = o(vec![("family", s("Inter")), ("style", s("Regular")), ("postscript", s(""))]);
    let px = |v: f64| o(vec![("value", f(v)), ("units", e("PIXELS"))]);
    nodes.push(node(
        6,
        Some((5, "!")),
        "TEXT",
        "Label",
        vec![
            ("size", xy(40.0, 20.0)),
            ("transform", at(10.0, 10.0)),
            ("fontSize", f(16.0)),
            ("fontName", font),
            ("lineHeight", px(24.0)),
            ("letterSpacing", px(2.0)),
            ("textData", o(vec![("characters", s("Hi"))])),
        ],
    ));
    nodes.push(instance(9, (1, "\""), "RowHalf", 5, (100.0, 30.0), Some(0.5), vec![]));
    let doc = import_nodes(nodes);
    let row = find(&doc, "RowHalf");
    let layout = row.layout.as_ref().expect("the instance has its main's auto layout");
    assert_eq!((layout.gap, layout.cross_gap, layout.padding), (10.0, 3.0, [5.0; 4]));
    let NodeKind::Text(t) = &first_child(&doc, "RowHalf").kind else { panic!("not text") };
    assert_eq!(t.style.font_size, 8.0);
    assert_eq!(t.style.line_height, LineHeight::Pixels(12.0));
    assert_eq!(t.style.letter_spacing, LetterSpacing::Pixels(1.0));
    // The main keeps its own.
    let NodeKind::Text(main) = &first_child(&doc, "Row").kind else { panic!("not text") };
    assert_eq!(main.style.font_size, 16.0);
}

#[test]
fn nested_scaled_instances_compose_their_factors() {
    let mut nodes = page();
    // Inner: 200 px with a 200 px rectangle of radius 40.
    nodes.push(node(5, Some((1, "!")), "SYMBOL", "Inner", vec![("size", xy(200.0, 200.0)), ("transform", at(0.0, 0.0))]));
    nodes.push(node(
        6,
        Some((5, "!")),
        "ROUNDED_RECTANGLE",
        "Dot",
        vec![("size", xy(200.0, 200.0)), ("transform", at(0.0, 0.0)), ("cornerRadius", f(40.0))],
    ));
    // Outer: 200 px, holding Inner scaled to 100 px (factor 0.5) at (20, 20).
    nodes.push(node(7, Some((1, "\"")), "SYMBOL", "Outer", vec![("size", xy(200.0, 200.0)), ("transform", at(300.0, 0.0))]));
    nodes.push(set(instance(8, (7, "!"), "Kid", 5, (100.0, 100.0), Some(0.5), vec![]), "transform", at(20.0, 20.0)));
    // The outer one scaled to 100 px (factor 0.5).
    nodes.push(instance(9, (1, "#"), "Half", 7, (100.0, 100.0), Some(0.5), vec![]));
    let doc = import_nodes(nodes);
    let kid = first_child(&doc, "Half");
    // The nested instance itself is the outer factor times its stored size and place.
    assert_eq!(([kid.width, kid.height], [kid.transform.translation().x, kid.transform.translation().y]), ([50.0, 50.0], [10.0, 10.0]));
    // Its layers take both factors: 0.5 x 0.5 of the main's.
    let dot = doc.node(kid.children[0]).unwrap();
    assert_eq!(([dot.width, dot.height], dot.corners.radii), ([50.0, 50.0], [10.0; 4]));
}

#[test]
fn a_factor_of_one_or_none_changes_nothing() {
    let mut nodes = page();
    nodes.extend(icon());
    nodes.push(instance(9, (1, "\""), "One", 5, (256.0, 256.0), Some(1.0), vec![]));
    nodes.push(instance(10, (1, "#"), "None", 5, (256.0, 256.0), None, vec![]));
    let doc = import_nodes(nodes);
    for name in ["One", "None"] {
        let (size, place, radii) = mark(&doc, name);
        assert_eq!((size, place, radii), ([200.0, 200.0], [20.0, 40.0], [64.0; 4]), "{name}");
        assert_eq!(first_child(&doc, name).stroke.weight, 8.0, "{name}");
    }
}

#[test]
fn a_junk_factor_is_ignored() {
    // The file encoder refuses NaN and infinities; the range check turns those away too.
    for factor in [-0.5, 0.0, 1e30, 3e38, 1e-30] {
        let mut nodes = page();
        nodes.extend(icon());
        nodes.push(instance(9, (1, "\""), "Junk", 5, (256.0, 256.0), Some(factor), vec![]));
        let doc = import_nodes(nodes);
        let (size, place, radii) = mark(&doc, "Junk");
        assert_eq!((size, place, radii), ([200.0, 200.0], [20.0, 40.0], [64.0; 4]), "factor {factor}");
    }
}

#[test]
fn a_nested_instances_own_rows_are_at_its_enclosing_mains_scale() {
    let mut nodes = page();
    // Inner: 200 px with a 200 px rectangle "Dot" of radius 40.
    nodes.push(node(5, Some((1, "!")), "SYMBOL", "Inner", vec![("size", xy(200.0, 200.0)), ("transform", at(0.0, 0.0))]));
    nodes.push(node(
        6,
        Some((5, "!")),
        "ROUNDED_RECTANGLE",
        "Dot",
        vec![("size", xy(200.0, 200.0)), ("transform", at(0.0, 0.0)), ("cornerRadius", f(40.0))],
    ));
    // Outer: 200 px, holding Kid, an instance of Inner at 100 px (factor 0.5). Kid has the derived
    // row Figma writes for every nested instance and an override of its own.
    nodes.push(node(7, Some((1, "\"")), "SYMBOL", "Outer", vec![("size", xy(200.0, 200.0)), ("transform", at(300.0, 0.0))]));
    let dot = |more: Vec<(&'static str, Kv<'static>)>| {
        let mut fields = vec![("guidPath", o(vec![("guids", arr(vec![g(6)]))]))];
        fields.extend(more);
        o(fields)
    };
    let symbol = o(vec![("symbolID", g(5)), ("uniformScaleFactor", f(0.5)), ("symbolOverrides", arr(vec![dot(vec![("cornerRadius", f(7.0))])]))]);
    let kid = node(
        8,
        Some((7, "!")),
        "INSTANCE",
        "Kid",
        vec![
            ("size", xy(100.0, 100.0)),
            ("transform", at(20.0, 20.0)),
            ("symbolData", symbol),
            ("derivedSymbolData", arr(vec![dot(vec![("size", xy(100.0, 100.0))])])),
        ],
    );
    nodes.push(kid);
    // Half: Outer at factor 0.5.
    nodes.push(instance(9, (1, "#"), "Half", 7, (100.0, 100.0), Some(0.5), vec![]));
    // Whole: Outer at its own size, for the unscaled reading.
    nodes.push(instance(10, (1, "$"), "Whole", 7, (200.0, 200.0), None, vec![]));
    let doc = import_nodes(nodes);
    let dot_of = |name: &str| {
        let kid = first_child(&doc, name);
        doc.node(kid.children[0]).unwrap()
    };
    // Both rows are in Outer's space: Half draws them at half.
    let half = dot_of("Half");
    assert_eq!(([half.width, half.height], half.corners.radii), ([50.0, 50.0], [3.5; 4]));
    let whole = dot_of("Whole");
    assert_eq!(([whole.width, whole.height], whole.corners.radii), ([100.0, 100.0], [7.0; 4]));
}

#[test]
fn a_progressive_blurs_positions_are_not_lengths() {
    let mut nodes = page();
    let blur = o(vec![
        ("type", e("FOREGROUND_BLUR")),
        ("radius", f(8.0)),
        ("blurOpType", e("PROGRESSIVE")),
        ("startRadius", f(2.0)),
        ("startOffset", xy(0.5, 0.0)),
        ("endOffset", xy(0.5, 1.0)),
    ]);
    nodes.push(node(5, Some((1, "!")), "SYMBOL", "Icon", vec![("size", xy(256.0, 256.0)), ("transform", at(0.0, 0.0))]));
    nodes.push(node(
        6,
        Some((5, "!")),
        "ROUNDED_RECTANGLE",
        "Mark",
        vec![("size", xy(200.0, 200.0)), ("transform", at(0.0, 0.0)), ("effects", arr(vec![blur]))],
    ));
    nodes.push(instance(9, (1, "\""), "Half", 5, (128.0, 128.0), Some(0.5), vec![]));
    let doc = import_nodes(nodes);
    let EffectKind::LayerBlur(b) = &first_child(&doc, "Half").effects[0].kind else { panic!("not a layer blur") };
    // The radii are lengths and scale; the ramp's ends are fractions of the layer and do not.
    assert_eq!((b.radius, b.start_radius, b.start, b.end), (4.0, 1.0, (0.5, 0.0), (0.5, 1.0)));
}

#[test]
fn an_effect_style_takes_the_scale_of_the_layer_that_links_it() {
    let mut nodes = page();
    let shadow = |radius: f64| o(vec![("type", e("DROP_SHADOW")), ("radius", f(radius))]);
    let link = |id: u64| o(vec![("guid", g(id))]);
    nodes.push(node(30, Some((1, "~")), "ROUNDED_RECTANGLE", "Soft", vec![("styleType", e("EFFECT")), ("effects", arr(vec![shadow(10.0)]))]));
    nodes.push(node(31, Some((1, "}")), "ROUNDED_RECTANGLE", "Hard", vec![("styleType", e("EFFECT")), ("effects", arr(vec![shadow(10.0)]))]));
    nodes.push(node(5, Some((1, "!")), "SYMBOL", "Icon", vec![("size", xy(256.0, 256.0)), ("transform", at(0.0, 0.0))]));
    nodes.push(node(
        6,
        Some((5, "!")),
        "ROUNDED_RECTANGLE",
        "Mark",
        vec![("size", xy(200.0, 200.0)), ("transform", at(0.0, 0.0)), ("styleIdForEffect", link(30))],
    ));
    // Kept: the main's own layer links the style, so the instance's factor applies.
    nodes.push(instance(9, (1, "\""), "Kept", 5, (128.0, 128.0), Some(0.5), vec![]));
    // Swapped: a row of the instance links another style, which is at the instance's scale.
    let row = o(vec![("guidPath", o(vec![("guids", arr(vec![g(6)]))])), ("styleIdForEffect", link(31))]);
    let symbol = o(vec![("symbolID", g(5)), ("uniformScaleFactor", f(0.5)), ("symbolOverrides", arr(vec![row]))]);
    nodes.push(node(
        10,
        Some((1, "#")),
        "INSTANCE",
        "Swapped",
        vec![("size", xy(128.0, 128.0)), ("transform", at(600.0, 0.0)), ("symbolData", symbol)],
    ));
    let doc = import_nodes(nodes);
    let radius = |name: &str| {
        let EffectKind::DropShadow(sh) = &first_child(&doc, name).effects[0].kind else { panic!("not a drop shadow") };
        sh.radius
    };
    assert_eq!((radius("Kept"), radius("Swapped")), (5.0, 10.0));
}

fn point_radius_vector_data(radius: f64) -> Kv<'static> {
    o(vec![
        ("vectorNetworkBlob", f(0.0)),
        ("normalizedSize", xy(200.0, 200.0)),
        ("styleOverrideTable", arr(vec![o(vec![("styleID", f(1.0)), ("cornerRadius", f(radius))])])),
    ])
}

fn point_radius_blob() -> Vec<u8> {
    use super::network::{Network, Region, Segment, Vertex};
    super::network::encode(&Network {
        vertices: vec![Vertex { style: 1, x: 0.0, y: 0.0 }, Vertex { style: 1, x: 200.0, y: 0.0 }, Vertex { style: 1, x: 0.0, y: 200.0 }],
        segments: vec![
            Segment { style: 0, a: 0, b: 1, ta: (0.0, 0.0), tb: (0.0, 0.0) },
            Segment { style: 0, a: 1, b: 2, ta: (0.0, 0.0), tb: (0.0, 0.0) },
            Segment { style: 0, a: 2, b: 0, ta: (0.0, 0.0), tb: (0.0, 0.0) },
        ],
        regions: vec![Region { style: 0, non_zero: true, loops: vec![vec![0, 1, 2]] }],
    })
}

fn point_radius_nodes() -> Vec<Kv<'static>> {
    let mut nodes = page();
    nodes.push(node(5, Some((1, "!")), "SYMBOL", "Corner main", vec![("size", xy(200.0, 200.0)), ("transform", at(0.0, 0.0))]));
    nodes.push(node(
        6,
        Some((5, "!")),
        "VECTOR",
        "Rounded vector",
        vec![("size", xy(200.0, 200.0)), ("transform", at(0.0, 0.0)), ("vectorData", point_radius_vector_data(40.0))],
    ));
    nodes
}

fn point_radius_import(nodes: Vec<Kv<'static>>) -> Document {
    let message = o(vec![("nodeChanges", arr(nodes)), ("blobs", arr(vec![o(vec![("bytes", Kv::Bytes(point_radius_blob()))])]))]);
    let (doc, issues) = import(&canvas(&schema(), &message), "radius").unwrap();
    assert!(!issues.iter().any(|issue| issue.contains("damaged")), "{issues:?}");
    doc
}

fn first_point_radius(doc: &Document, parent: &str) -> f64 {
    let NodeKind::Vector { network } = &first_child(doc, parent).kind else { panic!("not an editable vector") };
    assert_eq!(network.vertices.len(), 3);
    network.vertices[0].radius
}

#[test]
fn scaled_instance_scales_per_point_radii_from_the_main_vector_data() {
    let mut nodes = point_radius_nodes();
    nodes.push(instance(9, (1, "\""), "Half radius", 5, (100.0, 100.0), Some(0.5), vec![]));
    let doc = point_radius_import(nodes);
    assert_eq!(first_point_radius(&doc, "Half radius"), 20.0);
    assert_eq!(first_point_radius(&doc, "Corner main"), 40.0, "the main is unchanged");
}

#[test]
fn scaled_instance_keeps_per_point_radii_from_an_already_scaled_override_row() {
    let mut nodes = point_radius_nodes();
    let row = o(vec![("guidPath", o(vec![("guids", arr(vec![g(6)]))])), ("vectorData", point_radius_vector_data(7.0))]);
    nodes.push(instance(9, (1, "\""), "Own radius", 5, (100.0, 100.0), Some(0.5), vec![("derivedSymbolData", arr(vec![row]))]));
    let doc = point_radius_import(nodes);
    assert_eq!(first_point_radius(&doc, "Own radius"), 7.0, "the override table is already at instance scale");
}

#[test]
fn nested_scaled_instances_compose_per_point_radius_scale() {
    let mut nodes = point_radius_nodes();
    nodes.push(node(8, Some((1, "\"")), "SYMBOL", "Outer radius", vec![("size", xy(200.0, 200.0)), ("transform", at(0.0, 0.0))]));
    nodes.push(instance(9, (8, "!"), "Inner half", 5, (100.0, 100.0), Some(0.5), vec![]));
    nodes.push(instance(10, (1, "#"), "Quarter radius", 8, (100.0, 100.0), Some(0.5), vec![]));
    let doc = point_radius_import(nodes);
    let nested = first_child(&doc, "Quarter radius");
    let child = doc.node(nested.children[0]).unwrap();
    let NodeKind::Vector { network } = &child.kind else { panic!("not a vector") };
    assert_eq!(network.vertices[0].radius, 10.0);
}

const SCALE_OBSERVATION_KEY: &str = "figcraft.import.fig.scale-provenance.v1";

fn observation(node: &Node) -> serde_json::Value {
    serde_json::from_str(node.plugin_data.get(SCALE_OBSERVATION_KEY).expect("descriptive scale provenance")).unwrap()
}

fn observed_field<'v>(value: &'v serde_json::Value, name: &str) -> &'v serde_json::Value {
    value["fields"].as_array().unwrap().iter().find(|field| field["name"] == name).unwrap_or_else(|| panic!("missing observed field {name}"))
}

#[test]
fn imported_mixed_fields_keep_their_distinct_supplier_rows_without_rescaling_them() {
    let mut nodes = page();
    nodes.extend(icon());
    let row = o(vec![("guidPath", o(vec![("guids", arr(vec![g(6)]))])), ("cornerRadius", f(7.0))]);
    let symbol = o(vec![("symbolID", g(5)), ("uniformScaleFactor", f(0.5)), ("symbolOverrides", arr(vec![row]))]);
    nodes.push(node(9, Some((1, "\"")), "INSTANCE", "Mixed provenance", vec![("size", xy(128.0, 128.0)), ("symbolData", symbol)]));
    let doc = import_nodes(nodes);
    let mark = first_child(&doc, "Mixed provenance");
    assert_eq!((mark.corners.radii[0], mark.stroke.weight), (7.0, 4.0), "physical import policy stays unchanged");
    let record = observation(mark);
    assert_eq!(record["authority"], "descriptive-only");
    let radius = observed_field(&record, "cornerRadius");
    assert_eq!(radius["supplier"]["scale"], 1.0);
    assert_eq!(radius["supplier"]["kind"], "override");
    assert_eq!(radius["supplier"]["ownerGuid"], "1:9");
    assert_eq!(radius["supplier"]["ordinal"], 0);
    let stroke = observed_field(&record, "strokeWeight");
    assert_eq!(stroke["supplier"]["scale"], 0.5);
    assert_eq!(stroke["supplier"]["rowGuid"], "1:6");
}

#[test]
fn shared_text_style_and_auto_layout_fallback_keep_supplier_and_link_contexts() {
    let mut nodes = page();
    nodes.push(node(
        5,
        Some((1, "!")),
        "SYMBOL",
        "Styled row",
        vec![("size", xy(200.0, 60.0)), ("stackMode", e("HORIZONTAL")), ("stackSpacing", f(20.0))],
    ));
    nodes.push(node(50, None, "TEXT", "Shared type", vec![("styleType", e("TEXT")), ("fontSize", f(16.0))]));
    nodes.push(node(
        6,
        Some((5, "!")),
        "TEXT",
        "Styled label",
        vec![
            ("size", xy(40.0, 20.0)),
            ("fontSize", f(99.0)),
            ("textData", o(vec![("characters", s("Hi"))])),
            ("styleIdForText", o(vec![("guid", g(50))])),
        ],
    ));
    nodes.push(instance(9, (1, "\""), "Styled half", 5, (100.0, 30.0), Some(0.5), vec![]));
    let doc = import_nodes(nodes);
    let row = observation(find(&doc, "Styled half"));
    let gap = observed_field(&row, "stackSpacing");
    assert_eq!(gap["supplier"]["scale"], 0.5);
    assert_eq!(gap["supplier"]["rowGuid"], "1:5", "the transient main-backed layout view is retained");
    let text = observation(first_child(&doc, "Styled half"));
    let font = observed_field(&text, "fontSize");
    assert_eq!(font["styleGuid"], "1:50");
    assert_eq!(font["supplier"]["kind"], "shared-style");
    assert_eq!(font["supplier"]["rowGuid"], "1:50");
    assert_eq!(font["supplier"]["scale"], 0.5);
    assert_eq!(font["linkedBy"]["rowGuid"], "1:6");
    assert_eq!(font["linkedBy"]["scale"], 0.5);
}

#[test]
fn nested_scale_observations_separate_local_relative_from_enclosing_and_children_cumulative() {
    let mut nodes = point_radius_nodes();
    nodes.push(node(8, Some((1, "\"")), "SYMBOL", "Provenance outer", vec![("size", xy(200.0, 200.0))]));
    nodes.push(instance(9, (8, "!"), "Provenance inner", 5, (100.0, 100.0), Some(0.5), vec![]));
    nodes.push(instance(10, (1, "#"), "Provenance quarter", 8, (100.0, 100.0), Some(0.5), vec![]));
    let doc = point_radius_import(nodes);
    let inner = first_child(&doc, "Provenance quarter");
    let record = observation(inner);
    assert_eq!(record["enclosingCumulative"], 0.5);
    assert_eq!(record["localRelative"]["effective"], 0.5);
    assert_eq!(record["childrenCumulative"], 0.25);
    let descendant = observation(doc.node(inner.children[0]).unwrap());
    assert_eq!(descendant["enclosingCumulative"], 0.25);
    assert!(descendant.get("localRelative").is_none(), "a leaf does not pretend that a cumulative factor is its relative factor");
}

#[test]
fn descriptive_supplier_records_survive_native_save_without_losing_existing_plugin_data() {
    let mut nodes = page();
    nodes.extend(icon());
    nodes.push(instance(9, (1, "\""), "Persisted half", 5, (128.0, 128.0), Some(0.5), vec![]));
    let mut doc = import_nodes(nodes);
    let id = first_child(&doc, "Persisted half").id;
    doc.node_mut(id).unwrap().plugin_data.insert("existing-agent-note".into(), "keep byte-for-byte".into());
    let expected = doc.node(id).unwrap().plugin_data.clone();
    let bytes = crate::save_bytes(&doc, None).unwrap();
    let (reopened, issues) = crate::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(reopened.node(id).unwrap().plugin_data, expected);
    assert_eq!(observation(reopened.node(id).unwrap())["authority"], "descriptive-only");
}

fn forged_observation_json() -> serde_json::Value {
    let payload = r#"{"version":1,"authority":"descriptive-only","originGuid":"1:9","restId":"1:9","enclosingCumulative":0.5,"childClampApplied":false,"fittedGeometry":false,"fields":[]}"#;
    serde_json::json!({"document": {"id": "0:0", "type": "DOCUMENT", "children": [
        {"id": "0:1", "type": "CANVAS", "children": [
            {"id": "1:9", "type": "RECTANGLE", "name": "Untrusted carrier", "size": {"x": 10, "y": 10},
             "_figSource": {"origin": "1:9", "expanded": false, "scaleProvenance": payload}}
        ]}
    ]}})
}

#[test]
fn ordinary_rest_cannot_admit_the_private_scale_observation_carrier() {
    let (doc, _) = crate::rest::import(&forged_observation_json(), &|_| None).unwrap();
    let node = find(&doc, "Untrusted carrier");
    assert!(!node.plugin_data.contains_key(SCALE_OBSERVATION_KEY));
    assert_eq!(node.width, 10.0);
}

#[test]
fn private_import_stores_observations_but_does_not_apply_them_as_native_context() {
    let (doc, _) = crate::rest::import_fig(&forged_observation_json(), &|_| None).unwrap();
    let node = find(&doc, "Untrusted carrier");
    assert_eq!(observation(node)["enclosingCumulative"], 0.5);
    assert_eq!((node.width, node.height), (10.0, 10.0), "descriptive metadata never blindly rescales a snapshot");
}

#[test]
fn invalid_private_scale_observations_warn_but_keep_the_imported_layer_unchanged() {
    let mut json = forged_observation_json();
    let source = &mut json["document"]["children"][0]["children"][0]["_figSource"];
    source["scaleProvenance"] = serde_json::json!(source["scaleProvenance"].as_str().unwrap().replace("descriptive-only", "native-authority"));
    let (doc, warnings) = crate::rest::import_fig(&json, &|_| None).unwrap();
    let node = find(&doc, "Untrusted carrier");
    assert_eq!((node.width, node.height), (10.0, 10.0));
    assert!(!node.plugin_data.contains_key(SCALE_OBSERVATION_KEY));
    assert!(warnings.iter().any(|warning| warning.contains("scale provenance")), "{warnings:?}");
}

fn provenance_nested_list_writer(symbol_override: bool) {
    let mut nodes = page();
    nodes.extend(icon());
    nodes.push(node(7, Some((1, "\"")), "SYMBOL", "Writer outer main", vec![("size", xy(256.0, 256.0))]));
    nodes.push(instance(8, (7, "!"), "Writer nested instance", 5, (256.0, 256.0), None, vec![]));
    let leaf = o(vec![("guidPath", o(vec![("guids", arr(vec![g(6)]))])), ("cornerRadius", f(7.0))]);
    let supplied = if symbol_override {
        ("symbolData", o(vec![("symbolID", g(5)), ("symbolOverrides", arr(vec![leaf]))]))
    } else {
        ("derivedSymbolData", arr(vec![leaf]))
    };
    let nested = o(vec![("guidPath", o(vec![("guids", arr(vec![g(8)]))])), supplied]);
    let symbol = o(vec![("symbolID", g(7)), ("uniformScaleFactor", f(0.5)), ("symbolOverrides", arr(vec![nested]))]);
    nodes.push(node(9, Some((1, "#")), "INSTANCE", "Actual outer writer", vec![("size", xy(256.0, 256.0)), ("symbolData", symbol)]));
    let doc = import_nodes(nodes);
    let nested = first_child(&doc, "Actual outer writer");
    let leaf = doc.node(nested.children[0]).unwrap();
    assert_eq!(leaf.corners.radii, [7.0; 4], "writer provenance does not change physical override values");
    let record = observation(leaf);
    let radius = observed_field(&record, "cornerRadius");
    assert_eq!(radius["supplier"]["ownerGuid"], "1:9", "the effective list was written on outer instance9, not the base nested wire8");
    assert_eq!(radius["supplier"]["kind"], if symbol_override { "override" } else { "derived" });
    assert_eq!(radius["supplier"]["scale"], 1.0);
    assert_eq!(first_child(&doc, "Icon").corners.radii, [64.0; 4], "the shared main remains unchanged");
}

#[test]
fn provenance_nested_derived_list_tracks_the_actual_outer_writer() {
    provenance_nested_list_writer(false);
}

#[test]
fn provenance_nested_symbol_override_list_tracks_the_actual_outer_writer() {
    provenance_nested_list_writer(true);
}

#[test]
fn provenance_unit_only_instances_and_nested_overrides_do_not_add_or_serialize_history() {
    let mut nodes = page();
    nodes.extend(icon()); // effects is a mixed-unit container, but every factor here is one.
    nodes.push(node(7, Some((1, "\"")), "SYMBOL", "Unit outer main", vec![("size", xy(256.0, 256.0))]));
    nodes.push(instance(8, (7, "!"), "Unit nested", 5, (256.0, 256.0), Some(1.0), vec![]));
    let leaf = o(vec![("guidPath", o(vec![("guids", arr(vec![g(8), g(6)]))])), ("cornerRadius", f(7.0))]);
    nodes.push(node(
        9,
        Some((1, "#")),
        "INSTANCE",
        "Unit outer copy",
        vec![
            ("size", xy(256.0, 256.0)),
            ("symbolData", o(vec![("symbolID", g(7)), ("uniformScaleFactor", f(1.0)), ("symbolOverrides", arr(vec![leaf]))])),
        ],
    ));
    nodes.push(instance(10, (1, "$"), "Absent factor", 5, (256.0, 256.0), None, vec![]));
    nodes.push(instance(11, (1, "%"), "Explicit unit", 5, (256.0, 256.0), Some(1.0), vec![]));
    crate::fig_scale_provenance::test_reset_work();
    let mut doc = import_nodes(nodes);
    assert!(
        doc.nodes().all(|node| !node.plugin_data.contains_key(SCALE_OBSERVATION_KEY)),
        "ordinary unit-only imports must not acquire historical plugin data"
    );
    assert_eq!(
        crate::fig_scale_provenance::test_work(),
        [0; 4],
        "unit-only expansion performs no optional capture, encoding or private payload parsing"
    );
    let nested = first_child(&doc, "Unit outer copy");
    assert_eq!(doc.node(nested.children[0]).unwrap().corners.radii, [7.0; 4]);
    assert_eq!(first_child(&doc, "Absent factor").corners.radii, [64.0; 4]);
    assert_eq!(first_child(&doc, "Explicit unit").stroke.weight, 8.0);
    let id = find(&doc, "Explicit unit").id;
    doc.node_mut(id).unwrap().plugin_data.insert(SCALE_OBSERVATION_KEY.into(), "existing historical bytes remain user data".into());
    doc.node_mut(id).unwrap().plugin_data.insert("unrelated".into(), "unchanged".into());
    let expected = doc.node(id).unwrap().plugin_data.clone();
    let bytes = crate::save_bytes(&doc, None).unwrap();
    let (reopened, issues) = crate::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(reopened.node(id).unwrap().plugin_data, expected, "native load/save never filters existing plugin data");
}

#[test]
fn provenance_inverse_nested_factors_keep_leaf_history_after_the_product_returns_to_one() {
    let mut nodes = page();
    nodes.extend(icon());
    nodes.push(node(7, Some((1, "\"")), "SYMBOL", "Cancel outer main", vec![("size", xy(128.0, 128.0))]));
    nodes.push(instance(8, (7, "!"), "Cancel inner half", 5, (128.0, 128.0), Some(0.5), vec![]));
    nodes.push(instance(9, (1, "#"), "Cancel outer double", 7, (256.0, 256.0), Some(2.0), vec![]));
    let doc = import_nodes(nodes);
    let inner = first_child(&doc, "Cancel outer double");
    let inner_record = observation(inner);
    assert_eq!(inner_record["enclosingCumulative"], 2.0);
    assert_eq!(inner_record["localRelative"]["wire"], 0.5);
    assert_eq!(inner_record["childrenCumulative"], 1.0);
    let leaf = doc.node(inner.children[0]).unwrap();
    assert_eq!(leaf.corners.radii, [64.0; 4]);
    let record = observation(leaf);
    assert_eq!(record["enclosingCumulative"], 1.0);
    assert!(record.get("localRelative").is_none());
    assert_eq!(observed_field(&record, "cornerRadius")["supplier"]["scale"], 1.0);
    assert_eq!(observed_field(&record, "effects")["units"], "mixed", "all admitted branch details remain complete despite cumulative cancellation");
}

#[test]
fn provenance_invalid_explicit_factor_retains_ignored_wire_history_and_unchanged_geometry() {
    for factor in [0.0, -1.0, 2000.0] {
        let mut nodes = page();
        nodes.extend(icon());
        nodes.push(instance(9, (1, "\""), "Ignored factor", 5, (256.0, 256.0), Some(factor), vec![]));
        let doc = import_nodes(nodes);
        let record = observation(find(&doc, "Ignored factor"));
        assert_eq!(record["localRelative"]["wire"], factor);
        assert_eq!(record["localRelative"]["effective"], 1.0);
        let leaf = first_child(&doc, "Ignored factor");
        assert_eq!(leaf.corners.radii, [64.0; 4]);
        assert_eq!(observation(leaf)["enclosingCumulative"], 1.0, "the ignored raw factor remains meaningful historical branch context");
    }
}

#[test]
fn provenance_scaled_branch_keeps_fitted_geometry_and_unknown_text_units() {
    let mut nodes = page();
    nodes.push(node(5, Some((1, "!")), "SYMBOL", "Fit type main", vec![("size", xy(200.0, 60.0))]));
    nodes.push(node(
        6,
        Some((5, "!")),
        "TEXT",
        "Unknown type units",
        vec![
            ("size", xy(40.0, 20.0)),
            ("transform", at(10.0, 10.0)),
            ("fontSize", f(16.0)),
            ("textData", o(vec![("characters", s("Hi"))])),
            ("lineHeight", o(vec![("value", f(20.0))])),
        ],
    ));
    nodes.push(instance(9, (1, "\""), "Fit half", 5, (150.0, 30.0), Some(0.5), vec![]));
    let doc = import_nodes(nodes);
    let leaf = first_child(&doc, "Fit half");
    let NodeKind::Text(text) = &leaf.kind else { panic!("not text") };
    assert_eq!(text.style.font_size, 8.0);
    let record = observation(leaf);
    assert_eq!(record["fittedGeometry"], true);
    assert_eq!(observed_field(&record, "lineHeight")["units"], "unknown");
    assert_eq!(observed_field(&record, "size")["resolution"], "computed-fit");
    assert_eq!(observed_field(&record, "fontSize")["supplier"]["scale"], 0.5);
}
