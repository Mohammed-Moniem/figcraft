//! The line breaks a `.fig` file saved with its text (`derivedTextData`) come across as the text's
//! saved breaks, on messages built in code.

use figcraft_doc::{Document, TextData};

use super::kiwi::Kv;

type Fields = Vec<(&'static str, Kv<'static>)>;

fn object(fields: Fields) -> Kv<'static> {
    Kv::Object(fields)
}

fn guid(id: u64) -> Kv<'static> {
    object(vec![("sessionID", Kv::Uint(9)), ("localID", Kv::Uint(id))])
}

fn node(id: u64, ty: &'static str, name: &str, parent: u64, mut more: Fields) -> Kv<'static> {
    let mut fields = vec![
        ("guid", guid(id)),
        ("type", Kv::Enum(ty)),
        ("name", Kv::Str(name.into())),
        ("parentIndex", object(vec![("guid", guid(parent)), ("position", Kv::Str("a".into()))])),
    ];
    fields.append(&mut more);
    object(fields)
}

/// The saved layout: one baseline per line, starting at these UTF-16 offsets.
fn derived(firsts: &[u64]) -> Kv<'static> {
    let lines = firsts.iter().map(|f| object(vec![("firstCharacter", Kv::Uint(*f))])).collect();
    object(vec![("baselines", Kv::Array(lines))])
}

/// A 100 × 40 text layer named `name` with `chars`, sizing `resize` and the saved layout `firsts`.
fn label(id: u64, name: &str, parent: u64, chars: &str, resize: &'static str, firsts: &[u64]) -> Kv<'static> {
    node(
        id,
        "TEXT",
        name,
        parent,
        vec![
            ("size", object(vec![("x", Kv::Float(100.0)), ("y", Kv::Float(40.0))])),
            ("fontSize", Kv::Float(14.0)),
            ("textAutoResize", Kv::Enum(resize)),
            ("textData", object(vec![("characters", Kv::Str(chars.into()))])),
            ("derivedTextData", derived(firsts)),
        ],
    )
}

fn import(nodes: Vec<Kv<'static>>) -> Document {
    let page = vec![
        object(vec![("guid", guid(1)), ("type", Kv::Enum("DOCUMENT")), ("name", Kv::Str("Document".into()))]),
        node(2, "CANVAS", "Page", 1, vec![]),
    ];
    let message = object(vec![("nodeChanges", Kv::Array(page.into_iter().chain(nodes).collect()))]);
    let value = super::map::to_rest(&message, "Synthetic wrap", crate::rest::MAX_NODES).0;
    crate::rest::import_fig(&value, &|_| None).unwrap().0
}

fn texts<'d>(doc: &'d Document, name: &str) -> Vec<&'d TextData> {
    doc.nodes().filter(|n| n.name == name).filter_map(|n| n.kind.text()).collect()
}

fn reopen(doc: &Document) -> Document {
    let (bytes, _) = super::export(doc, None).unwrap();
    super::import(&bytes, "Synthetic wrap").unwrap().0
}

#[test]
fn native_saves_keep_valid_wraps_and_drop_them_after_a_resize() {
    let mut doc = import(vec![
        label(10, "Wrapped", 2, "😀 ab cd\nef", "HEIGHT", &[0, 6, 9]),
        label(11, "Unwrapped", 2, "Followers", "NONE", &[0]),
        label(12, "Resized", 2, "ab cd", "HEIGHT", &[0, 3]),
    ]);
    let id = doc.nodes().find(|n| n.name == "Resized").unwrap().id;
    doc.node_mut(id).unwrap().width = 180.0;
    for _ in 0..2 {
        doc = reopen(&doc);
        assert_eq!(texts(&doc, "Wrapped")[0].saved_breaks_for(100.0), Some(&[5][..]));
        assert_eq!(texts(&doc, "Unwrapped")[0].saved_breaks_for(100.0), Some(&[][..]));
        assert!(texts(&doc, "Resized")[0].saved_breaks.is_none());
    }
}

#[test]
fn saved_line_starts_come_across_as_chars_without_hard_breaks() {
    // UTF-16: the emoji takes two units, so "c" is unit 6 but char 5; "e" follows a hard break.
    let doc = import(vec![
        label(10, "Wrapped", 2, "😀 ab cd\nef", "HEIGHT", &[0, 6, 9]),
        label(11, "Unwrapped", 2, "Followers", "NONE", &[0]),
        label(12, "Auto width", 2, "ab cd", "WIDTH_AND_HEIGHT", &[0, 3]),
        label(13, "Inside a pair", 2, "😀 ab", "HEIGHT", &[0, 1]),
        label(14, "Out of order", 2, "ab cd ef", "HEIGHT", &[0, 6, 3]),
        label(15, "Not from the start", 2, "ab cd", "HEIGHT", &[3]),
    ]);
    let wrapped = texts(&doc, "Wrapped")[0];
    assert_eq!(wrapped.saved_breaks_for(100.0), Some(&[5][..]));
    assert_eq!(texts(&doc, "Unwrapped")[0].saved_breaks_for(100.0), Some(&[][..]), "one line: no wrapped start");
    for name in ["Auto width", "Inside a pair", "Out of order", "Not from the start"] {
        assert_eq!(texts(&doc, name)[0].saved_breaks, None, "{name}");
    }
}

#[test]
fn a_saved_layout_for_other_text_is_left_out() {
    let row = object(vec![
        ("guidPath", object(vec![("guids", Kv::Array(vec![guid(21)]))])),
        ("textData", object(vec![("characters", Kv::Str("Something much longer".into()))])),
    ]);
    let doc = import(vec![
        node(20, "SYMBOL", "Main", 2, vec![]),
        label(21, "Label", 20, "Hi there", "HEIGHT", &[0, 3]),
        node(30, "INSTANCE", "Inst", 2, vec![("symbolData", object(vec![("symbolID", guid(20)), ("symbolOverrides", Kv::Array(vec![row]))]))]),
    ]);
    let labels = texts(&doc, "Label");
    let main = labels.iter().find(|t| t.characters == "Hi there").unwrap();
    assert_eq!(main.saved_breaks_for(100.0), Some(&[3][..]));
    // The instance's text comes from its override, the saved layout only from the main.
    let inst = labels.iter().find(|t| t.characters == "Something much longer").unwrap();
    assert_eq!(inst.saved_breaks, None);
    let doc = reopen(&doc);
    let labels = texts(&doc, "Label");
    assert_eq!(labels.iter().find(|t| t.characters == "Hi there").unwrap().saved_breaks_for(100.0), Some(&[3][..]));
    assert!(labels.iter().find(|t| t.characters == "Something much longer").unwrap().saved_breaks.is_none());
}

#[test]
fn native_layout_import_shares_paths_and_skips_stale_instance_overrides() {
    let text = |id, parent| {
        let glyph = object(vec![("commandsBlob", Kv::Uint(0)), ("firstCharacter", Kv::Uint(0)), ("fontSize", Kv::Float(14.0))]);
        let data = object(vec![
            ("glyphs", Kv::Array(vec![glyph.clone(), glyph])),
            ("baselines", Kv::Array(vec![object(vec![("firstCharacter", Kv::Uint(0)), ("endCharacter", Kv::Uint(1))])])),
        ]);
        node(
            id,
            "TEXT",
            "Label",
            parent,
            vec![
                ("textData", object(vec![("characters", Kv::Str("X".into()))])),
                ("fontSize", Kv::Float(14.0)),
                ("size", object(vec![("x", Kv::Float(60.0)), ("y", Kv::Float(18.0))])),
                ("derivedTextData", data),
            ],
        )
    };
    let inst = |id, name, changed: Fields| {
        let mut row = vec![("guidPath", object(vec![("guids", Kv::Array(vec![guid(21)]))]))];
        row.extend(changed);
        node(id, "INSTANCE", name, 2, vec![("symbolData", object(vec![("symbolID", guid(20)), ("symbolOverrides", Kv::Array(vec![object(row)]))]))])
    };
    let nodes = vec![
        node(1, "DOCUMENT", "Doc", 0, vec![]),
        node(2, "CANVAS", "Page", 1, vec![]),
        node(20, "SYMBOL", "Main", 2, vec![]),
        text(21, 20),
        text(22, 2),
        inst(30, "Intact", vec![]),
        inst(31, "Characters", vec![("textData", object(vec![("characters", Kv::Str("Y".into()))]))]),
        inst(32, "Font size", vec![("fontSize", Kv::Float(20.0))]),
        inst(33, "Resize", vec![("size", object(vec![("x", Kv::Float(50.0)), ("y", Kv::Float(18.0))]))]),
        inst(34, "Same size", vec![("size", object(vec![("x", Kv::Float(60.0)), ("y", Kv::Float(18.0))]))]),
        inst(35, "Restated text", vec![("textData", object(vec![("characters", Kv::Str("X".into()))])), ("fontSize", Kv::Float(14.0))]),
    ];
    let valid_path = vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let msg = object(vec![("nodeChanges", Kv::Array(nodes)), ("blobs", Kv::Array(vec![object(vec![("bytes", Kv::Bytes(valid_path))])]))]);
    let (json, warnings, networks, layouts) = super::map::to_rest_with_layouts(&msg, "Text layouts", crate::rest::MAX_NODES);
    assert!(warnings.is_empty(), "{warnings:?}");
    let (back, _) = crate::rest::import_with_native_motion(&json, &|_| None, networks, crate::rest::MAX_NODES, Default::default(), layouts).unwrap();
    let mut shared = Vec::new();
    for n in back.nodes().filter(|n| n.name == "Label") {
        let t = n.kind.text().unwrap();
        let parent = back.node(n.parent.unwrap()).unwrap();
        let actual = t.native_layout(n.width, n.height);
        if ["Characters", "Font size", "Resize"].contains(&parent.name.as_str()) {
            assert!(actual.is_none(), "stale inherited cache on {}", parent.name);
            assert!(t.saved_breaks.is_none(), "stale inherited line breaks on {}", parent.name);
        } else {
            let layout = actual.unwrap();
            assert_eq!(layout.outlines.len(), 1);
            assert_eq!(layout.glyphs[0].outline, layout.glyphs[1].outline);
            shared.push(&layout.outlines[0]);
        }
    }
    assert_eq!(shared.len(), 5);
    assert!(shared.windows(2).all(|pair| std::sync::Arc::ptr_eq(pair[0], pair[1])));
}

#[test]
fn native_font_import_keeps_original_postscript_and_digest_without_guessing_missing_fields() {
    let name =
        |ps: &str| object(vec![("family", Kv::Str("Example".into())), ("style", Kv::Str("Semibold".into())), ("postscript", Kv::Str(ps.into()))]);
    let glyph = object(vec![("firstCharacter", Kv::Uint(0)), ("fontSize", Kv::Float(14.0))]);
    let source_meta = object(vec![
        ("key", name("")),
        ("fontDigest", Kv::Bytes(vec![17; 20])),
        ("fontWeight", Kv::Int(600)),
        ("fontLineHeight", Kv::Float(1.2)),
        ("fontStyle", Kv::Enum("NORMAL")),
    ]);
    let label = |id, label_name, font: Option<Kv<'static>>, meta: Option<Kv<'static>>| {
        let mut derived = vec![("glyphs", Kv::Array(vec![glyph.clone()]))];
        if let Some(meta) = meta {
            derived.extend([
                ("fontMetaData", Kv::Array(vec![meta])),
                ("derivedLines", Kv::Array(vec![object(vec![("directionality", Kv::Enum("RTL"))])])),
                ("logicalIndexToCharacterOffsetMap", Kv::Array(vec![Kv::Float(0.0), Kv::Float(8.25)])),
                ("truncationStartIndex", Kv::Int(-1)),
                ("truncatedHeight", Kv::Float(-1.0)),
                (
                    "decorations",
                    Kv::Array(vec![object(vec![
                        ("styleID", Kv::Uint(0)),
                        (
                            "rects",
                            Kv::Array(vec![object(vec![
                                ("x", Kv::Float(1.25)),
                                ("y", Kv::Float(5.0)),
                                ("w", Kv::Float(8.0)),
                                ("h", Kv::Float(1.0)),
                            ])]),
                        ),
                    ])]),
                ),
            ]);
        }
        let mut fields = vec![("textData", object(vec![("characters", Kv::Str("x".into()))])), ("derivedTextData", object(derived))];
        if let Some(font) = font {
            fields.extend([
                ("fontName", font),
                ("textUserLayoutVersion", Kv::Uint(3)),
                ("textExplicitLayoutVersion", Kv::Uint(4)),
                ("textTracking", Kv::Float(0.75)),
            ]);
        }
        node(id, "TEXT", label_name, 2, fields)
    };
    let nodes = vec![
        node(1, "DOCUMENT", "Doc", 0, vec![]),
        node(2, "CANVAS", "Canvas", 1, vec![]),
        label(3, "Saved", Some(name("Example-RealSemibold")), Some(source_meta)),
        label(4, "Missing", None, None),
    ];
    let msg = object(vec![("nodeChanges", Kv::Array(nodes))]);
    let (json, _, networks, resources) = super::map::to_rest_with_layouts(&msg, "Native fonts", crate::rest::MAX_NODES);
    let (doc, _) = crate::rest::import_with_native_motion(&json, &|_| None, networks, crate::rest::MAX_NODES, Default::default(), resources).unwrap();
    let saved = doc.nodes().find(|n| n.name == "Saved").unwrap();
    let t = saved.kind.text().unwrap();
    let face = t.native_font_name().unwrap();
    assert_eq!((face.family.as_str(), face.style.as_str(), face.postscript.as_str()), ("Example", "Semibold", "Example-RealSemibold"));
    let layout = t.native_layout(saved.width, saved.height).unwrap();
    let meta = &layout.font_meta_data[0];
    assert_eq!(meta.key.as_ref().unwrap().postscript, "");
    assert_eq!(meta.font_digest.as_deref(), Some(&[17; 20][..]));
    assert_eq!((meta.font_weight, meta.font_line_height, meta.font_style.as_deref()), (Some(600), Some(1.2), Some("NORMAL")));
    assert_eq!((layout.user_layout_version, layout.explicit_layout_version, layout.text_tracking), (Some(3), Some(4), Some(0.75)));
    assert_eq!(layout.derived_lines.as_ref().unwrap(), &["RTL"]);
    assert_eq!(layout.logical_offsets.as_deref(), Some(&[0.0, 8.25][..]));
    assert_eq!((layout.truncation_start_index, layout.truncated_height), (Some(-1), Some(-1.0)));
    let decoration = &layout.decorations.as_ref().unwrap()[0];
    assert_eq!(decoration.rects, vec![[1.25, 5.0, 8.0, 1.0]]);
    assert_eq!(decoration.style_id, Some(0));
    let missing = doc.nodes().find(|n| n.name == "Missing").unwrap();
    let t = missing.kind.text().unwrap();
    assert!(t.native_font_name().is_none());
    let missing_layout = t.native_layout(missing.width, missing.height).unwrap();
    assert!(missing_layout.font_meta_data.is_empty());
    assert!(missing_layout.decorations.is_none() && missing_layout.derived_lines.is_none() && missing_layout.logical_offsets.is_none());
    assert_eq!((missing_layout.user_layout_version, missing_layout.explicit_layout_version, missing_layout.text_tracking), (None, None, None));
}
