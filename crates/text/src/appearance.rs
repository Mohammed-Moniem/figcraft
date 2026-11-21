//! Portable text geometry: no installed font is needed to display an unchanged text layer.
use std::sync::Arc;

use figcraft_doc::{Document, NodeKind, TextAppearance, TextAutoResize, TextData, TextDecorationPath, TextGlyph, TextLine};
use figcraft_geom::{Affine, BezPath, Rect};
use sha2::{Digest, Sha256};

use crate::{Decoration, Glyph, Line, TextLayout, no_coords};

const MAX_GLYPHS: usize = 100_000;
const MAX_PATH_BYTES: usize = 16 * 1024 * 1024;

/// All named faces must exist before reshaping or capturing; never preserve a substitute as
/// though it were the author's font. A variable font's named instance counts as that face.
pub fn fonts_available(t: &TextData) -> bool {
    fonts_available_in(crate::db(), t)
}

fn fonts_available_in(database: &std::sync::RwLock<crate::FontDb>, t: &TextData) -> bool {
    let styles = || std::iter::once(&t.style).chain(t.runs.iter().map(|r| &r.style));
    {
        let db = database.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut complete = true;
        for s in styles() {
            match db.cached_named_face(&s.font_family, &s.font_style) {
                Some(false) => return false,
                Some(true) => {}
                None => complete = false,
            }
        }
        if complete {
            return true;
        }
    }
    let mut db = database.write().unwrap_or_else(std::sync::PoisonError::into_inner);
    styles().all(|s| db.named_face_available(&s.font_family, &s.font_style))
}

fn fingerprint(t: &TextData, w: Option<f64>, h: Option<f64>) -> Option<String> {
    // The immutable appearance is shared: cloning editable inputs never copies glyph paths.
    let mut source = t.clone();
    source.appearance = None;
    source.native_cache = None;
    source.native_face = None;
    source.style.style_id = None;
    source.style.link = None;
    for run in &mut source.runs {
        run.style.style_id = None;
        run.style.link = None;
    }
    let (w, h) = if t.path.is_some() {
        (None, None)
    } else {
        match t.auto_resize {
            TextAutoResize::WidthAndHeight => (None, None),
            TextAutoResize::Height => (w, None),
            TextAutoResize::None => (w, None),
            TextAutoResize::Truncate => (w, h),
        }
    };
    let bytes = serde_json::to_vec(&(source, w, h)).ok()?;
    Some(format!("{:x}", Sha256::digest(bytes)))
}

/// Only protect/use outlines that still describe this text, not a snapshot left behind by
/// an explicit font replacement, an imported style update or a direct document edit.
pub fn has_saved(t: &TextData, w: Option<f64>, h: Option<f64>) -> bool {
    t.appearance.as_ref().is_some_and(|a| fingerprint(t, w, h).is_some_and(|key| key == a.fingerprint))
}

pub(crate) fn restore(t: &TextData, w: Option<f64>, h: Option<f64>) -> Option<TextLayout> {
    let a = t.appearance.as_ref()?;
    if fonts_available(t)
        || a.fingerprint != fingerprint(t, w, h)?
        || a.glyphs.len() > MAX_GLYPHS
        || a.lines.len() > MAX_GLYPHS
        || a.decorations.len() > MAX_GLYPHS
        || !a.width.is_finite()
        || !a.height.is_finite()
        || !a.box_width.is_finite()
        || !a.box_height.is_finite()
        || a.styles.len() > MAX_GLYPHS
    {
        return None;
    }
    let chars = t.char_count();
    let mut bytes = 0usize;
    let glyphs = a
        .glyphs
        .iter()
        .map(|g| {
            bytes = bytes.saturating_add(g.d.len());
            if bytes > MAX_PATH_BYTES
                || g.style >= a.styles.len()
                || g.char_start > g.char_end
                || g.char_end > chars
                || ![g.x, g.y, g.size, g.advance].iter().all(|v| v.is_finite())
            {
                return None;
            }
            let path = BezPath::from_svg(&g.d).ok()?;
            if !path.is_finite() {
                return None;
            }
            Some(Glyph {
                face: None,
                saved_path: Some(Arc::new(path)),
                coords: no_coords(),
                gid: 0,
                x: g.x,
                y: g.y,
                size: g.size,
                advance: g.advance,
                char_start: g.char_start,
                char_end: g.char_end,
                style: g.style,
                rtl: g.rtl,
                warp: Affine::IDENTITY,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let mut previous_end = 0;
    let lines = a
        .lines
        .iter()
        .map(|l| {
            if l.glyph_start < previous_end
                || l.glyph_start > l.glyph_end
                || l.glyph_end > glyphs.len()
                || l.char_start > l.char_end
                || l.char_end > chars
                || ![l.top, l.height, l.baseline, l.width, l.x].iter().all(|v| v.is_finite())
            {
                return None;
            }
            previous_end = l.glyph_end;
            Some(Line {
                glyph_start: l.glyph_start,
                glyph_end: l.glyph_end,
                char_start: l.char_start,
                char_end: l.char_end,
                top: l.top,
                height: l.height,
                baseline: l.baseline,
                width: l.width,
                x: l.x,
                para_end: l.para_end,
                break_after: l.break_after,
                rtl: l.rtl,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let decorations = a
        .decorations
        .iter()
        .map(|d| {
            if d.style >= a.styles.len() || !d.rect.iter().all(|v| v.is_finite()) || !d.thickness.is_finite() {
                return None;
            }
            Some(Decoration { rect: Rect::new(d.rect[0], d.rect[1], d.rect[2], d.rect[3]), style: d.style, kind: d.kind, thickness: d.thickness })
        })
        .collect::<Option<Vec<_>>>()?;
    // Saved geometry owns drawing styles, never hyperlink destinations. Resolve current
    // editable metadata by character span, including links changed after capture.
    let mut glyphs = glyphs;
    let mut styles = a.styles.clone();
    for style in &mut styles {
        style.link = None;
        style.style_id = None;
    }
    let spans = t.spans();
    let mut mapped = std::collections::BTreeMap::new();
    for glyph in &mut glyphs {
        let index = spans.partition_point(|(_, end, _)| *end <= glyph.char_start).min(spans.len().saturating_sub(1));
        let Some((_, _, current)) = spans.get(index) else {
            continue;
        };
        if current.link.is_none() && current.style_id.is_none() {
            continue;
        }
        let mut style = styles.get(glyph.style).cloned()?;
        let key = (glyph.style, index);
        glyph.style = *mapped.entry(key).or_insert_with(|| {
            style.link = current.link.clone();
            style.style_id = current.style_id.clone();
            let id = styles.len();
            styles.push(style);
            id
        });
    }
    Some(TextLayout { glyphs, lines, decorations, styles, width: a.width, height: a.height, truncated: a.truncated, on_path: None })
}

/// Add/update appearances on machines with the original fonts. Existing valid geometry is
/// retained on a guest missing fonts. Returns the number of changed text layers.
pub fn capture(doc: &mut Document, ids: Option<&[figcraft_doc::NodeId]>) -> usize {
    capture_impl(doc, ids, false).captured
}

/// Portable appearances added/replaced or discarded because the current face cannot be captured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshResult {
    /// New or replaced portable appearances.
    pub captured: usize,
    /// Stale appearances removed after a current available face could not be captured.
    pub discarded: usize,
}

/// Refresh portable geometry after a named face changes, even when editable inputs and the
/// text box stayed the same. Missing named faces keep their existing portable geometry.
pub fn refresh_fonts(doc: &mut Document, ids: Option<&[figcraft_doc::NodeId]>) -> RefreshResult {
    // Portable outlines are derived caches. Preserve every preexisting authored dirty id;
    // cache-only node changes must not become new edits of read-only library definitions.
    let pending = doc.take_dirty();
    let result = capture_impl(doc, ids, true);
    doc.take_dirty();
    doc.mark_dirty(pending);
    result
}

fn discard_stale_on_refresh(doc: &mut Document, id: figcraft_doc::NodeId, refresh: bool) -> usize {
    if !refresh || !doc.node(id).is_some_and(|n| matches!(&n.kind, NodeKind::Text(t) if t.appearance.is_some())) {
        return 0;
    }
    if let Ok(n) = doc.get_mut(id)
        && let NodeKind::Text(t) = &mut n.kind
    {
        t.appearance = None;
        return 1;
    }
    0
}

fn capture_impl(doc: &mut Document, ids: Option<&[figcraft_doc::NodeId]>, refresh: bool) -> RefreshResult {
    let ids = ids.map_or_else(|| doc.nodes().map(|n| n.id).collect::<Vec<_>>(), |v| v.to_vec());
    let mut changed = RefreshResult::default();
    for id in ids {
        let Some(n) = doc.node(id) else { continue };
        let NodeKind::Text(t) = &n.kind else { continue };
        let Some(key) = fingerprint(t, Some(n.width), Some(n.height)) else {
            continue;
        };
        if (!refresh && t.appearance.as_ref().is_some_and(|a| a.fingerprint == key)) || !fonts_available(t) {
            continue;
        }
        let Some((l, _)) = crate::layout_node(n) else {
            changed.discarded += discard_stale_on_refresh(doc, id, refresh);
            continue;
        };
        if l.glyphs.len() > MAX_GLYPHS || l.glyphs.iter().any(|g| g.color(1.0).is_some()) {
            changed.discarded += discard_stale_on_refresh(doc, id, refresh);
            continue;
        }
        let glyphs: Vec<_> = l
            .glyphs
            .iter()
            .map(|g| TextGlyph {
                d: g.path().to_svg(),
                x: g.x,
                y: g.y,
                size: g.size,
                advance: g.advance,
                char_start: g.char_start,
                char_end: g.char_end,
                style: g.style,
                rtl: g.rtl,
            })
            .collect();
        if glyphs.iter().map(|g| g.d.len()).sum::<usize>() > MAX_PATH_BYTES {
            changed.discarded += discard_stale_on_refresh(doc, id, refresh);
            continue;
        }
        let a = TextAppearance {
            fingerprint: key,
            box_width: n.width,
            box_height: n.height,
            width: l.width,
            height: l.height,
            glyphs,
            lines: l
                .lines
                .iter()
                .map(|v| TextLine {
                    glyph_start: v.glyph_start,
                    glyph_end: v.glyph_end,
                    char_start: v.char_start,
                    char_end: v.char_end,
                    top: v.top,
                    height: v.height,
                    baseline: v.baseline,
                    width: v.width,
                    x: v.x,
                    para_end: v.para_end,
                    break_after: v.break_after,
                    rtl: v.rtl,
                })
                .collect(),
            decorations: l
                .decorations
                .iter()
                .map(|d| TextDecorationPath {
                    rect: [d.rect.x0, d.rect.y0, d.rect.x1, d.rect.y1],
                    style: d.style,
                    kind: d.kind,
                    thickness: d.thickness,
                })
                .collect(),
            styles: l.styles.clone(),
            truncated: l.truncated,
        };
        if t.appearance.as_ref().is_some_and(|old| **old == a) {
            continue;
        }
        if let Ok(n) = doc.get_mut(id)
            && let NodeKind::Text(t) = &mut n.kind
        {
            t.appearance = Some(Arc::new(a));
            changed.captured += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic() -> TextData {
        let mut t = TextData::new("X");
        t.style.font_family = "Synthetic absent portable font".into();
        let a = TextAppearance {
            fingerprint: fingerprint(&t, Some(80.0), Some(40.0)).unwrap(),
            box_width: 80.0,
            box_height: 40.0,
            width: 25.0,
            height: 40.0,
            glyphs: vec![TextGlyph {
                d: "M0 0L25 0L25 40Z".into(),
                x: 0.0,
                y: 30.0,
                size: 40.0,
                advance: 25.0,
                char_start: 0,
                char_end: 1,
                style: 0,
                rtl: false,
            }],
            lines: vec![TextLine {
                glyph_start: 0,
                glyph_end: 1,
                char_start: 0,
                char_end: 1,
                top: 0.0,
                height: 40.0,
                baseline: 30.0,
                width: 25.0,
                x: 0.0,
                para_end: true,
                break_after: false,
                rtl: false,
            }],
            decorations: vec![],
            styles: vec![t.style.clone()],
            truncated: false,
        };
        t.appearance = Some(Arc::new(a));
        t
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn warm_missing_face_check_does_not_require_a_database_write_lock() {
        let t = synthetic();
        let database = Arc::new(std::sync::RwLock::new(crate::FontDb::default()));
        assert!(!fonts_available_in(&database, &t));
        let read = database.read().unwrap();
        let worker_database = database.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || tx.send(fonts_available_in(&worker_database, &t)).unwrap());
        let result = rx.recv_timeout(std::time::Duration::from_secs(5));
        // Always release and join, even if the regression needs the write lock.
        drop(read);
        worker.join().unwrap();
        assert!(!result.unwrap());
    }

    #[test]
    fn editing_inputs_and_fingerprinting_share_large_geometry() {
        let t = synthetic();
        let cloned = t.clone();
        assert!(Arc::ptr_eq(t.appearance.as_ref().unwrap(), cloned.appearance.as_ref().unwrap()));
        assert!(has_saved(&cloned, Some(80.0), Some(40.0)));
        assert_eq!(Arc::strong_count(t.appearance.as_ref().unwrap()), 2);
    }

    #[test]
    fn portable_outlines_do_not_change_saved_line_break_identity() {
        let mut t = synthetic();
        t.characters = "XX XX".into();
        t.appearance = None;
        t.save_breaks(vec![3], 80.0);
        let key = t.layout_key();
        t.appearance = synthetic().appearance;
        assert_eq!(t.layout_key(), key);
        assert_eq!(t.saved_breaks_for(80.0), Some(&[3][..]));
        t.word_break = figcraft_doc::WordBreak::KeepAll;
        assert_ne!(t.layout_key(), key);
        assert_eq!(t.saved_breaks_for(80.0), None);
        assert!(!has_saved(&t, Some(80.0), Some(40.0)));
    }

    #[test]
    fn style_bindings_and_links_do_not_invalidate_saved_geometry() {
        let mut t = synthetic();
        t.auto_resize = TextAutoResize::None;
        t.runs.push(figcraft_doc::StyleRun { start: 0, end: 1, style: t.style.clone() });
        Arc::make_mut(t.appearance.as_mut().unwrap()).fingerprint = fingerprint(&t, Some(80.0), Some(40.0)).unwrap();
        t.style.style_id = Some("shared-style".into());
        t.style.link = Some("https://example.org/base".into());
        t.runs[0].style.style_id = Some("run-style".into());
        t.runs[0].style.link = Some("https://example.org/run".into());
        assert!(restore(&t, Some(80.0), Some(40.0)).is_some());
        let mut changed = t.clone();
        changed.style.font_size += 1.0;
        assert!(!has_saved(&changed, Some(80.0), Some(40.0)));
        let mut changed = t.clone();
        changed.runs[0].style.fills = Some(vec![]);
        assert!(!has_saved(&changed, Some(80.0), Some(40.0)));
        assert!(!has_saved(&t, Some(160.0), Some(40.0)));
    }

    #[test]
    fn current_layout_box_invalidates_stale_geometry_but_ignores_non_layout_dimensions() {
        for mode in [TextAutoResize::None, TextAutoResize::Height, TextAutoResize::Truncate, TextAutoResize::WidthAndHeight] {
            let mut t = synthetic();
            t.auto_resize = mode;
            Arc::make_mut(t.appearance.as_mut().unwrap()).fingerprint = fingerprint(&t, Some(80.0), Some(40.0)).unwrap();
            assert!(has_saved(&t, Some(80.0), Some(40.0)));
            let width_matters = mode != TextAutoResize::WidthAndHeight;
            assert_eq!(has_saved(&t, Some(160.0), Some(40.0)), !width_matters);
            assert_eq!(restore(&t, Some(160.0), Some(40.0)).is_some(), !width_matters);
            let height_matters = mode == TextAutoResize::Truncate;
            assert_eq!(has_saved(&t, Some(80.0), Some(90.0)), !height_matters);
            assert_eq!(restore(&t, Some(80.0), Some(90.0)).is_some(), !height_matters);
        }
    }

    #[test]
    fn saved_geometry_is_validated_and_never_reused_for_changed_text() {
        let mut t = synthetic();
        assert_eq!(restore(&t, Some(80.0), Some(40.0)).unwrap().to_path().to_svg(), "M0,0 L25,0 L25,40 Z");
        t.characters = "Y".into();
        assert!(restore(&t, Some(80.0), Some(40.0)).is_none());
        let mut t = synthetic();
        Arc::make_mut(t.appearance.as_mut().unwrap()).glyphs[0].char_end = 100;
        assert!(restore(&t, Some(80.0), Some(40.0)).is_none());
        let mut t = synthetic();
        Arc::make_mut(t.appearance.as_mut().unwrap()).glyphs[0].style = 100;
        assert!(restore(&t, Some(80.0), Some(40.0)).is_none());
        let mut t = synthetic();
        Arc::make_mut(t.appearance.as_mut().unwrap()).lines[0].glyph_end = 100;
        assert!(restore(&t, Some(80.0), Some(40.0)).is_none());
    }

    #[test]
    fn restored_links_follow_current_character_spans_not_saved_styles() {
        let mut t = synthetic();
        t.characters = "XY".into();
        t.runs.push(figcraft_doc::StyleRun { start: 1, end: 2, style: t.style.clone() });
        let key = fingerprint(&t, Some(80.0), Some(40.0)).unwrap();
        let a = Arc::make_mut(t.appearance.as_mut().unwrap());
        a.fingerprint = key;
        a.styles[0].link = Some("https://example.org/stale".into());
        let mut second = a.glyphs[0].clone();
        second.char_start = 1;
        second.char_end = 2;
        a.glyphs.push(second);
        a.lines[0].glyph_end = 2;
        a.lines[0].char_end = 2;
        t.runs[0].style.link = Some("https://example.org/current".into());
        let layout = restore(&t, Some(80.0), Some(40.0)).unwrap();
        assert!(layout.styles[layout.glyphs[0].style].link.is_none());
        assert_eq!(layout.styles[layout.glyphs[1].style].link.as_deref(), Some("https://example.org/current"));
        assert!(layout.styles.iter().all(|s| s.link.as_deref() != Some("https://example.org/stale")));
        t.runs[0].style.link = None;
        let layout = restore(&t, Some(80.0), Some(40.0)).unwrap();
        assert!(layout.styles.iter().all(|s| s.link.is_none()));
    }

    #[test]
    fn restored_line_ranges_cannot_multiply_glyph_traversal() {
        let mut t = synthetic();
        let a = Arc::make_mut(t.appearance.as_mut().unwrap());
        a.lines = vec![a.lines[0].clone(); MAX_GLYPHS];
        assert!(restore(&t, Some(80.0), Some(40.0)).is_none());
        let a = Arc::make_mut(t.appearance.as_mut().unwrap());
        a.lines.truncate(2);
        a.lines[1].glyph_start = 1;
        a.lines[1].glyph_end = 1;
        assert!(restore(&t, Some(80.0), Some(40.0)).is_some(), "empty trailing lines are valid");
    }

    #[test]
    fn capturing_an_unavailable_named_font_keeps_existing_portable_guest_geometry() {
        let mut doc = Document::new("Portable guest");
        let page = doc.first_page().unwrap();
        let id = doc.alloc_id();
        let node = figcraft_doc::Node::new(id, NodeKind::Text(synthetic()), "Saved guest", 80.0, 40.0);
        doc.insert(node, page, None).unwrap();
        let before = doc.clone();
        let NodeKind::Text(t) = &doc.node(id).unwrap().kind else { panic!("text") };
        let original = t.appearance.clone().unwrap();
        assert_eq!(capture(&mut doc, None), 0);
        assert_eq!(doc, before);
        let NodeKind::Text(t) = &doc.node(id).unwrap().kind else { panic!("text") };
        assert!(Arc::ptr_eq(&original, t.appearance.as_ref().unwrap()));
    }
}
