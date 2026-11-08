//! Copy, cut and paste in vector edit mode (#1875), measured in Figma: Copy takes the selected
//! segments (a lone point: the segments touching it) and a paste outside is a new vector in place;
//! Cut removes them, splitting the path in the same layer; Paste while editing adds the clipboard's
//! segments or a copied layer's outline to the vector's network in place, selected.

use serde_json::{Value, json};

use crate::{EngineError, Session, params};
use figcraft_doc::{NodeId, NodeKind, VectorNetwork};

fn ex(s: &mut Session, c: &str, p: Value) -> Value {
    s.execute(c, &p).unwrap_or_else(|e| panic!("{c}: {e}"))
}

fn id_of(v: &Value) -> NodeId {
    params::id_of(&v["id"]).unwrap()
}

/// Open path A-B-C-D (three segments) with a look of its own, in edit mode.
fn path(s: &mut Session) -> NodeId {
    let r = ex(s, "vector.create", json!({"points": [[100, 100], [200, 150], [300, 100], [400, 200]], "closed": false}));
    let id = id_of(&r);
    ex(s, "node.set", json!({"id": id, "props": {"strokeWeight": 7, "opacity": 0.5}}));
    ex(s, "vector.edit.begin", json!({"id": id}));
    id
}

fn net(s: &Session, id: NodeId) -> VectorNetwork {
    crate::vectoredit::network(s.doc().unwrap(), id).unwrap().clone()
}

fn layers(s: &Session) -> Vec<NodeId> {
    s.doc().unwrap().node(s.page().unwrap()).unwrap().children.clone()
}

/// The network's points on the page, rounded, in vertex order.
fn page_points(s: &Session, id: NodeId) -> Vec<(i64, i64)> {
    let w = s.doc().unwrap().world_transform(id);
    net(s, id).vertices.iter().map(|v| w * v.point()).map(|p| (p.x.round() as i64, p.y.round() as i64)).collect()
}

/// The number of separate paths a network is made of.
fn subpaths(n: &VectorNetwork) -> usize {
    let mut group: Vec<usize> = (0..n.vertices.len()).collect();
    fn root(g: &[usize], mut i: usize) -> usize {
        while g[i] != i {
            i = g[i];
        }
        i
    }
    for sg in &n.segments {
        let (a, b) = (root(&group, sg.a), root(&group, sg.b));
        group[a] = b;
    }
    (0..n.vertices.len()).filter(|i| root(&group, *i) == *i && n.segments.iter().any(|sg| root(&group, sg.a) == *i)).count()
}

fn select(s: &mut Session, vertices: &[usize], segments: &[usize]) {
    ex(s, "vector.edit.select", json!({"vertices": vertices, "segments": segments}));
}

fn leave(s: &mut Session) {
    ex(s, "vector.edit.end", json!({}));
    assert!(s.ds().unwrap().vector_edit.is_none());
}

fn world_rect(s: &Session, id: NodeId) -> (f64, f64, f64, f64) {
    let b = s.doc().unwrap().world_bounds(id).unwrap();
    (b.x0, b.y0, b.width(), b.height())
}

#[test]
fn a_copied_segment_pastes_outside_as_a_new_vector_in_place() {
    let mut s = Session::new();
    let id = path(&mut s);
    select(&mut s, &[], &[1]);
    let r = ex(&mut s, "edit.copy", json!({}));
    assert_eq!(r["segments"], 1);
    assert_eq!(net(&s, id).segments.len(), 3, "Copy changes nothing");
    leave(&mut s);
    ex(&mut s, "edit.paste", json!({}));
    let all = layers(&s);
    assert_eq!(all.len(), 2);
    let new = all[1];
    let n = net(&s, new);
    assert_eq!((n.vertices.len(), n.segments.len()), (2, 1));
    assert_eq!(page_points(&s, new), vec![(200, 150), (300, 100)], "at the segment's own page position");
    assert_eq!(world_rect(&s, new), (200.0, 100.0, 100.0, 50.0));
    // The layer's look comes along (the width profile aside).
    let node = s.doc().unwrap().node(new).unwrap();
    assert_eq!((node.stroke.weight, node.opacity), (7.0, 0.5));
    assert_eq!(s.selection(), vec![new]);
}

#[test]
fn a_copied_point_takes_the_segments_touching_it() {
    let mut s = Session::new();
    path(&mut s);
    select(&mut s, &[1], &[]);
    ex(&mut s, "edit.copy", json!({}));
    leave(&mut s);
    ex(&mut s, "edit.paste", json!({}));
    let new = *layers(&s).last().unwrap();
    let n = net(&s, new);
    assert_eq!((n.vertices.len(), n.segments.len()), (3, 2), "A-B and B-C");
    assert_eq!(page_points(&s, new), vec![(100, 100), (200, 150), (300, 100)]);
}

#[test]
fn nothing_selected_still_copies_the_whole_layer() {
    let mut s = Session::new();
    path(&mut s);
    ex(&mut s, "edit.copy", json!({}));
    leave(&mut s);
    ex(&mut s, "edit.paste", json!({}));
    let new = *layers(&s).last().unwrap();
    assert_eq!(net(&s, new).segments.len(), 3);
}

#[test]
fn cut_removes_the_segment_in_the_same_layer_and_stays_in_edit_mode() {
    let mut s = Session::new();
    let id = path(&mut s);
    select(&mut s, &[], &[1]);
    ex(&mut s, "edit.cut", json!({}));
    assert_eq!(layers(&s), vec![id], "no new layer");
    let n = net(&s, id);
    assert_eq!(n.segments.len(), 2);
    assert_eq!(subpaths(&n), 2, "the path split in two");
    let ve = s.ds().unwrap().vector_edit.clone().expect("edit mode stays on");
    assert!(ve.segments.is_empty() && ve.vertices.is_empty());
    leave(&mut s);
    ex(&mut s, "edit.paste", json!({}));
    let all = layers(&s);
    assert_eq!(all.len(), 2);
    let new = all[1];
    assert_eq!(net(&s, new).segments.len(), 1);
    assert_eq!(page_points(&s, new), vec![(200, 150), (300, 100)]);
}

#[test]
fn cut_is_one_undo_step() {
    let mut s = Session::new();
    let id = path(&mut s);
    select(&mut s, &[], &[1]);
    ex(&mut s, "edit.cut", json!({}));
    assert_eq!(net(&s, id).segments.len(), 2);
    s.undo().unwrap();
    assert_eq!(net(&s, id).segments.len(), 3);
    assert_eq!(page_points(&s, id), vec![(100, 100), (200, 150), (300, 100), (400, 200)]);
}

#[test]
fn a_cut_point_takes_its_segments_with_it() {
    let mut s = Session::new();
    let id = path(&mut s);
    select(&mut s, &[1], &[]);
    ex(&mut s, "edit.cut", json!({}));
    assert_eq!(net(&s, id).segments.len(), 1, "only C-D is left");
    leave(&mut s);
    ex(&mut s, "edit.paste", json!({}));
    let new = *layers(&s).last().unwrap();
    assert_eq!(net(&s, new).segments.len(), 2, "A-B and B-C");
}

#[test]
fn pasting_segments_while_editing_adds_them_to_the_same_network_in_place() {
    let mut s = Session::new();
    let id = path(&mut s);
    select(&mut s, &[], &[1]);
    ex(&mut s, "edit.copy", json!({}));
    let r = ex(&mut s, "edit.paste", json!({}));
    assert_eq!(r["vertices"], 2);
    assert_eq!(layers(&s), vec![id], "no new layer");
    let n = net(&s, id);
    assert_eq!((n.vertices.len(), n.segments.len()), (6, 4));
    assert_eq!(page_points(&s, id)[4..], [(200, 150), (300, 100)], "on top of the original");
    let ve = s.ds().unwrap().vector_edit.clone().unwrap();
    assert_eq!(ve.vertices, vec![4, 5], "the pasted points are selected");
    assert!(ve.segments.is_empty());
    // One undo reverts it.
    s.undo().unwrap();
    assert_eq!(net(&s, id).segments.len(), 3);
    assert_eq!(net(&s, id).vertices.len(), 4);
}

#[test]
fn pasting_at_a_point_moves_the_copied_top_left_there() {
    let mut s = Session::new();
    let id = path(&mut s);
    select(&mut s, &[], &[1]);
    ex(&mut s, "edit.copy", json!({}));
    ex(&mut s, "edit.paste", json!({"x": 0, "y": 0}));
    assert_eq!(page_points(&s, id)[4..], [(0, 50), (100, 0)]);
}

#[test]
fn pasting_a_rectangle_while_editing_adds_its_outline_in_the_vectors_coordinates() {
    let mut s = Session::new();
    let rect = id_of(&ex(&mut s, "shape.rectangle", json!({"x": 500, "y": 300, "width": 60, "height": 40})));
    ex(&mut s, "select.set", json!({"ids": [rect]}));
    ex(&mut s, "edit.copy", json!({}));
    let id = path(&mut s);
    let before = world_rect(&s, id);
    ex(&mut s, "edit.paste", json!({}));
    assert_eq!(layers(&s), vec![rect, id], "no new layer");
    let n = net(&s, id);
    assert_eq!((n.vertices.len(), n.segments.len()), (8, 7), "four corners, four sides, joined to the path");
    let mut corners: Vec<(i64, i64)> = page_points(&s, id)[4..].to_vec();
    corners.sort_unstable();
    assert_eq!(corners, vec![(500, 300), (500, 340), (560, 300), (560, 340)]);
    let ve = s.ds().unwrap().vector_edit.clone().unwrap();
    assert_eq!(ve.vertices, vec![4, 5, 6, 7]);
    let after = world_rect(&s, id);
    assert!(after.2 > before.2 && after.3 > before.3, "the vector's bounds grow: {before:?} to {after:?}");
    assert_eq!((after.0, after.1), (before.0, before.1));
    assert_eq!(after.0 + after.2, 560.0);
    s.undo().unwrap();
    assert_eq!(net(&s, id).vertices.len(), 4);
    assert_eq!(world_rect(&s, id), before);
}

#[test]
fn a_rotated_vector_takes_pasted_points_in_its_own_coordinates() {
    let mut s = Session::new();
    let id = path(&mut s);
    ex(&mut s, "node.set", json!({"id": id, "props": {"rotation": 90}}));
    select(&mut s, &[], &[1]);
    ex(&mut s, "edit.copy", json!({}));
    let before = page_points(&s, id);
    assert_ne!(before[0], (100, 100), "the vector is rotated");
    ex(&mut s, "edit.paste", json!({}));
    let after = page_points(&s, id);
    assert_eq!(after[4..], before[1..3]);
}

#[test]
fn layers_with_no_outline_are_skipped_and_named() {
    let mut s = Session::new();
    let rect = id_of(&ex(&mut s, "shape.rectangle", json!({"x": 500, "y": 300, "width": 60, "height": 40})));
    let text = id_of(&ex(&mut s, "text.create", json!({"text": "hello", "x": 500, "y": 400})));
    let name = s.doc().unwrap().node(text).unwrap().name.clone();
    // Only text: nothing to add, and nothing changes.
    ex(&mut s, "select.set", json!({"ids": [text]}));
    ex(&mut s, "edit.copy", json!({}));
    let id = path(&mut s);
    let err = s.execute("edit.paste", &json!({})).unwrap_err();
    assert!(matches!(err, EngineError::Disabled(..)), "{err}");
    assert_eq!(net(&s, id).vertices.len(), 4);
    // The rectangle joins, the text is named as skipped.
    leave(&mut s);
    ex(&mut s, "edit.copy", json!({"ids": [rect, text]}));
    ex(&mut s, "vector.edit.begin", json!({"id": id}));
    let r = ex(&mut s, "edit.paste", json!({}));
    assert_eq!(r["skipped"], json!([name]));
    assert_eq!(net(&s, id).vertices.len(), 8);
}

#[test]
fn a_parent_still_pastes_layers_while_editing() {
    let mut s = Session::new();
    let id = path(&mut s);
    select(&mut s, &[], &[1]);
    ex(&mut s, "edit.copy", json!({}));
    let page = s.page().unwrap();
    ex(&mut s, "edit.paste", json!({"parent": page}));
    assert_eq!(layers(&s).len(), 2);
    assert_eq!(net(&s, id).segments.len(), 3);
}

#[test]
fn an_empty_clipboard_refuses_to_paste_while_editing() {
    let mut s = Session::new();
    path(&mut s);
    assert!(matches!(s.execute("edit.paste", &json!({})), Err(EngineError::Disabled(..))));
}

#[test]
fn a_selected_face_copies_its_own_outline_with_the_fill() {
    let mut s = Session::new();
    let r = ex(&mut s, "shape.rectangle", json!({"x": 100, "y": 100, "width": 80, "height": 60}));
    let id = id_of(&r);
    ex(&mut s, "vector.edit.begin", json!({"id": id}));
    select(&mut s, &[], &[0, 1, 2, 3]);
    ex(&mut s, "edit.copy", json!({}));
    leave(&mut s);
    ex(&mut s, "edit.paste", json!({}));
    let new = *layers(&s).last().unwrap();
    let n = net(&s, new);
    assert_eq!((n.segments.len(), n.regions.len()), (4, 1), "the closed shape stays filled");
    assert!(matches!(s.doc().unwrap().node(new).unwrap().kind, NodeKind::Vector { .. }));
}

#[test]
fn copy_cut_and_paste_survive_junk_in_vector_edit_mode() {
    let junk = [
        json!({}),
        json!(null),
        json!([1, 2]),
        json!({"ids": ["x", -1, 99999]}),
        json!({"id": "nope", "x": "NaN", "y": 1e308}),
        json!({"parent": "nope", "x": -1e308, "y": "x"}),
        json!({"x": 1e308, "y": 1e308}),
    ];
    for cmd in ["edit.copy", "edit.cut", "edit.paste", "edit.pasteOver", "edit.pasteReplace"] {
        for j in &junk {
            let mut s = Session::new();
            path(&mut s);
            select(&mut s, &[1], &[0]);
            ex(&mut s, "text.create", json!({"text": "t"}));
            if let Err(EngineError::Internal { cmd, msg }) = s.execute(cmd, j) {
                panic!("{cmd} panicked on {j}: {msg}");
            }
            // And after something was copied from the edit selection.
            let mut s = Session::new();
            path(&mut s);
            select(&mut s, &[2], &[]);
            let _ = s.execute("edit.copy", &json!({}));
            if let Err(EngineError::Internal { cmd, msg }) = s.execute(cmd, j) {
                panic!("{cmd} panicked on {j}: {msg}");
            }
        }
    }
}

fn region_image_data(rgba: [u8; 4]) -> figcraft_doc::ImageData {
    use image::ImageEncoder as _;
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder.set_icc_profile(figcraft_color::ColorProfile::DisplayP3.icc()).unwrap();
    encoder.write_image(&rgba, 1, 1, image::ExtendedColorType::Rgba8).unwrap();
    assert!(bytes.windows(4).any(|chunk| chunk == b"iCCP"), "the fixture carries actual ICC metadata");
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (1, 1));
    figcraft_doc::ImageData { bytes: std::sync::Arc::new(bytes), width: 1, height: 1 }
}

fn region_image_source(s: &mut Session, data: &figcraft_doc::ImageData, also_layer_fill: bool) -> NodeId {
    let source = id_of(&ex(s, "vector.create", json!({"points": [[0, 0], [100, 0], [0, 100]], "closed": true})));
    let paint = figcraft_color::Paint::image("region-image", figcraft_color::ImageScaleMode::Fill);
    let doc = s.doc_mut().unwrap();
    doc.images.insert("region-image".into(), data.clone());
    let node = doc.get_mut(source).unwrap();
    if also_layer_fill {
        node.fills = vec![paint.clone()];
    }
    let NodeKind::Vector { network } = &mut node.kind else { panic!("not a vector") };
    network.regions[0].fills = Some(vec![paint]);
    ex(s, "vector.edit.begin", json!({"id": source}));
    select(s, &[], &[0, 1, 2]);
    ex(s, "edit.copy", json!({}));
    leave(s);
    source
}

fn pasted_region_image_key(s: &Session, target: NodeId) -> String {
    let network = net(s, target);
    let paint = &network.regions.last().unwrap().fills.as_ref().unwrap()[0];
    let figcraft_color::PaintKind::Image(image) = &paint.kind else { panic!("not an image region") };
    image.image.clone()
}

fn assert_clipboard_unchanged(s: &Session, before: &crate::Clipboard) {
    assert_eq!(s.clipboard.nodes, before.nodes);
    assert_eq!(s.clipboard.images, before.images);
    assert_eq!(s.clipboard.props, before.props);
    assert_eq!(s.clipboard.paint, before.paint);
    assert_eq!(s.clipboard.stroke, before.stroke);
    assert_eq!(s.clipboard.paint_images, before.paint_images);
    assert_eq!(s.clipboard.paint_source_doc, before.paint_source_doc);
    assert_eq!(s.clipboard.effect, before.effect);
    assert_eq!(s.clipboard.bounds, before.bounds);
    assert_eq!(s.clipboard.source_doc, before.source_doc);
    assert_eq!(s.clipboard.copied_components, before.copied_components);
    assert_eq!(s.clipboard.layers_generation, before.layers_generation);
    assert_eq!(s.clipboard.source, before.source);
    assert_eq!(s.clipboard.source_path, before.source_path);
}

fn assert_refused_paste_keeps_document_and_clipboard(s: &mut Session) {
    let doc = s.doc().unwrap().clone();
    let undo = s.undo_depth();
    let selection = s.selection();
    let clipboard = s.clipboard.clone();
    assert!(s.execute("edit.paste", &json!({})).is_err(), "the paste must refuse before any mutation");
    assert_eq!(s.doc().unwrap(), &doc, "including images and network");
    assert_eq!(s.undo_depth(), undo);
    assert_eq!(s.selection(), selection);
    assert_clipboard_unchanged(s, &clipboard);
}

#[test]
fn vector_copy_collects_images_used_only_by_selected_region_fills() {
    let mut s = Session::new();
    let data = region_image_data([255, 0, 0, 255]);
    region_image_source(&mut s, &data, false);
    assert_eq!(s.clipboard.images.get("region-image"), Some(&data));
}

#[test]
fn cross_document_vector_paste_keeps_region_image_bytes_profile_and_native_roundtrip() {
    let mut s = Session::new();
    let data = region_image_data([255, 0, 0, 255]);
    // The layer fill ensures the original collector also carries the bytes, isolating paste admission.
    region_image_source(&mut s, &data, true);
    assert_eq!(s.clipboard.images.get("region-image"), Some(&data));
    ex(&mut s, "file.new", json!({}));
    let target = path(&mut s);
    let clipboard = s.clipboard.clone();
    ex(&mut s, "edit.paste", json!({}));
    let key = pasted_region_image_key(&s, target);
    assert_eq!(s.doc().unwrap().images.get(&key), Some(&data), "data and ICC bytes follow the retained region fill");
    assert_clipboard_unchanged(&s, &clipboard);
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (reopened, issues) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(reopened.images.get(&key), Some(&data));
    let NodeKind::Vector { network } = &reopened.node(target).unwrap().kind else { panic!("not a vector") };
    let figcraft_color::PaintKind::Image(image) = &network.regions.last().unwrap().fills.as_ref().unwrap()[0].kind else { panic!("not image") };
    assert_eq!(image.image, key);
}

#[test]
fn cross_document_vector_image_collision_remaps_without_overwriting_and_is_undoable() {
    let mut s = Session::new();
    let copied = region_image_data([255, 0, 0, 255]);
    let existing = region_image_data([0, 0, 255, 255]);
    region_image_source(&mut s, &copied, true);
    ex(&mut s, "file.new", json!({}));
    let target = path(&mut s);
    s.doc_mut().unwrap().images.insert("region-image".into(), existing.clone());
    let before = s.doc().unwrap().clone();
    let undo = s.undo_depth();
    let clipboard = s.clipboard.clone();
    ex(&mut s, "edit.paste", json!({}));
    let key = pasted_region_image_key(&s, target);
    assert_ne!(key, "region-image");
    assert!(figcraft_doc::ImageData::is_valid_key(&key));
    assert_eq!(s.doc().unwrap().images.get(&key), Some(&copied));
    assert_eq!(s.doc().unwrap().images.get("region-image"), Some(&existing));
    assert_eq!(s.undo_depth(), (undo.0 + 1, 0));
    assert_clipboard_unchanged(&s, &clipboard);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before, "one undo rolls back both assets and geometry");
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap().images.get(&key), Some(&copied));
    ex(&mut s, "edit.paste", json!({}));
    assert_eq!(s.doc().unwrap().images.len(), 2, "a repeated paste reuses the same copied bytes");
}

#[test]
fn same_document_vector_region_paste_reuses_its_existing_image() {
    let mut s = Session::new();
    let data = region_image_data([255, 0, 0, 255]);
    region_image_source(&mut s, &data, false);
    let target = path(&mut s);
    let images = s.doc().unwrap().images.clone();
    ex(&mut s, "edit.paste", json!({}));
    assert_eq!(pasted_region_image_key(&s, target), "region-image");
    assert_eq!(s.doc().unwrap().images, images);
}

#[test]
fn normal_layer_paste_remaps_colliding_node_and_region_images_together() {
    let mut s = Session::new();
    let copied = region_image_data([255, 0, 0, 255]);
    let existing = region_image_data([0, 0, 255, 255]);
    region_image_source(&mut s, &copied, true);
    ex(&mut s, "file.new", json!({}));
    s.doc_mut().unwrap().images.insert("region-image".into(), existing.clone());
    let out = ex(&mut s, "edit.paste", json!({}));
    let id = params::id_of(&out["ids"][0]).unwrap();
    let key = pasted_region_image_key(&s, id);
    let figcraft_color::PaintKind::Image(node_image) = &s.doc().unwrap().node(id).unwrap().fills[0].kind else { panic!("not image") };
    assert_eq!(node_image.image, key);
    assert_ne!(key, "region-image");
    assert_eq!(s.doc().unwrap().images.get(&key), Some(&copied));
    assert_eq!(s.doc().unwrap().images.get("region-image"), Some(&existing));
}

fn region_pattern_source(s: &mut Session, source: NodeId) {
    let from = id_of(&ex(s, "vector.create", json!({"points": [[0, 0], [100, 0], [0, 100]], "closed": true})));
    let NodeKind::Vector { network } = &mut s.doc_mut().unwrap().get_mut(from).unwrap().kind else { panic!("not vector") };
    network.regions[0].fills = Some(vec![figcraft_color::Paint::pattern(source.0)]);
    ex(s, "vector.edit.begin", json!({"id": from}));
    select(s, &[], &[0, 1, 2]);
    ex(s, "edit.copy", json!({}));
    leave(s);
}

#[test]
fn foreign_region_patterns_refuse_without_changing_document_clipboard_or_undo() {
    let mut s = Session::new();
    let pattern = id_of(&ex(&mut s, "shape.rectangle", json!({"width": 20, "height": 20})));
    region_pattern_source(&mut s, pattern);
    ex(&mut s, "file.new", json!({}));
    path(&mut s);
    assert_refused_paste_keeps_document_and_clipboard(&mut s);
}

#[test]
fn vector_region_pattern_cannot_refer_to_the_edited_vector_itself() {
    let mut s = Session::new();
    let target = path(&mut s);
    leave(&mut s);
    region_pattern_source(&mut s, target);
    ex(&mut s, "vector.edit.begin", json!({"id": target}));
    assert_refused_paste_keeps_document_and_clipboard(&mut s);
}

#[test]
fn an_unrelated_same_document_region_pattern_stays_local_and_usable() {
    let mut s = Session::new();
    let pattern = id_of(&ex(&mut s, "shape.rectangle", json!({"width": 20, "height": 20})));
    region_pattern_source(&mut s, pattern);
    let target = path(&mut s);
    ex(&mut s, "edit.paste", json!({}));
    let network = net(&s, target);
    let figcraft_color::PaintKind::Pattern(paint) = &network.regions.last().unwrap().fills.as_ref().unwrap()[0].kind else { panic!("not pattern") };
    assert_eq!(paint.source, pattern.0);
}

#[test]
fn no_outline_refusal_keeps_copied_images_out_of_the_target_and_clipboard_unchanged() {
    let mut s = Session::new();
    let data = region_image_data([255, 0, 0, 255]);
    let text = id_of(&ex(&mut s, "text.create", json!({"text": "not an outline"})));
    s.doc_mut().unwrap().images.insert("region-image".into(), data);
    s.doc_mut().unwrap().get_mut(text).unwrap().fills = vec![figcraft_color::Paint::image("region-image", figcraft_color::ImageScaleMode::Fill)];
    ex(&mut s, "edit.copy", json!({"ids": [text]}));
    ex(&mut s, "file.new", json!({}));
    path(&mut s);
    assert_refused_paste_keeps_document_and_clipboard(&mut s);
}

#[test]
fn segment_limit_refusal_does_not_admit_copied_region_images() {
    let mut s = Session::new();
    let copied = region_image_data([255, 0, 0, 255]);
    region_image_source(&mut s, &copied, true);
    ex(&mut s, "file.new", json!({}));
    let target = path(&mut s);
    let NodeKind::Vector { network } = &mut s.doc_mut().unwrap().get_mut(target).unwrap().kind else { panic!("not vector") };
    let segment = network.segments[0];
    network.segments.resize(500_000, segment);
    assert_refused_paste_keeps_document_and_clipboard(&mut s);
}

#[test]
fn copying_one_region_and_merging_it_keep_the_original_image_paint_frame() {
    let mut s = Session::new();
    let data = region_image_data([255, 0, 0, 255]);
    let source = id_of(&ex(&mut s, "vector.create", json!({"points": [[0, 0], [100, 0], [0, 100]], "closed": true})));
    let mut other = net(&s, source);
    other.translate(100.0, 0.0);
    {
        let doc = s.doc_mut().unwrap();
        doc.images.insert("region-image".into(), data);
        let node = doc.get_mut(source).unwrap();
        let NodeKind::Vector { network } = &mut node.kind else { panic!("not vector") };
        network.regions[0].fills = Some(vec![figcraft_color::Paint::image("region-image", figcraft_color::ImageScaleMode::Fill)]);
        let (vo, so) = (network.vertices.len(), network.segments.len());
        network.vertices.extend(other.vertices);
        network.segments.extend(other.segments.into_iter().map(|mut segment| {
            segment.a += vo;
            segment.b += vo;
            segment
        }));
        network.regions.extend(other.regions.into_iter().map(|mut region| {
            for index in region.loops.iter_mut().flatten() {
                *index += so;
            }
            region
        }));
        crate::vectoredit::refit(doc, source).unwrap();
    }
    let node = s.doc().unwrap().node(source).unwrap();
    assert_eq!((node.width, node.height), (200.0, 100.0));
    let source_world = s.doc().unwrap().world_transform(source);
    ex(&mut s, "vector.edit.begin", json!({"id": source}));
    select(&mut s, &[], &[0, 1, 2]);
    ex(&mut s, "edit.copy", json!({}));
    let copied = &s.clipboard.nodes[0].1[0];
    let NodeKind::Vector { network } = &copied.kind else { panic!("not vector") };
    let (width, height, frame) = network.regions[0].paint_frame(copied.width, copied.height);
    assert_eq!((width, height), (200.0, 100.0), "partial copy retains the original layer's paint box");
    assert_eq!(copied.transform * frame, source_world, "the paint frame stays in page space");
    leave(&mut s);
    let target = path(&mut s);
    ex(&mut s, "edit.paste", json!({}));
    let node = s.doc().unwrap().node(target).unwrap();
    let NodeKind::Vector { network } = &node.kind else { panic!("not vector") };
    let (width, height, frame) = network.regions.last().unwrap().paint_frame(node.width, node.height);
    assert_eq!((width, height), (200.0, 100.0));
    assert_eq!(s.doc().unwrap().world_transform(target) * frame, source_world, "target refit preserves the source paint coordinates");
}

/// The public native-file loader admits finite transforms and finite stored radius/frame values.
/// Build bare JSON bytes, then open them through the same command available to the CLI/control UI.
fn finite_transform_file_source(
    s: &mut Session,
    transform: figcraft_geom::Affine,
    radius: f64,
    frame: Option<figcraft_doc::RegionPaintSpace>,
) -> NodeId {
    use base64::Engine;
    let mut source = Session::new();
    let from = id_of(&ex(&mut source, "vector.create", json!({"points": [[0, 0], [1, 0], [1, 1]], "closed": true})));
    let node = source.doc_mut().unwrap().get_mut(from).unwrap();
    node.transform = transform;
    let NodeKind::Vector { network } = &mut node.kind else { panic!("not vector") };
    for vertex in &mut network.vertices {
        vertex.radius = radius;
    }
    if let Some(frame) = frame {
        network.regions[0].fills = Some(vec![figcraft_color::Paint::solid(figcraft_color::Color::WHITE)]);
        network.regions[0].paint_space = Some(frame);
        assert_eq!(network.regions[0].paint_frame(1.0, 1.0).2, frame.transform, "the public source frame is valid before composition");
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(source.doc().unwrap()).unwrap());
    ex(s, "file.open", json!({"name": "finite-vector-transform.json", "data": encoded}));
    assert_eq!(s.doc().unwrap().node(from).unwrap().transform, transform, "no numeric transform cap hides this path");
    ex(s, "edit.copy", json!({"ids": [from]}));
    from
}

#[test]
fn vector_placement_native_file_copy_and_paste_keeps_a_large_finite_transform_and_zero_radii() {
    let mut s = Session::new();
    finite_transform_file_source(&mut s, figcraft_geom::Affine::scale(1e200), 0.0, None);
    let target = path(&mut s);
    let before = net(&s, target).vertices.len();
    ex(&mut s, "edit.paste", json!({}));
    let output = net(&s, target);
    assert_eq!(output.vertices.len(), before + 3);
    assert!(output.vertices.iter().all(|v| v.x.is_finite() && v.y.is_finite() && v.radius.is_finite()), "public paste must not store NaN radii");
    assert!(output.vertices[before..].iter().all(|v| v.radius == 0.0));
}

fn finite_transform_refusal(bad_frame: bool) {
    let mut s = Session::new();
    let frame = bad_frame.then_some(figcraft_doc::RegionPaintSpace {
        width: 10.0,
        height: 20.0,
        transform: figcraft_geom::Affine::new([1e308, 0.0, 0.0, 1e-308, 0.0, 0.0]),
    });
    finite_transform_file_source(&mut s, figcraft_geom::Affine::scale(2.0), if bad_frame { 0.0 } else { 1e308 }, frame);
    path(&mut s);
    assert_refused_paste_keeps_document_and_clipboard(&mut s);
}

#[test]
fn vector_placement_native_file_unrepresentable_radius_refuses_atomically() {
    finite_transform_refusal(false);
}

#[test]
fn vector_placement_native_file_unrepresentable_paint_frame_refuses_atomically() {
    finite_transform_refusal(true);
}

#[test]
fn vector_placement_native_file_finite_paint_coefficients_with_overflowed_determinant_refuse_atomically() {
    let mut s = Session::new();
    let frame = figcraft_doc::RegionPaintSpace { width: 10.0, height: 20.0, transform: figcraft_geom::Affine::scale(1e154) };
    finite_transform_file_source(&mut s, figcraft_geom::Affine::scale(2.0), 0.0, Some(frame));
    path(&mut s);
    assert_refused_paste_keeps_document_and_clipboard(&mut s);
}

#[test]
fn vector_placement_native_file_finite_paint_coefficients_with_unrepresentable_inverse_refuse_atomically() {
    let mut s = Session::new();
    let frame = figcraft_doc::RegionPaintSpace { width: 10.0, height: 20.0, transform: figcraft_geom::Affine::scale_non_uniform(1.0, 1e-307) };
    finite_transform_file_source(&mut s, figcraft_geom::Affine::scale(0.01), 0.0, Some(frame));
    path(&mut s);
    assert_refused_paste_keeps_document_and_clipboard(&mut s);
}

fn partial_bind_dimensions(s: &mut Session, source: NodeId, width: bool, height: bool) {
    ex(s, "variable.createCollection", json!({"name": "Partial metadata fixture"}));
    for (field, value, enabled) in [("width", 300.0, width), ("height", 100.0, height)] {
        if enabled {
            let name = format!("partial-{field}");
            ex(s, "variable.create", json!({"collection": "Partial metadata fixture", "name": name, "type": "number", "value": value}));
            ex(s, "variable.bind", json!({"ids": [source], "field": field, "variable": name}));
        }
    }
}

fn partial_points(s: &Session, id: NodeId) -> Vec<(f64, f64)> {
    let world = s.doc().unwrap().world_transform(id);
    net(s, id).vertices.iter().map(|vertex| world * vertex.point()).map(|point| (point.x, point.y)).collect()
}

fn partial_copy_middle(s: &mut Session) {
    select(s, &[], &[1]);
    assert_eq!(ex(s, "edit.copy", json!({}))["segments"], 1);
    let clip = &s.clipboard.nodes[0].1[0];
    assert_eq!((clip.width, clip.height), (100.0, 50.0), "the copied sub-network is already bounded correctly before paste normalization");
    leave(s);
}

fn partial_pasted_id(s: &mut Session, params: Value) -> NodeId {
    let result = ex(s, "edit.paste", params);
    params::id_of(&result["ids"][0]).unwrap()
}

fn partial_assert_fragment(s: &Session, pasted: NodeId, at: (f64, f64)) {
    let node = s.doc().unwrap().node(pasted).unwrap();
    assert_eq!((node.width, node.height), (100.0, 50.0), "a strict segment copy keeps its own box, not its source's bound size");
    assert_eq!(
        partial_points(s, pasted),
        vec![(at.0, at.1 + 50.0), (at.0 + 100.0, at.1)],
        "the copied segment's actual page points and placement are unchanged"
    );
    assert_eq!(world_rect(s, pasted), (at.0, at.1, 100.0, 50.0));
    assert_eq!(net(s, pasted).segments.len(), 1);
}

#[test]
fn vector_partial_metadata_width_binding_does_not_resize_paste_and_one_undo_restores_the_document() {
    let mut s = Session::new();
    let source = path(&mut s);
    partial_bind_dimensions(&mut s, source, true, false);
    partial_copy_middle(&mut s);
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let pasted = partial_pasted_id(&mut s, json!({}));
    partial_assert_fragment(&s, pasted, (200.0, 100.0));
    assert_eq!(s.undo_depth(), (history.0 + 1, 0));
    assert!(s.doc().unwrap().node(source).unwrap().bound.contains_key("width"), "the source binding is still authored and live");
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before, "one undo restores all source geometry and binding metadata");
    s.redo().unwrap();
    partial_assert_fragment(&s, pasted, (200.0, 100.0));
}

#[test]
fn vector_partial_metadata_height_binding_does_not_resize_explicit_placement_and_undo_redo() {
    let mut s = Session::new();
    let source = path(&mut s);
    partial_bind_dimensions(&mut s, source, false, true);
    partial_copy_middle(&mut s);
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let pasted = partial_pasted_id(&mut s, json!({"x": 700, "y": 800}));
    partial_assert_fragment(&s, pasted, (700.0, 800.0));
    assert_eq!(s.undo_depth(), (history.0 + 1, 0));
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.redo().unwrap();
    partial_assert_fragment(&s, pasted, (700.0, 800.0));
}

#[test]
fn vector_partial_metadata_fragment_does_not_follow_later_source_size_variable_revalues() {
    let mut s = Session::new();
    let source = path(&mut s);
    partial_bind_dimensions(&mut s, source, true, true);
    partial_copy_middle(&mut s);
    let pasted = partial_pasted_id(&mut s, json!({}));
    // Isolate later rebinding from the immediate paste-size regression in the other controls.
    let before = (world_rect(&s, pasted), partial_points(&s, pasted));
    ex(&mut s, "variable.set", json!({"variable": "partial-width", "value": 600}));
    ex(&mut s, "variable.set", json!({"variable": "partial-height", "value": 200}));
    let original = s.doc().unwrap().node(source).unwrap();
    assert_eq!((original.width, original.height), (600.0, 200.0), "the original full vector still follows both authored variables");
    assert_eq!((world_rect(&s, pasted), partial_points(&s, pasted)), before, "a partial fragment is not the source object's full bound dimensions");
    partial_assert_fragment(&s, pasted, (200.0, 100.0));
}

#[test]
fn vector_partial_metadata_copy_keeps_paint_stroke_variables_and_live_effect_style() {
    let mut s = Session::new();
    let source = path(&mut s);
    ex(&mut s, "variable.createCollection", json!({"name": "Partial look fixture"}));
    ex(&mut s, "variable.create", json!({"collection": "Partial look fixture", "name": "partial-ink", "type": "color", "value": "#ff0000"}));
    ex(&mut s, "variable.create", json!({"collection": "Partial look fixture", "name": "partial-weight", "type": "number", "value": 7}));
    ex(&mut s, "variable.bind", json!({"ids": [source], "field": "fill", "variable": "partial-ink"}));
    ex(&mut s, "variable.bind", json!({"ids": [source], "field": "strokeWeight", "variable": "partial-weight"}));
    ex(&mut s, "effect.add", json!({"ids": [source], "type": "dropShadow"}));
    let effect = ex(&mut s, "style.create", json!({"type": "effect", "name": "Partial shadow", "from": source}))["id"].as_str().unwrap().to_string();
    ex(&mut s, "style.apply", json!({"ids": [source], "style": effect}));
    partial_copy_middle(&mut s);
    let pasted = partial_pasted_id(&mut s, json!({}));
    partial_assert_fragment(&s, pasted, (200.0, 100.0));
    let before = s.doc().unwrap().node(pasted).unwrap();
    assert!(before.bound.contains_key("fill") || before.bound.contains_key("fills/0/color"));
    assert!(before.bound.contains_key("strokeWeight"));
    assert_eq!(before.styles.effect.as_deref(), Some(effect.as_str()));
    assert_eq!(before.effects.len(), 1);
    ex(&mut s, "variable.set", json!({"variable": "partial-ink", "value": "#0000ff"}));
    ex(&mut s, "variable.set", json!({"variable": "partial-weight", "value": 11}));
    let node = s.doc().unwrap().node(pasted).unwrap();
    assert_eq!(node.fills[0].solid_color().unwrap().to_hex(), "0000FF");
    assert_eq!(node.stroke.weight, 11.0);
    let donor = id_of(&ex(&mut s, "shape.rectangle", json!({"x": 900, "y": 900, "width": 10, "height": 10})));
    ex(&mut s, "style.update", json!({"style": effect, "from": donor}));
    assert!(s.doc().unwrap().node(pasted).unwrap().effects.is_empty(), "a retained legitimate effect-style link keeps following its definition");
    partial_assert_fragment(&s, pasted, (200.0, 100.0));
}

#[test]
fn vector_partial_metadata_whole_layer_copy_keeps_dimension_bindings_even_with_an_edit_selection() {
    let mut s = Session::new();
    let source = path(&mut s);
    partial_bind_dimensions(&mut s, source, true, true);
    select(&mut s, &[], &[1]);
    ex(&mut s, "edit.copy", json!({"ids": [source]}));
    leave(&mut s);
    let pasted = partial_pasted_id(&mut s, json!({}));
    assert_eq!(net(&s, pasted).segments.len(), 3);
    assert!(s.doc().unwrap().node(pasted).unwrap().bound.contains_key("width"));
    assert!(s.doc().unwrap().node(pasted).unwrap().bound.contains_key("height"));
    ex(&mut s, "variable.set", json!({"variable": "partial-width", "value": 600}));
    ex(&mut s, "variable.set", json!({"variable": "partial-height", "value": 200}));
    assert_eq!((s.doc().unwrap().node(pasted).unwrap().width, s.doc().unwrap().node(pasted).unwrap().height), (600.0, 200.0));
}

#[test]
fn vector_partial_metadata_all_selected_segments_keep_dimension_bindings_when_the_copied_network_is_whole() {
    let mut s = Session::new();
    let source = path(&mut s);
    partial_bind_dimensions(&mut s, source, true, true);
    select(&mut s, &[], &[0, 1, 2]);
    ex(&mut s, "edit.copy", json!({}));
    leave(&mut s);
    let pasted = partial_pasted_id(&mut s, json!({}));
    assert_eq!(net(&s, pasted).segments.len(), 3);
    ex(&mut s, "variable.set", json!({"variable": "partial-width", "value": 600}));
    ex(&mut s, "variable.set", json!({"variable": "partial-height", "value": 200}));
    let node = s.doc().unwrap().node(pasted).unwrap();
    assert_eq!((node.width, node.height), (600.0, 200.0), "the vector-edit clipboard path still preserves whole-object geometry bindings");
    assert!(node.bound.contains_key("width") && node.bound.contains_key("height"));
}

fn partial_board_connector(s: &mut Session) -> (NodeId, NodeId, NodeId) {
    ex(s, "file.newBoard", json!({"name": "Partial connector fixture"}));
    let a = id_of(&ex(s, "board.shape", json!({"x": 0, "y": 0, "width": 100, "height": 100})));
    let b = id_of(&ex(s, "board.shape", json!({"x": 300, "y": 200, "width": 100, "height": 100})));
    let connector = id_of(&ex(s, "board.connector", json!({"from": a, "to": b, "style": "elbow"})));
    assert!(matches!(s.doc().unwrap().node(connector).unwrap().board, Some(figcraft_doc::BoardItem::Connector(_))));
    assert!(net(s, connector).segments.len() > 1, "public connector fixture has a strict sub-network to select");
    ex(s, "vector.edit.begin", json!({"id": connector}));
    (a, b, connector)
}

#[test]
fn vector_partial_metadata_board_connector_pastes_only_the_segment_and_does_not_regenerate_or_follow_ends() {
    let mut s = Session::new();
    let (a, _, source) = partial_board_connector(&mut s);
    let network = net(&s, source);
    let segment = network.segments[0];
    let points = partial_points(&s, source);
    let expected = vec![points[segment.a], points[segment.b]];
    let at = (expected[0].0.min(expected[1].0), expected[0].1.min(expected[1].1));
    select(&mut s, &[], &[0]);
    ex(&mut s, "edit.copy", json!({}));
    leave(&mut s);
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let pasted = partial_pasted_id(&mut s, json!({"x": at.0, "y": at.1}));
    assert_eq!(net(&s, pasted).segments.len(), 1, "a selected connector segment must not regenerate its source's complete live route");
    assert_eq!(partial_points(&s, pasted), expected);
    assert!(s.doc().unwrap().node(pasted).unwrap().board.is_none(), "a fragment does not own the source connector's endpoint relationship");
    assert!(matches!(s.doc().unwrap().node(source).unwrap().board, Some(figcraft_doc::BoardItem::Connector(_))));
    assert_eq!(s.undo_depth(), (history.0 + 1, 0));
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.redo().unwrap();
    ex(&mut s, "node.set", json!({"id": a, "props": {"x": 50}}));
    assert_eq!(partial_points(&s, pasted), expected, "moving an original endpoint cannot reroute an independent copied fragment");
}

#[test]
fn vector_partial_metadata_whole_board_connector_copy_keeps_its_live_endpoint_relationships() {
    let mut s = Session::new();
    let (a, _, source) = partial_board_connector(&mut s);
    let all: Vec<usize> = (0..net(&s, source).segments.len()).collect();
    select(&mut s, &[], &all);
    ex(&mut s, "edit.copy", json!({}));
    leave(&mut s);
    let pasted = partial_pasted_id(&mut s, json!({}));
    assert!(matches!(s.doc().unwrap().node(pasted).unwrap().board, Some(figcraft_doc::BoardItem::Connector(_))));
    let before = partial_points(&s, pasted);
    ex(&mut s, "node.set", json!({"id": a, "props": {"x": 50}}));
    assert_ne!(partial_points(&s, pasted), before, "ordinary whole connector copy still follows the legitimate endpoints");
}

#[test]
fn vector_partial_metadata_board_ink_keeps_its_ordinary_ink_metadata_and_look() {
    let mut s = Session::new();
    ex(&mut s, "file.newBoard", json!({"name": "Partial ink fixture"}));
    let source = id_of(&ex(&mut s, "board.ink", json!({"points": [[100, 100], [200, 150], [300, 100], [400, 200]], "highlighter": true})));
    ex(&mut s, "vector.edit.begin", json!({"id": source}));
    partial_copy_middle(&mut s);
    let pasted = partial_pasted_id(&mut s, json!({}));
    partial_assert_fragment(&s, pasted, (200.0, 100.0));
    let (source, pasted) = (s.doc().unwrap().node(source).unwrap(), s.doc().unwrap().node(pasted).unwrap());
    assert!(matches!(pasted.board, Some(figcraft_doc::BoardItem::Ink)), "non-derived ink metadata remains useful to board tools");
    assert_eq!((pasted.opacity, pasted.blend, pasted.stroke.weight), (source.opacity, source.blend, source.stroke.weight));
}

fn partial_metadata_bound_property_case(field: &str, value: f64) {
    let mut s = Session::new();
    let source = path(&mut s);
    ex(&mut s, "variable.createCollection", json!({"name": "Additional geometry fixture"}));
    ex(&mut s, "variable.create", json!({"collection": "Additional geometry fixture", "name": "geometry-value", "type": "number", "value": value}));
    ex(&mut s, "variable.bind", json!({"ids": [source], "field": field, "variable": "geometry-value"}));
    assert!(s.doc().unwrap().node(source).unwrap().bound.contains_key(field), "the public binding was retained exactly as authored");
    partial_copy_middle(&mut s);
    let pasted = partial_pasted_id(&mut s, json!({"x": 700, "y": 800}));
    partial_assert_fragment(&s, pasted, (700.0, 800.0));
    assert!(s.doc().unwrap().node(source).unwrap().bound.contains_key(field), "only the independent fragment can detach object geometry");
}

#[test]
fn vector_partial_metadata_width_alias_binding_does_not_reassert_source_width() {
    partial_metadata_bound_property_case("w", 300.0);
}

#[test]
fn vector_partial_metadata_height_alias_binding_does_not_reassert_source_height() {
    partial_metadata_bound_property_case("h", 100.0);
}

#[test]
fn vector_partial_metadata_x_binding_does_not_override_explicit_fragment_placement() {
    partial_metadata_bound_property_case("x", 100.0);
}

#[test]
fn vector_partial_metadata_y_binding_does_not_override_explicit_fragment_placement() {
    partial_metadata_bound_property_case("y", 100.0);
}

fn partial_metadata_assert_points_near(s: &Session, id: NodeId, expected: &[(f64, f64)]) {
    let actual = partial_points(s, id);
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!((a.0 - e.0).abs() < 1e-8 && (a.1 - e.1).abs() < 1e-8, "actual page point {a:?} differs from copied page point {e:?}");
    }
}

#[test]
fn vector_partial_metadata_rotation_binding_does_not_strip_a_rotated_parent_when_reanchored_to_the_page() {
    let mut s = Session::new();
    let parent = id_of(&ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 500, "height": 500})));
    ex(&mut s, "node.set", json!({"id": parent, "props": {"rotation": 30}}));
    let source =
        id_of(&ex(&mut s, "vector.create", json!({"parent": parent, "points": [[100, 100], [200, 150], [300, 100], [400, 200]], "closed": false})));
    ex(&mut s, "variable.createCollection", json!({"name": "Rotation owner fixture"}));
    ex(&mut s, "variable.create", json!({"collection": "Rotation owner fixture", "name": "local-angle", "type": "number", "value": 0}));
    ex(&mut s, "variable.bind", json!({"ids": [source], "field": "rotation", "variable": "local-angle"}));
    let before_points = partial_points(&s, source);
    let expected = vec![before_points[1], before_points[2]];
    let local = s.doc().unwrap().node(source).unwrap().transform.as_coeffs();
    assert!(local[1].atan2(local[0]).abs() < 1e-9, "the public source binding is its unrotated local angle");
    ex(&mut s, "vector.edit.begin", json!({"id": source}));
    select(&mut s, &[], &[1]);
    ex(&mut s, "edit.copy", json!({}));
    let bounds = s.clipboard.bounds.unwrap();
    leave(&mut s);
    // An explicit equal bounding-box origin keeps the world shift identity, regardless of viewport.
    let pasted = partial_pasted_id(&mut s, json!({"x": bounds.x0, "y": bounds.y0}));
    partial_metadata_assert_points_near(&s, pasted, &expected);
    assert_eq!(s.doc().unwrap().node(pasted).unwrap().parent, s.page().ok());
    ex(&mut s, "variable.set", json!({"variable": "local-angle", "value": 90}));
    assert_ne!(partial_points(&s, source), before_points, "the source's legitimate local rotation binding is still live");
    partial_metadata_assert_points_near(&s, pasted, &expected);
}
