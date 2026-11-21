//! Native writer regression controls. The Kiwi wire is inspected separately from the importer.

use figcraft_doc::geom::BezPath;
use figcraft_doc::text::{NativeTextBaseline, NativeTextGlyph, NativeTextLayout};
use figcraft_doc::{Document, NodeId, NodeKind, TextData, VectorNetwork};
use std::sync::Arc;

use super::kiwi::Kv;

fn outline() -> Vec<u8> {
    let mut bytes = Vec::new();
    for (op, x, y) in [(1, 0.0_f32, 0.0_f32), (2, 0.75, 0.0), (2, 0.0, 1.0)] {
        bytes.push(op);
        bytes.extend(x.to_le_bytes());
        bytes.extend(y.to_le_bytes());
    }
    bytes.push(0);
    bytes
}

fn fixture() -> (Document, NodeId) {
    let mut doc = Document::new("Glyph cache");
    let page = doc.first_page().unwrap();
    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((8.0, 8.0));
    doc.create(page, NodeKind::Vector { network: VectorNetwork::from_path(&path) }, "Earlier blob", 0.0, 0.0, 8.0, 8.0).unwrap();
    let mut text = TextData::new("A😀b");
    let face = figcraft_doc::NativeFontName {
        family: text.style.font_family.clone(),
        style: text.style.font_style.clone(),
        postscript: "Original-Regular".into(),
    };
    assert!(text.save_native_font_name(face.clone()));
    text.restyle(2, 3, |style| style.font_size = 19.0);
    let line = NativeTextBaseline {
        first_character: Some(0),
        end_character: Some(4),
        position: Some([1.0, 14.0]),
        width: Some(32.5),
        line_y: Some(2.0),
        line_height: Some(18.0),
        line_ascent: Some(13.0),
        ignore_leading_trim: Some(0.5),
    };
    let first = NativeTextGlyph {
        outline: Some(0),
        position: Some([1.0, 14.0]),
        font_size: Some(14.0),
        advance: Some(8.0),
        first_character: Some(0),
        rotation: Some(0.25),
        ..Default::default()
    };
    let generated = NativeTextGlyph {
        outline: Some(0),
        position: Some([22.0, 14.0]),
        font_size: Some(19.0),
        advance: Some(7.0),
        style_id: Some(91),
        style_character: Some(3),
        ..Default::default()
    };
    let layout = NativeTextLayout {
        layout_size: Some([32.5, 20.0]),
        user_layout_version: Some(4),
        explicit_layout_version: None,
        text_tracking: Some(-0.02),
        derived_lines: Some(vec!["LTR".into()]),
        logical_offsets: Some(vec![0.0, 8.0, 8.0, 22.0]),
        truncation_start_index: Some(-1),
        truncated_height: Some(-1.0),
        decorations: Some(vec![figcraft_doc::NativeTextDecoration { rects: vec![[1.0, 13.0, 12.0, 1.25]], style_id: Some(0), ..Default::default() }]),
        outlines: vec![outline().into()],
        baselines: vec![line],
        glyphs: vec![first, generated],
        font_meta_data: vec![figcraft_doc::NativeFontMeta {
            key: Some(face),
            font_line_height: Some(1.25),
            font_digest: Some(vec![0x83, 0xa7, 0, 0xff]),
            font_style: Some("NORMAL".into()),
            font_weight: Some(400),
        }],
    };
    assert!(text.save_native_layout(Arc::new(layout), 40.0, 20.0));
    let id = doc.create(page, NodeKind::Text(text.clone()), "Imported", 20.0, 0.0, 40.0, 20.0).unwrap();
    doc.create(page, NodeKind::Text(text), "Second copy", 80.0, 0.0, 40.0, 20.0).unwrap();
    (doc, id)
}

fn with_wire(doc: &Document, inspect: impl FnOnce(&Kv, &Kv)) {
    let (bytes, _) = super::export(doc, None).unwrap();
    let archive = super::Archive::open(&bytes).unwrap();
    let (schema, raw) = super::decode_canvas(&archive.canvas).unwrap();
    let message = schema.decode_message(schema.find("Message").unwrap(), &raw).unwrap();
    let label = message.get("nodeChanges").unwrap().list().iter().find(|n| n.get("name").and_then(Kv::str) == Some("Imported")).unwrap();
    inspect(label, &message);
}

#[test]
fn native_glyph_export_preserves_font_identity_and_rejects_it_after_font_change() {
    let (mut doc, id) = fixture();
    with_wire(&doc, |label, _| {
        assert_eq!(label.get("fontName").unwrap().get("postscript").and_then(Kv::str), Some("Original-Regular"));
        let font = &label.get("derivedTextData").unwrap().get("fontMetaData").unwrap().list()[0];
        assert_eq!(font.get("key").unwrap(), label.get("fontName").unwrap());
        assert_eq!(font.get("fontDigest").and_then(Kv::bytes), Some([0x83, 0xa7, 0, 0xff].as_slice()));
        assert_eq!(font.get("fontStyle").and_then(Kv::str), Some("NORMAL"));
        assert_eq!(font.get("fontWeight").and_then(Kv::f64), Some(400.0));
        assert_eq!(font.get("fontLineHeight").and_then(Kv::f64), Some(1.25));
        let derived = label.get("derivedTextData").unwrap();
        assert_eq!(derived.get("derivedLines").unwrap().list()[0].get("directionality").and_then(Kv::str), Some("LTR"));
        assert_eq!(derived.get("logicalIndexToCharacterOffsetMap").unwrap().list().len(), 4);
        assert_eq!(derived.get("truncationStartIndex").and_then(Kv::f64), Some(-1.0));
        assert_eq!(derived.get("truncatedHeight").and_then(Kv::f64), Some(-1.0));
        let decoration = &derived.get("decorations").unwrap().list()[0];
        assert_eq!(decoration.get("rects").unwrap().list()[0].get("h").and_then(Kv::f64), Some(1.25));
        assert_eq!(label.get("textUserLayoutVersion").and_then(Kv::f64), Some(4.0));
        assert!(label.get("textExplicitLayoutVersion").is_none());
    });
    let text = doc.node_mut(id).unwrap().kind.text_mut().unwrap();
    text.characters.push('!');
    with_wire(&doc, |label, _| {
        assert_eq!(label.get("fontName").unwrap().get("postscript").and_then(Kv::str), Some("Original-Regular"));
        assert!(label.get("derivedTextData").and_then(|d| d.get("fontMetaData")).is_none());
    });
    doc.node_mut(id).unwrap().kind.text_mut().unwrap().style.font_family = "Replacement".into();
    with_wire(&doc, |label, _| {
        assert_eq!(label.get("fontName").unwrap().get("family").and_then(Kv::str), Some("Replacement"));
        assert_eq!(label.get("fontName").unwrap().get("postscript").and_then(Kv::str), Some(""));
        assert!(label.get("derivedTextData").and_then(|d| d.get("fontMetaData")).is_none());
    });
}

#[test]
fn native_glyph_export_remaps_shared_outline_and_styled_utf16_placement() {
    let (doc, _) = fixture();
    with_wire(&doc, |label, message| {
        let layout = label.get("derivedTextData").unwrap();
        assert_eq!(layout.get("layoutSize").unwrap().get("x").and_then(Kv::f64), Some(32.5));
        let line = &layout.get("baselines").unwrap().list()[0];
        assert_eq!(line.get("lineAscent").and_then(Kv::f64), Some(13.0));
        assert_eq!(line.get("position").unwrap().get("y").and_then(Kv::f64), Some(14.0));
        let glyphs = layout.get("glyphs").unwrap().list();
        assert_eq!(glyphs.len(), 2);
        let blob_index = glyphs[0].get("commandsBlob").and_then(Kv::f64).unwrap() as usize;
        assert!(blob_index > 0, "earlier vector occupied the first global index");
        let blobs = message.get("blobs").unwrap().list();
        assert_eq!(blobs[blob_index].get("bytes").and_then(Kv::bytes), Some(outline().as_slice()));
        assert_eq!(blobs.iter().filter(|b| b.get("bytes").and_then(Kv::bytes) == Some(outline().as_slice())).count(), 1);
        assert_eq!(glyphs[0].get("rotation").and_then(Kv::f64), Some(0.25));
        assert_eq!(glyphs[1].get("commandsBlob"), glyphs[0].get("commandsBlob"));
        assert_eq!(glyphs[1].get("styleID").and_then(Kv::f64), Some(1.0));
        assert!(glyphs[1].get("firstCharacter").is_none());
    });
}

#[test]
fn native_glyph_export_discards_stale_text_style_and_box_cache() {
    let (doc, id) = fixture();
    for edit in ["text", "style", "width", "height"] {
        let mut edited = doc.clone();
        let node = edited.node_mut(id).unwrap();
        let NodeKind::Text(data) = &mut node.kind else { panic!("expected text") };
        match edit {
            "text" => data.characters.push('!'),
            "style" => data.style.font_size += 4.0,
            "width" => node.width += 5.0,
            _ => node.height += 5.0,
        }
        with_wire(&edited, |label, _| {
            assert!(label.get("derivedTextData").and_then(|v| v.get("glyphs")).is_none(), "stale {edit}");
            let chars = label.get("textData").unwrap().get("characters").and_then(Kv::str).unwrap();
            assert_eq!(chars, if edit == "text" { "A😀b!" } else { "A😀b" });
        });
    }
}

#[test]
fn native_glyph_instance_keeps_its_overridden_text_layout_with_its_computed_size() {
    let (mut doc, original) = fixture();
    let page = doc.first_page().unwrap();
    let main = doc
        .create(
            page,
            NodeKind::Component { description: String::new(), props: Default::default(), prop_order: vec![] },
            "Text component",
            0.0,
            0.0,
            80.0,
            32.0,
        )
        .unwrap();
    doc.reparent(original, main, None).unwrap();
    let instance =
        doc.create(page, NodeKind::Instance { component: main, props: Default::default() }, "Text instance", 100.0, 0.0, 80.0, 32.0).unwrap();
    let mut text = doc.node(original).unwrap().kind.text().unwrap().clone();
    let mut layout = text.native_layout(40.0, 20.0).unwrap().clone();
    text.characters = "C😀b".into();
    layout.glyphs[0].position = Some([3.0, 14.0]);
    assert!(text.save_native_layout(Arc::new(layout), 48.0, 20.0));
    let copy = doc.create(instance, NodeKind::Text(text), "Imported", 0.0, 0.0, 48.0, 20.0).unwrap();
    doc.node_mut(copy).unwrap().source = Some(original);
    let bytes = super::export(&doc, None).unwrap().0;
    let (read, _) = super::import(&bytes, "Instance control").unwrap();
    let copied = read.nodes().find(|n| n.parent.and_then(|id| read.node(id)).is_some_and(|p| p.name == "Text instance")).unwrap();
    let data = copied.kind.text().unwrap();
    assert_eq!(data.characters, "C😀b");
    assert_eq!(copied.width, 48.0);
    assert_eq!(data.native_layout(copied.width, copied.height).unwrap().glyphs[0].position, Some([3.0, 14.0]));
}
