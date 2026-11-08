//! The Export section's Preview row: a slice paints nothing itself, so its preview must show the
//! page content under it — the same thing exporting the slice writes — and not a blank
//! transparency checkerboard next to a correct export. And its setting rows: a size only for
//! images, and advanced options named as the reference app names them.

use figcraft_doc::{Document, NodeId};
use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::assets_panel::thumbnail_pixmap;

/// Run `commands`, returning the document and the id each one made.
fn doc_after(commands: &[(&str, Value)]) -> (Document, Vec<NodeId>) {
    let mut s = Session::new();
    let ids = commands.iter().map(|(cmd, params)| NodeId(s.execute(cmd, params).unwrap()["id"].as_u64().unwrap())).collect();
    (s.doc().unwrap().clone(), ids)
}

/// Whether the pixmap paints anything (any pixel not fully transparent).
fn paints(pix: &figcraft_render::Pixmap) -> bool {
    figcraft_render::to_rgba8(pix).as_chunks::<4>().0.iter().any(|p| p.get(3).copied().unwrap_or(0) > 0)
}

#[test]
fn a_slice_previews_the_content_under_it() {
    let (doc, ids) = doc_after(&[
        ("shape.rectangle", json!({"x": 0, "y": 0, "width": 200, "height": 100})),
        ("node.create", json!({"type": "slice", "x": 0, "y": 0, "width": 200, "height": 100})),
    ]);
    let slice = ids[1];
    let pix = thumbnail_pixmap(&doc, slice, 200.0).expect("the slice previews");
    assert_eq!([pix.width(), pix.height()], [200, 100], "the slice's area, not an empty node");
    assert!(paints(&pix), "the rectangle under the slice shows");
}

/// The preview follows the slice's first export setting: with Ignore overlapping layers (contents
/// only) on, a slice in a frame shows only the frame's content, as its export does.
#[test]
fn a_slice_in_a_frame_previews_only_the_frame() {
    let mut s = Session::new();
    let new = |s: &mut Session, p: Value| NodeId(s.execute("node.create", &p).unwrap()["id"].as_u64().unwrap());
    let f = new(&mut s, json!({"type": "frame", "x": 0, "y": 0, "width": 200, "height": 100, "props": {"fills": []}}));
    let slice = new(&mut s, json!({"type": "slice", "x": 0, "y": 0, "width": 200, "height": 100, "parent": f}));
    new(&mut s, json!({"type": "rectangle", "x": 0, "y": 0, "width": 200, "height": 100}));
    let pix = thumbnail_pixmap(s.doc().unwrap(), slice, 200.0).expect("the slice previews");
    assert_eq!([pix.width(), pix.height()], [200, 100]);
    assert!(!paints(&pix), "the rectangle on the page above the empty frame is left out");
    s.execute("export.add", &json!({"ids": [slice], "setting": {"size": "1x", "contentsOnly": false}})).unwrap();
    let pix = thumbnail_pixmap(s.doc().unwrap(), slice, 200.0).expect("the slice previews");
    assert!(paints(&pix), "with the option off, the rectangle shows");
}

#[test]
fn a_slice_with_nothing_under_it_is_blank() {
    let (doc, ids) = doc_after(&[("node.create", json!({"type": "slice", "x": 0, "y": 0, "width": 200, "height": 100}))]);
    let pix = thumbnail_pixmap(&doc, ids[0], 200.0).expect("the preview still has the slice's area");
    assert_eq!([pix.width(), pix.height()], [200, 100]);
    assert!(!paints(&pix), "nothing under it stays transparent");
}

#[test]
fn other_layers_preview_themselves() {
    let (doc, ids) = doc_after(&[
        ("shape.rectangle", json!({"x": 0, "y": 0, "width": 200, "height": 100})),
        ("node.create", json!({"type": "slice", "x": 0, "y": 0, "width": 200, "height": 100})),
    ]);
    let pix = thumbnail_pixmap(&doc, ids[0], 200.0).expect("the rectangle previews");
    assert_eq!([pix.width(), pix.height()], [200, 100], "the layer's own area");
    assert!(paints(&pix));
}

/// SVG and PDF are vector: their export rows offer no size, which their export ignores (#256).
#[test]
fn export_rows_offer_a_size_for_images_only() {
    let mut s = Session::new();
    let r = crate::present_tests::id(&s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 100, "height": 80})).unwrap());
    s.execute("select.set", &json!({"ids": [r]})).unwrap();
    s.execute("export.add", &json!({"ids": [r]})).unwrap();
    let mut u = crate::layers_drag_tests::Ui::new(s);
    let size_menu = |u: &crate::layers_drag_tests::Ui| u.ctx.read_response(egui::Id::unique(("export_size", 0)).with("menu")).is_some();
    u.frame(vec![]);
    assert!(size_menu(&u), "PNG has sizes");
    for format in ["svg", "pdf", "xml"] {
        u.app.run("export.update", json!({"ids": [r], "index": 0, "format": format}));
        u.frame(vec![]);
        u.frame(vec![]);
        assert!(!size_menu(&u), "{format} is vector: no size");
    }
    for format in ["jpg", "webp"] {
        u.app.run("export.update", json!({"ids": [r], "index": 0, "format": format}));
        u.frame(vec![]);
        u.frame(vec![]);
        assert!(size_menu(&u), "{format} has sizes again");
    }
}

/// The text painted this frame, with the centre of each piece.
pub(crate) fn labelled(u: &mut crate::layers_drag_tests::Ui) -> Vec<(String, egui::Pos2)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos + t.galley.rect.center().to_vec2())),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    u.frame(vec![]);
    let frame = u.frame_output(vec![]);
    let mut out = Vec::new();
    for c in &frame.shapes {
        walk(&c.shape, &mut out);
    }
    out
}

pub(crate) fn click(u: &mut crate::layers_drag_tests::Ui, p: egui::Pos2) {
    u.frame(vec![egui::Event::PointerMoved(p)]);
    u.button(p, true);
    u.button(p, false);
    u.frame(vec![]);
}

/// Android Vector Drawable is in the format dropdown as "XML", and choosing it sets the format.
#[test]
fn the_format_dropdown_offers_android_vector_drawable() {
    assert!(figcraft_doc::ExportFormat::ALL.contains(&figcraft_doc::ExportFormat::VectorDrawable));
    let mut s = Session::new();
    let r = crate::present_tests::id(&s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 100, "height": 80})).unwrap());
    s.execute("select.set", &json!({"ids": [r]})).unwrap();
    s.execute("export.add", &json!({"ids": [r]})).unwrap();
    let mut u = crate::layers_drag_tests::Ui::new(s);
    u.frame(vec![]);
    let nodes = labelled(&mut u);
    let field = nodes.iter().find(|(l, _)| l == "PNG").map(|n| n.1).unwrap_or_else(|| panic!("no PNG dropdown in {nodes:?}"));
    click(&mut u, field);
    let nodes = labelled(&mut u);
    let xml = nodes.iter().find(|(l, _)| l == "XML").map(|n| n.1).unwrap_or_else(|| panic!("no XML row in {nodes:?}"));
    click(&mut u, xml);
    let doc = u.app.session.doc().unwrap();
    assert_eq!(doc.node(r).unwrap().exports[0].format, figcraft_doc::ExportFormat::VectorDrawable);
}

/// The File > Export dialog has an XML preset.
#[test]
fn the_export_dialog_has_an_xml_preset() {
    let mut s = Session::new();
    let r = crate::present_tests::id(&s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 100, "height": 80})).unwrap());
    s.execute("select.set", &json!({"ids": [r]})).unwrap();
    let mut u = crate::layers_drag_tests::Ui::new(s);
    u.frame(vec![]);
    crate::menus::invoke(&mut u.app, "file.export");
    let nodes = labelled(&mut u);
    assert!(nodes.iter().any(|(l, _)| l == "XML"), "no XML preset in {nodes:?}");
}

/// The advanced options: Ignore overlapping layers (the `contentsOnly` setting), and Include
/// bounding box for text layers only. A setting that already has the bounding box on keeps it, and
/// the option stays so it can be turned off.
#[test]
fn advanced_options_offer_the_bounding_box_for_text_layers() {
    let mut s = Session::new();
    let r = crate::present_tests::id(&s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 100, "height": 80})).unwrap());
    let t = crate::present_tests::id(&s.execute("text.create", &json!({"text": "Hi", "x": 200, "y": 0})).unwrap());
    s.execute("export.add", &json!({"ids": [r]})).unwrap();
    s.execute("export.add", &json!({"ids": [t]})).unwrap();
    let mut u = crate::layers_drag_tests::Ui::new(s);
    let options = |u: &mut crate::layers_drag_tests::Ui, id: NodeId| {
        u.app.run("select.set", json!({"ids": [id]}));
        u.frame(vec![]);
        egui::Popup::open_id(&u.ctx, egui::Id::unique(("export_more", 0)));
        u.frame(vec![]);
        crate::import_report_tests::shown_texts(u.frame_output(vec![]).shapes)
    };
    let shown = options(&mut u, r);
    assert!(shown.iter().any(|s| s == "Ignore overlapping layers"), "the reference app's name: {shown:?}");
    assert!(!shown.iter().any(|s| s == "Contents only"));
    assert!(!shown.iter().any(|s| s == "Include bounding box"), "a rectangle has no bounding box option");
    let shown = options(&mut u, t);
    assert!(shown.iter().any(|s| s == "Include bounding box"), "a text layer has it: {shown:?}");
    u.app.run("export.update", json!({"ids": [r], "index": 0, "includeBoundingBox": true}));
    let shown = options(&mut u, r);
    assert!(shown.iter().any(|s| s == "Include bounding box"), "a rectangle that has it on shows it: {shown:?}");
    assert!(u.app.session.doc().unwrap().node(r).unwrap().exports[0].include_bounding_box, "the setting is kept");
}

// Export quality draft ownership. These fixtures click the live full-app widgets using their
// observed AccessKit bounds; no editor cache, focus, popup, or draft is seeded by the tests.
fn quality_owner_frame(u: &mut crate::layers_drag_tests::Ui, events: Vec<egui::Event>) -> egui::FullOutput {
    u.frame_raw(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 1800.0))),
        events,
        ..Default::default()
    })
}
fn quality_owner_click(u: &mut crate::layers_drag_tests::Ui, at: egui::Pos2) {
    quality_owner_frame(u, vec![egui::Event::PointerMoved(at)]);
    for pressed in [true, false] {
        quality_owner_frame(
            u,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
            ],
        );
    }
    quality_owner_frame(u, vec![]);
}
fn quality_owner_key(u: &mut crate::layers_drag_tests::Ui, key: egui::Key) {
    quality_owner_frame(
        u,
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE })
            .collect(),
    );
    quality_owner_frame(u, vec![]);
}
fn quality_owner_bounds(u: &mut crate::layers_drag_tests::Ui, role: egui::accesskit::Role, label: &str) -> Vec<egui::Pos2> {
    quality_owner_frame(u, vec![]);
    let out = quality_owner_frame(u, vec![]);
    let nodes = out.platform_output.accesskit_update.expect("actual AccessKit tree").nodes;
    let mut centers: Vec<_> = nodes
        .iter()
        .filter(|(_, n)| n.role() == role && n.label() == Some(label))
        .map(|(_, n)| {
            let b = n.bounds().expect("actual widget bounds");
            egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
        })
        .collect();
    centers.sort_by(|a, b| a.y.total_cmp(&b.y));
    centers
}
fn quality_owner_open_and_type(u: &mut crate::layers_drag_tests::Ui, row: usize) -> egui::Id {
    let before: Vec<_> = u
        .app
        .session
        .docs
        .iter()
        .map(|d| (d.uid, d.doc.nodes().map(|n| (n.id, n.exports.clone())).collect::<Vec<_>>(), d.history.undo.len()))
        .collect();
    let buttons = quality_owner_bounds(u, egui::accesskit::Role::Button, "Export settings");
    let at = *buttons.get(row).unwrap_or_else(|| panic!("no export settings row {row}: {buttons:?}"));
    quality_owner_click(u, at);
    quality_owner_frame(u, vec![]);
    assert!(egui::Popup::is_id_open(&u.ctx, egui::Id::unique(("export_more", row))), "actual button opened row popup");
    let fields = quality_owner_bounds(u, egui::accesskit::Role::TextInput, "90");
    assert_eq!(fields.len(), 1, "one live JPEG quality field: {fields:?}");
    quality_owner_click(u, fields[0]);
    let editor = u.ctx.memory(|m| m.focused()).expect("actual quality editor owns focus after click");
    assert!(u.ctx.read_response(editor).is_some(), "the focused editor was actually drawn");
    let out = quality_owner_frame(u, vec![egui::Event::Text("37".into())]);
    let mut painted = crate::import_report_tests::shown_texts(out.shapes);
    painted.extend(crate::import_report_tests::shown_texts(quality_owner_frame(u, vec![]).shapes));
    assert!(painted.iter().any(|t| t == "37"), "real typed draft is visibly 37: {painted:?}");
    assert_eq!(u.ctx.memory(|m| m.focused()), Some(editor));
    let after: Vec<_> = u
        .app
        .session
        .docs
        .iter()
        .map(|d| (d.uid, d.doc.nodes().map(|n| (n.id, n.exports.clone())).collect::<Vec<_>>(), d.history.undo.len()))
        .collect();
    assert_eq!(after, before, "a real typed draft has not changed any export setting or undo history");
    editor
}
fn quality_owner_setup() -> (crate::layers_drag_tests::Ui, NodeId, NodeId) {
    let mut s = Session::new();
    let a = crate::present_tests::id(&s.execute("shape.rectangle", &json!({"name":"Source A","width":100,"height":80})).unwrap());
    let b = crate::present_tests::id(&s.execute("shape.rectangle", &json!({"name":"Target B","x":200,"width":100,"height":80})).unwrap());
    for id in [a, b] {
        s.execute("export.add", &json!({"ids":[id],"setting":{"format":"jpg","quality":90,"suffix":"-first"}})).unwrap();
    }
    s.execute("select.set", &json!({"ids":[a]})).unwrap();
    let mut u = crate::layers_drag_tests::Ui::new(s);
    u.ctx.enable_accesskit();
    quality_owner_frame(&mut u, vec![]);
    (u, a, b)
}
fn quality_owner_value(u: &crate::layers_drag_tests::Ui, doc: usize, node: NodeId, row: usize) -> u8 {
    u.app.session.docs[doc].doc.node(node).unwrap().exports[row].quality()
}
fn quality_owner_undo(u: &crate::layers_drag_tests::Ui, doc: usize) -> usize {
    u.app.session.docs[doc].history.undo.len()
}
fn quality_owner_select(u: &mut crate::layers_drag_tests::Ui, ids: &[NodeId]) {
    u.app.run("select.set", json!({"ids":ids})).expect("actual app command selects target");
    quality_owner_frame(u, vec![]);
}
/// Retargeting may cancel the source draft or finish it on its original source. It must never
/// add a target edit, and any permitted source edit has precisely one source undo step.
fn quality_owner_assert_source_only(u: &mut crate::layers_drag_tests::Ui, a: NodeId, b: NodeId, before: usize) {
    let source = quality_owner_value(u, 0, a, 0);
    assert!(matches!(source, 90 | 37), "only original value or original source draft is allowed");
    assert_eq!(quality_owner_value(u, 0, b, 0), 90, "uncommitted source draft never writes a new target");
    assert_eq!(quality_owner_undo(u, 0), before + usize::from(source == 37), "only a source commit can add one undo step");
    if source == 37 {
        u.app.run("edit.undo", json!({})).unwrap();
        assert_eq!(quality_owner_value(u, 0, a, 0), 90);
        assert_eq!(quality_owner_value(u, 0, b, 0), 90);
    }
}
#[test]
fn export_quality_draft_does_not_follow_selection_replacement() {
    let (mut u, a, b) = quality_owner_setup();
    let before = quality_owner_undo(&u, 0);
    quality_owner_open_and_type(&mut u, 0);
    assert_eq!(quality_owner_value(&u, 0, a, 0), 90, "typing alone is uncommitted");
    quality_owner_select(&mut u, &[b]);
    quality_owner_key(&mut u, egui::Key::Enter);
    quality_owner_assert_source_only(&mut u, a, b, before);
}
#[test]
fn export_quality_draft_does_not_follow_expanded_selection_with_same_first_node() {
    let (mut u, a, b) = quality_owner_setup();
    let before = quality_owner_undo(&u, 0);
    quality_owner_open_and_type(&mut u, 0);
    quality_owner_select(&mut u, &[a, b]);
    assert_eq!(u.app.session.selection(), vec![a, b], "first source remains selected");
    quality_owner_key(&mut u, egui::Key::Enter);
    quality_owner_assert_source_only(&mut u, a, b, before);
}
#[test]
fn export_quality_draft_does_not_follow_document_retarget_with_equal_quality() {
    let (mut u, a, _) = quality_owner_setup();
    u.app.run("file.new", json!({"name":"Separate target document"})).unwrap();
    let b = crate::present_tests::id(&u.app.run("shape.rectangle", json!({"name":"Document B target","width":100,"height":80})).unwrap());
    u.app.run("export.add", json!({"ids":[b],"setting":{"format":"jpg","quality":90,"suffix":"-first"}})).unwrap();
    u.app.run("select.set", json!({"ids":[b]})).unwrap();
    assert_ne!(u.app.session.docs[0].uid, u.app.session.docs[1].uid);
    u.app.run("doc.switch", json!({"index":0})).unwrap();
    quality_owner_frame(&mut u, vec![]);
    let before_a = quality_owner_undo(&u, 0);
    let before_b = quality_owner_undo(&u, 1);
    quality_owner_open_and_type(&mut u, 0);
    u.app.run("doc.switch", json!({"index":1})).unwrap();
    quality_owner_frame(&mut u, vec![]);
    quality_owner_key(&mut u, egui::Key::Enter);
    assert_eq!(quality_owner_value(&u, 1, b, 0), 90, "a focused source quality cannot write the new document");
    assert_eq!(quality_owner_undo(&u, 1), before_b, "new document gains no undo entry");
    let source = quality_owner_value(&u, 0, a, 0);
    assert!(matches!(source, 90 | 37));
    assert_eq!(quality_owner_undo(&u, 0), before_a + usize::from(source == 37));
    if source == 37 {
        u.app.run("doc.switch", json!({"index":0})).unwrap();
        u.app.run("edit.undo", json!({})).unwrap();
        assert_eq!(quality_owner_value(&u, 0, a, 0), 90);
    }
}
#[test]
fn export_quality_draft_does_not_follow_row_shift_after_source_row_removal() {
    let (mut u, a, b) = quality_owner_setup();
    u.app.run("export.add", json!({"ids":[a],"setting":{"format":"jpg","quality":90,"suffix":"-remaining"}})).unwrap();
    quality_owner_frame(&mut u, vec![]);
    quality_owner_open_and_type(&mut u, 0);
    assert_eq!(quality_owner_value(&u, 0, a, 0), 90);
    u.app.run("export.remove", json!({"ids":[a],"index":0})).unwrap();
    let before = quality_owner_undo(&u, 0);
    quality_owner_frame(&mut u, vec![]);
    quality_owner_key(&mut u, egui::Key::Enter);
    assert_eq!(u.app.session.doc().unwrap().node(a).unwrap().exports.len(), 1);
    assert_eq!(u.app.session.doc().unwrap().node(a).unwrap().exports[0].suffix, "-remaining");
    assert_eq!(quality_owner_value(&u, 0, a, 0), 90, "the replacement row at index0 never inherits the removed row's draft");
    assert_eq!(quality_owner_value(&u, 0, b, 0), 90);
    assert_eq!(quality_owner_undo(&u, 0), before, "typing for a removed row adds no export update undo");
    u.app.run("edit.undo", json!({})).unwrap();
    let exports = &u.app.session.doc().unwrap().node(a).unwrap().exports;
    assert_eq!(exports.len(), 2);
    assert_eq!(exports[0].suffix, "-first");
    assert!(exports.iter().all(|setting| setting.quality() == 90));
}
#[test]
fn export_quality_real_enter_commits_only_original_row_once_and_undo_redo_work() {
    let (mut u, a, b) = quality_owner_setup();
    let before = quality_owner_undo(&u, 0);
    quality_owner_open_and_type(&mut u, 0);
    assert_eq!(quality_owner_undo(&u, 0), before);
    quality_owner_key(&mut u, egui::Key::Enter);
    assert_eq!(quality_owner_value(&u, 0, a, 0), 37);
    assert_eq!(quality_owner_value(&u, 0, b, 0), 90);
    assert_eq!(quality_owner_undo(&u, 0), before + 1);
    u.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(quality_owner_value(&u, 0, a, 0), 90);
    u.app.run("edit.redo", json!({})).unwrap();
    assert_eq!(quality_owner_value(&u, 0, a, 0), 37);
}
#[test]
fn export_quality_real_escape_cancels_draft_without_export_or_undo_change() {
    let (mut u, a, b) = quality_owner_setup();
    let before = quality_owner_undo(&u, 0);
    quality_owner_open_and_type(&mut u, 0);
    quality_owner_key(&mut u, egui::Key::Escape);
    assert_eq!(quality_owner_value(&u, 0, a, 0), 90);
    assert_eq!(quality_owner_value(&u, 0, b, 0), 90);
    assert_eq!(quality_owner_undo(&u, 0), before);
    if !egui::Popup::is_id_open(&u.ctx, egui::Id::unique(("export_more", 0))) {
        let buttons = quality_owner_bounds(&mut u, egui::accesskit::Role::Button, "Export settings");
        quality_owner_click(&mut u, buttons[0]);
    }
    quality_owner_focus_current(&mut u, a, 90);
    quality_owner_key(&mut u, egui::Key::Enter);
    assert_eq!(quality_owner_value(&u, 0, a, 0), 90, "cancelled draft cannot revive after popup reopen and later Enter");
    assert_eq!(quality_owner_undo(&u, 0), before);
}

#[test]
fn export_quality_camera_pan_keeps_the_live_draft_and_original_owner() {
    let (mut u, a, b) = quality_owner_setup();
    let before = quality_owner_undo(&u, 0);
    let editor = quality_owner_open_and_type(&mut u, 0);
    let authored = u.app.session.ds().unwrap().doc.clone();
    let command_revision = u.app.session.ds().unwrap().revision;
    u.app.run("view.pan", json!({"dx":1,"dy":-1})).unwrap();
    assert!(std::sync::Arc::ptr_eq(&authored, &u.app.session.ds().unwrap().doc), "real camera command preserves the authored snapshot");
    assert!(u.app.session.ds().unwrap().revision > command_revision, "command revisions are broader than authored document changes");
    quality_owner_frame(&mut u, vec![]);
    assert_eq!(u.ctx.memory(|m| m.focused()), Some(editor), "camera movement does not retire the live quality editor");
    quality_owner_key(&mut u, egui::Key::Enter);
    assert_eq!(quality_owner_value(&u, 0, a, 0), 37, "the original live draft still commits after camera movement");
    assert_eq!(quality_owner_value(&u, 0, b, 0), 90);
    assert_eq!(quality_owner_undo(&u, 0), before + 1);
}

#[test]
fn export_quality_switch_back_cannot_revive_a_retired_uncommitted_draft() {
    let (mut u, a, b) = quality_owner_setup();
    quality_owner_open_and_type(&mut u, 0);
    quality_owner_select(&mut u, &[b]);
    assert_eq!(quality_owner_value(&u, 0, b, 0), 90, "a retarget does not silently write targetB");
    // A safe implementation may have finished the old source before leaving it. Returning may
    // not manufacture a later commit from its retired cached editor in either policy.
    let source_before_return = quality_owner_value(&u, 0, a, 0);
    let steps_before_return = quality_owner_undo(&u, 0);
    quality_owner_select(&mut u, &[a]);
    quality_owner_focus_current(&mut u, a, source_before_return);
    quality_owner_key(&mut u, egui::Key::Enter);
    assert_eq!(quality_owner_value(&u, 0, a, 0), source_before_return, "returning to A cannot resurrect an uncommitted cached draft");
    assert_eq!(quality_owner_value(&u, 0, b, 0), 90);
    assert_eq!(quality_owner_undo(&u, 0), steps_before_return, "returning adds no delayed export-update undo");
}

#[test]
fn export_quality_focus_loss_and_profile_click_in_same_frame_preserve_both_in_one_undo() {
    let (mut u, a, b) = quality_owner_setup();
    let before_a = u.app.session.doc().unwrap().node(a).unwrap().exports.clone();
    let before_b = u.app.session.doc().unwrap().node(b).unwrap().exports.clone();
    let document_profile = u.app.session.doc().unwrap().color_profile;
    let steps = quality_owner_undo(&u, 0);
    quality_owner_open_and_type(&mut u, 0);
    let radio = quality_owner_bounds(&mut u, egui::accesskit::Role::RadioButton, "Display P3");
    assert_eq!(radio.len(), 1, "actual visible Display P3 radio");
    let at = radio[0];
    // A legitimate coalesced pointer click: both button transitions are delivered in the same
    // presented egui frame, so the radio click and text field focus-loss are one UI action.
    let event = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    quality_owner_frame(&mut u, vec![egui::Event::PointerMoved(at), event(true), event(false)]);
    quality_owner_frame(&mut u, vec![]);
    let doc = u.app.session.doc().unwrap();
    assert_eq!(doc.node(a).unwrap().exports[0].quality(), 37);
    assert_eq!(
        doc.node(a).unwrap().exports[0].color_profile,
        figcraft_doc::ExportColorProfile::DisplayP3,
        "the clicked option is not overwritten by the quality focus-loss commit"
    );
    assert_eq!(doc.node(b).unwrap().exports, before_b, "unselected target remains unchanged");
    assert_eq!(doc.color_profile, document_profile, "per-export profile does not change the document");
    assert_eq!(quality_owner_undo(&u, 0), steps + 1, "one UI action is one export update");
    u.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap().node(a).unwrap().exports, before_a);
    assert_eq!(u.app.session.doc().unwrap().node(b).unwrap().exports, before_b);
    u.app.run("edit.redo", json!({})).unwrap();
    let setting = &u.app.session.doc().unwrap().node(a).unwrap().exports[0];
    assert_eq!(setting.quality(), 37);
    assert_eq!(setting.color_profile, figcraft_doc::ExportColorProfile::DisplayP3);
    assert_eq!(u.app.session.doc().unwrap().node(b).unwrap().exports, before_b);
}

/// Focus the actually drawn current quality field, without typing, seeding, or replacing its
/// draft. A later Enter therefore tests this field's commit path, not an unfocused canvas key.
fn quality_owner_focus_current(u: &mut crate::layers_drag_tests::Ui, source: NodeId, expected: u8) {
    let before = quality_owner_undo(u, u.app.session.active);
    assert_eq!(u.app.session.doc().unwrap().node(source).unwrap().exports[0].quality(), expected);
    if !egui::Popup::is_id_open(&u.ctx, egui::Id::unique(("export_more", 0))) {
        let buttons = quality_owner_bounds(u, egui::accesskit::Role::Button, "Export settings");
        assert!(!buttons.is_empty(), "actual Export settings button");
        quality_owner_click(u, buttons[0]);
    }
    quality_owner_frame(u, vec![]);
    let out = quality_owner_frame(u, vec![]);
    assert_eq!(
        u.app.session.doc().unwrap().node(source).unwrap().exports[0].quality(),
        expected,
        "opening the fresh field cannot revive a retired value"
    );
    let expected_text = expected.to_string();
    let nodes = out.platform_output.accesskit_update.expect("actual current quality AccessKit tree").nodes;
    let fields: Vec<_> = nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == egui::accesskit::Role::TextInput && node.label() == Some("90") && node.value() == Some(expected_text.as_str())
        })
        .map(|(_, node)| {
            let bounds = node.bounds().expect("actual quality field bounds");
            egui::Rect::from_min_max(egui::pos2(bounds.x0 as f32, bounds.y0 as f32), egui::pos2(bounds.x1 as f32, bounds.y1 as f32))
        })
        .collect();
    assert_eq!(fields.len(), 1, "one actual current quality field showing {expected}");
    quality_owner_click(u, fields[0].center());
    let editor = u.ctx.memory(|memory| memory.focused()).expect("actual quality TextEdit focus after click");
    let response = u.ctx.read_response(editor).expect("the focused TextEdit was actually drawn");
    assert!(u.ctx.text_edit_focused() && response.rect.intersects(fields[0]), "observed quality field owns the later Enter");
    assert_eq!(
        u.app.session.doc().unwrap().node(source).unwrap().exports[0].quality(),
        expected,
        "clicking the field does not revive or replace its draft"
    );
    assert_eq!(quality_owner_undo(u, u.app.session.active), before, "focus preparation is not an authored command");
}
