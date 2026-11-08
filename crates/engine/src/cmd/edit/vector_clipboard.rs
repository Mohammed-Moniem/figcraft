//! Copy, cut and paste in vector edit mode (#1875).
//!
//! With points or segments selected, Copy and Cut take just those: a temporary vector holding the
//! selected segments (and the segments touching a selected point) goes on the ordinary layer
//! clipboard, kept at its place on the page, so a paste outside edit mode is a plain layer paste
//! in place. Cut then removes the selection the way Delete does and stays in edit mode. Paste
//! while editing adds the clipboard's layers to the edited vector's network, in place: a vector
//! with its segments, a shape or boolean with its outline.

use figcraft_doc::{NodeKind, Region, RegionPaintSpace, VectorNetwork};

use super::clipboard::{collect_images, plan_images, remap_paint_images, store_clipboard};
use super::{Affine, BTreeMap, BTreeSet, Document, EngineError, Node, NodeId, Result, Session, Value, bad, json, num};
use crate::canvas::vectoredit::{self, EditPart};

/// The most segments a vector network takes through a paste.
const MAX_SEGMENTS: usize = 500_000;

/// What the edited layers have selected.
pub(super) struct EditSelection {
    /// Whether any edited layer has a point or a segment selected.
    pub any: bool,
    /// Each edited layer with something to copy, and the sub-network that is.
    pub parts: Vec<(NodeId, VectorNetwork)>,
}

/// The selection of the vector being edited (and the other layers edited with it), or `None` when
/// not in vector edit mode.
pub(super) fn edit_selection(s: &Session) -> Option<EditSelection> {
    let ds = s.ds().ok()?;
    let ve = ds.vector_edit.as_ref()?;
    let mut sel = EditSelection { any: false, parts: Vec::new() };
    for part in ve.parts() {
        if part.vertices.is_empty() && part.segments.is_empty() {
            continue;
        }
        sel.any = true;
        let Some(net) = vectoredit::network(&ds.doc, part.id) else { continue };
        let active = part.id == ve.id;
        // Points selected only through a selected region don't pull in the segments touching
        // them: the region's own segments are the selection.
        let vertices: Vec<usize> = part.vertices.iter().copied().filter(|v| !active || !ve.region_vertices.contains(v)).collect();
        let mut segments = part.segments.clone();
        if active {
            segments.extend(ve.regions.iter().filter_map(|r| net.regions.get(*r)).flat_map(|r| r.loops.iter().flatten().copied()));
        }
        if let Some(sub) = sub_network(net, &vertices, &segments) {
            sel.parts.push((part.id, sub));
        }
    }
    Some(sel)
}

/// The part of `net` that is the segments `segments` and those touching a point of `vertices`,
/// with the points they join; `None` when there are none (a lone point with no segment draws
/// nothing). The regions whose every segment is kept stay regions.
fn sub_network(net: &VectorNetwork, vertices: &[usize], segments: &[usize]) -> Option<VectorNetwork> {
    let picked: BTreeSet<usize> = vertices.iter().copied().collect();
    let mut keep: BTreeSet<usize> = segments.iter().copied().filter(|i| *i < net.segments.len()).collect();
    for (i, sg) in net.segments.iter().enumerate() {
        if picked.contains(&sg.a) || picked.contains(&sg.b) {
            keep.insert(i);
        }
    }
    let used: BTreeSet<usize> = keep.iter().filter_map(|i| net.segments.get(*i)).flat_map(|sg| [sg.a, sg.b]).collect();
    let vmap: BTreeMap<usize, usize> = used.iter().enumerate().map(|(new, old)| (*old, new)).collect();
    let smap: BTreeMap<usize, usize> = keep.iter().enumerate().map(|(new, old)| (*old, new)).collect();
    let mut out = VectorNetwork {
        vertices: used.iter().filter_map(|v| net.vertices.get(*v).cloned()).collect(),
        segments: keep
            .iter()
            .filter_map(|i| net.segments.get(*i))
            .filter_map(|sg| {
                let mut sg = *sg;
                sg.a = *vmap.get(&sg.a)?;
                sg.b = *vmap.get(&sg.b)?;
                Some(sg)
            })
            .collect(),
        regions: Vec::new(),
    };
    if out.segments.is_empty() || out.segments.len() != keep.len() || out.vertices.len() != used.len() {
        return None;
    }
    for r in &net.regions {
        let loops: Option<Vec<Vec<usize>>> = r.loops.iter().map(|l| l.iter().map(|i| smap.get(i).copied()).collect::<Option<Vec<usize>>>()).collect();
        if let Some(loops) = loops.filter(|l| !l.is_empty()) {
            out.regions.push(Region { loops, ..r.clone() });
        }
    }
    Some(out)
}

/// Retained region paints use the source layer's box even when copying only one contour
/// or merging into a target whose bounds differ. Freeze the fallback before moving the network.
fn keep_region_paint_frames(net: &mut VectorNetwork, node: &Node) {
    for region in &mut net.regions {
        if region.fills.is_some() {
            let (width, height, transform) = region.paint_frame(node.width, node.height);
            region.paint_space = Some(RegionPaintSpace { width, height, transform });
        }
    }
}

/// The clipboard's layer for edited vector `id` holding `sub`: it keeps the layer's look (fills,
/// strokes, effects, opacity, blend) and its place on the page. A variable width profile runs
/// along the whole path, so it stays only when every segment is copied.
fn clip_node(doc: &Document, id: NodeId, mut sub: VectorNetwork) -> Option<Node> {
    let mut n = doc.node(id)?.clone();
    let whole = vectoredit::network(doc, id).is_some_and(|all| all.segments.len() == sub.segments.len());
    let b = sub.bounds().filter(|b| b.x0.is_finite() && b.y0.is_finite() && b.width().is_finite() && b.height().is_finite())?;
    keep_region_paint_frames(&mut sub, &n);
    sub.translate(-b.x0, -b.y0);
    n.transform = doc.world_transform(id) * Affine::translate((b.x0, b.y0));
    n.width = b.width();
    n.height = b.height();
    n.kind = NodeKind::Vector { network: sub };
    n.parent = None;
    n.children.clear();
    n.name = "Vector".into();
    if !whole {
        n.stroke.width_profile = None;
        // A strict sub-network owns its new box and page placement. Source-object geometry
        // variables would resize, move or rotate that fragment during normal paste normalization.
        for field in ["width", "height", "w", "h", "x", "y", "rotation"] {
            n.bound.remove(field);
        }
        // An extracted connector segment is an independent vector, not a live route between
        // the source connector's endpoints. Other board metadata (notably Ink) stays useful.
        if matches!(n.board, Some(figcraft_doc::BoardItem::Connector(_))) {
            n.board = None;
        }
    }
    Some(n)
}

/// Put the selection's sub-networks on the layer clipboard. Returns how many segments went.
fn copy_parts(s: &mut Session, cmd: &str, sel: &EditSelection) -> Result<usize> {
    let doc = s.doc()?;
    let mut nodes = Vec::new();
    let mut images = BTreeMap::new();
    let mut bounds: Option<figcraft_geom::Rect> = None;
    let mut segments = 0;
    for (id, sub) in &sel.parts {
        segments += sub.segments.len();
        let Some(n) = clip_node(doc, *id, sub.clone()) else { continue };
        let r = figcraft_geom::transform_rect_bbox(n.transform, n.local_rect());
        bounds = Some(bounds.map_or(r, |b| b.union(r)));
        collect_images(doc, std::slice::from_ref(&n), &mut images);
        nodes.push((*id, vec![n]));
    }
    if nodes.is_empty() {
        return Err(EngineError::Disabled(cmd.into(), "the selection has nothing to copy".into()));
    }
    store_clipboard(s, cmd, nodes, images, bounds)?;
    Ok(segments)
}

/// Copy the selected segments (see [`edit_selection`]).
pub(super) fn copy_selection(s: &mut Session, cmd: &str, sel: EditSelection) -> Result<Value> {
    let segments = copy_parts(s, cmd, &sel)?;
    Ok(json!({"copied": s.clipboard.nodes.len(), "segments": segments}))
}

/// Cut: copy the selected segments, then remove the selection as Delete does (the path splits in
/// the same layer and edit mode stays on). A selection that has no segment to copy (lone points
/// with nothing attached) is removed and the clipboard is left as it was.
pub(super) fn cut_selection(s: &mut Session, sel: EditSelection) -> Result<Value> {
    let segments = if sel.parts.is_empty() { 0 } else { copy_parts(s, "edit.cut", &sel)? };
    let parts: Vec<EditPart> = vectoredit::editing(s)?.parts();
    vectoredit::delete_selection(s, parts, false)?;
    Ok(json!({"cut": sel.parts.len(), "segments": segments}))
}

/// The network of the outline of copied layer `n` in its own coordinates: a vector's segments, a
/// shape's or boolean's outline. `None` for the layers with no outline to add (text, images,
/// frames, groups, instances, …). `doc` is the file the layer was copied from.
fn outline_network(doc: &Document, n: &Node) -> Option<VectorNetwork> {
    let mut net = match &n.kind {
        NodeKind::Vector { network } => network.clone(),
        k if k.is_shape() || matches!(k, NodeKind::BooleanOp { .. }) => {
            let (path, rule) = figcraft_render::node_outline(doc, n)?;
            let mut net = VectorNetwork::from_path(&path);
            if matches!(n.kind, NodeKind::Line) {
                net.regions.clear();
            }
            if rule == figcraft_render::FillRule::EvenOdd {
                for r in &mut net.regions {
                    r.winding = figcraft_doc::WindingRule::EvenOdd;
                }
            }
            net
        }
        _ => return None,
    };
    keep_region_paint_frames(&mut net, n);
    (!net.segments.is_empty()).then_some(net)
}

/// `net` moved by `m` (points, handles, the regions' paint frames and the points' radii).
fn placed(mut net: VectorNetwork, m: Affine) -> Option<VectorNetwork> {
    let c = m.as_coeffs();
    if !c.iter().all(|x| x.is_finite()) {
        return None;
    }
    let linear = Affine::new([c[0], c[1], c[2], c[3], 0.0, 0.0]);
    let scale = radius_scale(m);
    for v in &mut net.vertices {
        let p = m * v.point();
        (v.x, v.y) = (p.x, p.y);
        v.radius *= scale;
    }
    for sg in &mut net.segments {
        let (ta, tb) = (linear * figcraft_geom::Point::new(sg.ta.0, sg.ta.1), linear * figcraft_geom::Point::new(sg.tb.0, sg.tb.1));
        sg.ta = (ta.x, ta.y);
        sg.tb = (tb.x, tb.y);
    }
    for r in &mut net.regions {
        if let Some(frame) = r.paint_space.as_mut() {
            frame.transform = m * frame.transform;
        }
    }
    finite_placement(&net).then_some(net)
}

/// The geometric-mean length scale without squaring huge or tiny finite coefficients.
/// Keep ordinary determinant arithmetic exact as before; only its overflow/underflow uses
/// square-root products, with hypot for addition and a factored difference of squares.
fn radius_scale(m: Affine) -> f64 {
    let determinant = m.determinant();
    if determinant.is_finite() && determinant != 0.0 {
        return determinant.abs().sqrt();
    }
    let [a, b, c, d, ..] = m.as_coeffs();
    let diagonal = a.abs().sqrt() * d.abs().sqrt();
    let off_diagonal = b.abs().sqrt() * c.abs().sqrt();
    let opposite_signs = (a.is_sign_negative() != d.is_sign_negative()) != (b.is_sign_negative() != c.is_sign_negative());
    if opposite_signs {
        return diagonal.hypot(off_diagonal);
    }
    let (large, small) = (diagonal.max(off_diagonal), diagonal.min(off_diagonal));
    if large == 0.0 { 0.0 } else { (large - small).sqrt() * large.sqrt() * (1.0 + small / large).sqrt() }
}

/// A retained paint frame must satisfy the renderer's existing paint_frame contract;
/// otherwise it would silently fall back and lose the copied paint coordinates.
fn usable_paint_frame(frame: RegionPaintSpace) -> bool {
    let determinant = frame.transform.determinant();
    frame.width.is_finite()
        && frame.height.is_finite()
        && frame.width > 0.0
        && frame.height > 0.0
        && frame.transform.as_coeffs().iter().all(|value| value.is_finite())
        && determinant.is_finite()
        && determinant != 0.0
        && frame.transform.inverse().as_coeffs().iter().all(|value| value.is_finite())
}

/// A transformed network is admitted only when every persistent numeric output fits f64.
fn finite_placement(net: &VectorNetwork) -> bool {
    net.vertices.iter().all(|v| [v.x, v.y, v.radius].iter().all(|value| value.is_finite()))
        && net.segments.iter().all(|sg| [sg.ta.0, sg.ta.1, sg.tb.0, sg.tb.1].iter().all(|value| value.is_finite()))
        && net.regions.iter().all(|region| region.paint_space.is_none_or(usable_paint_frame))
}

/// Paste while editing a vector: the clipboard's layers join its network in place (with `x`, `y`:
/// the copied top-left moved there), their points selected. Layers with no outline are skipped
/// (named in `skipped`); when none has one, nothing is pasted.
pub(super) fn paste_into_vector(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "edit.paste";
    let clip = s.clipboard.clone();
    if clip.nodes.is_empty() {
        return Err(EngineError::Disabled(CMD.into(), "the clipboard is empty".into()));
    }
    let target = vectoredit::editing(s)?.id;
    let shift = match (num(p, "x"), num(p, "y"), clip.bounds) {
        (Some(x), Some(y), Some(b)) => Affine::translate((x - b.x0, y - b.y0)),
        _ => Affine::IDENTITY,
    };
    let live = s.doc()?;
    let source: &Document = clip.source.as_deref().unwrap_or(live);
    let to_local = live.world_transform(target).inverse() * shift;
    let mut pieces = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for (_, nodes) in &clip.nodes {
        let Some(root) = nodes.first() else { continue };
        match outline_network(source, root).and_then(|net| placed(net, to_local * root.transform)) {
            Some(net) => pieces.push(net),
            None => skipped.push(root.name.clone()),
        }
    }
    if pieces.is_empty() {
        return Err(EngineError::Disabled(
            CMD.into(),
            "none of the copied layers has an outline to add to the vector (text, frames and groups have none)".into(),
        ));
    }
    let have = vectoredit::network(live, target).map_or(0, |n| n.segments.len());
    if have.saturating_add(pieces.iter().map(|n| n.segments.len()).sum::<usize>()) > MAX_SEGMENTS {
        return Err(EngineError::Disabled(CMD.into(), format!("a vector takes at most {MAX_SEGMENTS} segments")));
    }
    // Resolve asset ownership before changing either the document or the clipboard. Patterns
    // name nodes in one source document, just as fill.paste's existing policy requires.
    let mut images = BTreeMap::new();
    for paint in pieces.iter().flat_map(|net| &net.regions).filter_map(|r| r.fills.as_ref()).flatten() {
        match &paint.kind {
            figcraft_color::PaintKind::Pattern(pattern) => {
                if s.ds()?.uid != clip.source_doc {
                    return Err(bad(CMD, "a pattern can't be pasted into another file"));
                }
                if live.is_ancestor(target, NodeId(pattern.source)) {
                    return Err(bad(CMD, "a layer can't be filled with itself or a layer inside it"));
                }
            }
            figcraft_color::PaintKind::Image(image) => {
                let data = clip
                    .images
                    .get(&image.image)
                    .or_else(|| source.images.get(&image.image))
                    .ok_or_else(|| bad(CMD, "a copied vector region is missing its image data"))?;
                images.insert(image.image.clone(), data.clone());
            }
            _ => {}
        }
    }
    let image_import = plan_images(CMD, live, &images)?;
    let mut merged = vectoredit::network(live, target).cloned().ok_or_else(|| bad(CMD, "not a vector"))?;
    let mut pasted = Vec::new();
    for mut piece in pieces {
        for region in &mut piece.regions {
            if let Some(fills) = &mut region.fills {
                remap_paint_images(fills, &image_import.remap);
            }
        }
        let (vo, so) = (merged.vertices.len(), merged.segments.len());
        pasted.extend(vo..vo + piece.vertices.len());
        merged.vertices.extend(piece.vertices);
        merged.segments.extend(piece.segments.into_iter().map(|mut segment| {
            segment.a += vo;
            segment.b += vo;
            segment
        }));
        merged.regions.extend(piece.regions.into_iter().map(|mut region| {
            for i in region.loops.iter_mut().flatten() {
                *i += so;
            }
            region
        }));
    }
    // The complete network and image map are now validated. Refit uses this already-existing
    // vector; only after its edit succeeds are the planned image additions committed.
    let doc = s.doc_mut()?;
    vectoredit::edit(doc, target, |net| *net = merged)?;
    image_import.commit(doc);
    let selected = pasted.clone();
    vectoredit::set_state(s, |ve| {
        ve.clear_selection();
        ve.clear_regions();
        ve.vertices = selected;
        ve.anchor = None;
        ve.pen_out = None;
        ve.width_point = None;
    })?;
    let mut out = json!({"ids": [], "edited": target, "vertices": pasted.len()});
    if !skipped.is_empty() {
        out["skipped"] = json!(skipped);
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod finite_transform_tests {
    use super::*;
    use figcraft_doc::Segment;
    use figcraft_geom::Point;

    fn network(radius: f64) -> VectorNetwork {
        let mut out = VectorNetwork::from_points(&[Point::new(0.0, 0.0), Point::new(1.0, 0.0), Point::new(1.0, 1.0)], true);
        for vertex in &mut out.vertices {
            vertex.radius = radius;
        }
        out
    }

    fn with_frame(mut net: VectorNetwork, frame: RegionPaintSpace) -> VectorNetwork {
        net.regions[0].paint_space = Some(frame);
        net
    }

    fn relative(actual: f64, expected: f64) {
        assert!(actual.is_finite(), "the stored output is finite: {actual}");
        assert!((actual / expected - 1.0).abs() < 1e-12, "{actual} versus {expected}");
    }

    #[test]
    fn vector_placement_keeps_large_and_tiny_finite_uniform_scales_without_losing_radii() {
        for factor in [1e200, 1e-200] {
            for radius in [0.0, 0.25, 1.0] {
                let output = placed(network(radius), Affine::scale(factor)).unwrap_or_else(|| panic!("finite points, handles and radii fit f64"));
                for vertex in &output.vertices {
                    assert!(vertex.x.is_finite() && vertex.y.is_finite() && vertex.radius.is_finite());
                    if radius == 0.0 {
                        assert_eq!(vertex.radius, 0.0, "a sharp vertex never becomes NaN");
                    } else {
                        relative(vertex.radius, radius * factor);
                    }
                }
                relative(output.vertices[1].x, factor);
            }
        }
    }

    #[test]
    fn vector_placement_handles_large_determinant_cancellation_and_opposite_product_signs() {
        let rank_one = Affine::new([1e200, 1e200, 1e200, 1e200, 0.0, 0.0]);
        let collapsed = placed(network(1.0), rank_one).unwrap();
        assert!(collapsed.vertices.iter().all(|v| v.radius == 0.0 && v.x.is_finite() && v.y.is_finite()));
        let reflected = Affine::new([1e200, 1e200, 1e200, -1e200, 0.0, 0.0]);
        let output = placed(network(1.0), reflected).unwrap();
        relative(output.vertices[0].radius, std::f64::consts::SQRT_2 * 1e200);
        let anisotropic = Affine::new([1e200, 0.0, 0.0, 1e-200, 0.0, 0.0]);
        let output = placed(network(3.0), anisotropic).unwrap();
        relative(output.vertices[0].radius, 3.0);
    }

    #[test]
    fn vector_placement_refuses_nonfinite_matrix_vertices_radii_and_handles() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut coefficients = Affine::IDENTITY.as_coeffs();
            for index in 0..6 {
                let old = coefficients[index];
                coefficients[index] = bad;
                assert!(placed(network(0.0), Affine::new(coefficients)).is_none(), "matrix coefficient {index}");
                coefficients[index] = old;
            }
            let mut point = network(0.0);
            point.vertices[0].x = bad;
            assert!(placed(point, Affine::IDENTITY).is_none());
            let mut point = network(0.0);
            point.vertices[0].y = bad;
            assert!(placed(point, Affine::IDENTITY).is_none());
            assert!(placed(network(bad), Affine::IDENTITY).is_none(), "stored radius {bad}");
            for component in 0..4 {
                let mut handles = network(0.0);
                let mut values = [0.0; 4];
                values[component] = bad;
                handles.segments[0] = Segment { a: 0, b: 1, ta: (values[0], values[1]), tb: (values[2], values[3]) };
                assert!(placed(handles, Affine::IDENTITY).is_none(), "handle component {component}");
            }
        }
    }

    #[test]
    fn vector_placement_refuses_nonfinite_stored_paint_frame_dimensions_and_coefficients() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for component in 0..8 {
                let mut frame = RegionPaintSpace { width: 10.0, height: 20.0, transform: Affine::IDENTITY };
                if component == 0 {
                    frame.width = bad;
                } else if component == 1 {
                    frame.height = bad;
                } else {
                    let mut coefficients = frame.transform.as_coeffs();
                    coefficients[component - 2] = bad;
                    frame.transform = Affine::new(coefficients);
                }
                assert!(placed(with_frame(network(0.0), frame), Affine::IDENTITY).is_none(), "paint frame component {component}");
            }
        }
    }

    #[test]
    fn vector_placement_refuses_finite_inputs_when_the_stored_radius_overflows() {
        assert!(placed(network(f64::MAX), Affine::scale(2.0)).is_none(), "finite radius input may overflow after scale");
    }

    #[test]
    fn vector_placement_refuses_finite_inputs_when_the_stored_paint_transform_overflows() {
        let frame = RegionPaintSpace { width: 10.0, height: 20.0, transform: Affine::new([f64::MAX, 0.0, 0.0, 1e-308, 0.0, 0.0]) };
        assert!(placed(with_frame(network(0.0), frame), Affine::scale(2.0)).is_none(), "finite paint coefficients may overflow during composition");
    }

    #[test]
    fn vector_placement_preserves_ordinary_rotation_handles_radii_and_region_paint_coordinates() {
        let mut net = network(1.0);
        net.segments[0].ta = (0.25, 0.5);
        net.segments[0].tb = (-0.5, 0.25);
        let frame = RegionPaintSpace { width: 10.0, height: 20.0, transform: Affine::translate((4.0, 5.0)) };
        let transform = Affine::new([0.0, 2.0, -3.0, 0.0, 10.0, 20.0]);
        let output = placed(with_frame(net, frame), transform).unwrap();
        assert_eq!((output.vertices[0].x, output.vertices[0].y), (10.0, 20.0));
        assert_eq!((output.vertices[2].x, output.vertices[2].y), (7.0, 22.0));
        assert_eq!(output.segments[0].ta, (-1.5, 0.5));
        assert_eq!(output.segments[0].tb, (-0.75, -1.0));
        assert_eq!(output.vertices[0].radius, 6.0_f64.sqrt(), "ordinary determinant arithmetic stays unchanged");
        let output_frame = output.regions[0].paint_space.unwrap();
        assert_eq!((output_frame.width, output_frame.height), (10.0, 20.0));
        assert_eq!(output_frame.transform.as_coeffs(), [0.0, 2.0, -3.0, 0.0, -5.0, 28.0]);
    }

    fn identity_frame() -> RegionPaintSpace {
        RegionPaintSpace { width: 10.0, height: 20.0, transform: Affine::IDENTITY }
    }

    #[test]
    fn vector_placement_refuses_a_finite_paint_matrix_when_its_determinant_overflows() {
        assert!(
            placed(with_frame(network(0.0), identity_frame()), Affine::scale(1e200)).is_none(),
            "the renderer would discard a finite-coefficient paint frame with infinite determinant"
        );
    }

    #[test]
    fn vector_placement_refuses_singular_or_underflowed_paint_frames_without_banning_collapsed_geometry() {
        for scale in [0.0, 1e-200] {
            assert!(
                placed(with_frame(network(0.0), identity_frame()), Affine::scale(scale)).is_none(),
                "the renderer would discard a paint frame with zero determinant"
            );
            assert!(
                placed(network(0.0), Affine::scale(scale)).is_some(),
                "the paint-frame requirement does not add a general minimum geometry scale"
            );
        }
    }

    #[test]
    fn vector_placement_refuses_a_finite_nonzero_paint_determinant_with_unrepresentable_inverse() {
        let transform = Affine::scale_non_uniform(1.0, 1e-309);
        assert!(transform.determinant().is_finite() && transform.determinant() != 0.0);
        assert!(!transform.inverse().as_coeffs().iter().all(|value| value.is_finite()));
        assert!(placed(with_frame(network(0.0), identity_frame()), transform).is_none());
    }

    #[test]
    fn vector_placement_refuses_nonpositive_paint_dimensions_instead_of_silently_falling_back() {
        for dimension in [0.0, -1.0] {
            for width in [false, true] {
                let mut frame = identity_frame();
                if width {
                    frame.width = dimension;
                } else {
                    frame.height = dimension;
                }
                assert!(placed(with_frame(network(0.0), frame), Affine::IDENTITY).is_none());
            }
        }
    }

    #[test]
    fn vector_placement_keeps_a_large_finite_invertible_paint_frame_instead_of_capping_coordinates() {
        let transform = Affine::scale_non_uniform(1e200, 1e-200);
        let output = placed(with_frame(network(1.0), identity_frame()), transform).unwrap();
        let frame = output.regions[0].paint_space.unwrap();
        assert_eq!(frame.transform, transform);
        assert_eq!(output.regions[0].paint_frame(1.0, 1.0), (10.0, 20.0, transform), "the actual paint consumer retains the frame");
        assert_eq!(output.vertices[0].radius, 1.0);
    }
}
