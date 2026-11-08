//! Direct text-size dragging, separate from resizing its container or the Scale tool.
use super::{Gesture, PointerEvent, Result, Session, ToolId, locked_in};
use figcraft_doc::{Document, LineHeight, NodeId, NodeKind, TextData, TextStyle};
use figcraft_geom::{Affine, Point};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct TextFontHandle {
    pub id: NodeId,
    pub at: [f64; 2],
    pub angle: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct FontSizeStart {
    id: NodeId,
    pub(super) point: Point,
    inverse: Affine,
    start_y: f64,
    height: f64,
    min: f64,
    max: f64,
    pub(super) angle: f64,
}

fn styles(t: &TextData) -> impl Iterator<Item = &TextStyle> {
    std::iter::once(&t.style).chain(t.runs.iter().map(|r| &r.style))
}
fn font_bounds(t: &TextData) -> Option<(f64, f64)> {
    let (mut min, mut max) = (5000.0_f64, 1.0_f64);
    for s in styles(t) {
        if !s.font_size.is_finite() || !(1.0..=5000.0).contains(&s.font_size) {
            return None;
        }
        min = min.min(s.font_size);
        max = max.max(s.font_size);
        match s.line_height {
            LineHeight::Pixels(v) | LineHeight::Percent(v) if !v.is_finite() => return None,
            _ => {}
        }
    }
    Some((min, max))
}
fn target(doc: &Document, sel: &[NodeId], zoom: f64) -> Option<(TextFontHandle, Affine, f64, f64, f64)> {
    let [id] = sel else { return None };
    let n = doc.node(*id)?;
    let NodeKind::Text(t) = &n.kind else { return None };
    if !zoom.is_finite()
        || zoom <= 0.0
        || !n.visible
        || doc.ancestors(*id).into_iter().any(|id| doc.node(id).is_some_and(|n| !n.visible))
        || locked_in(doc, *id)
        || t.path.is_some()
        || !n.width.is_finite()
        || !n.height.is_finite()
        || n.width <= 0.0
        || n.height <= 0.0
    {
        return None;
    }
    let world = doc.world_transform(*id);
    if !world.as_coeffs().iter().all(|v| v.is_finite()) || !world.determinant().is_finite() || world.determinant().abs() < 1e-12 {
        return None;
    }
    let origin = world * Point::ZERO;
    let down = world * Point::new(0.0, 1.0) - origin;
    // Keep the circle's reach clear of both bottom corner handles on small text boxes.
    if (world * Point::new(n.width, 0.0) - origin).hypot() * zoom < 32.0 || down.hypot() * n.height * zoom < 8.0 {
        return None;
    }
    let (min, max) = font_bounds(t)?;
    let p = world * Point::new(n.width / 2.0, n.height);
    if !p.x.is_finite() || !p.y.is_finite() {
        return None;
    }
    Some((TextFontHandle { id: *id, at: [p.x, p.y], angle: down.atan2().to_degrees() }, world.inverse(), n.height, min, max))
}

impl Session {
    fn text_font_shown(&self) -> bool {
        self.tool.tool == ToolId::Move
            && !self.tool.dev_mode
            && self.active_read_only().is_none()
            && (!self.tool.busy() || matches!(self.tool.gesture, Gesture::TextFontSize { .. }))
            && self.ds().is_ok_and(|ds| ds.text_edit.is_none() && ds.vector_edit.is_none())
    }
    /// The original circular font-size handle. Text-on-path and undersized/singular boxes have none.
    pub fn text_font_handle(&self) -> Option<TextFontHandle> {
        if !self.text_font_shown() {
            return None;
        }
        let ds = self.ds().ok()?;
        target(&ds.doc, &ds.selection, ds.view.zoom).map(|t| t.0)
    }
    pub(super) fn text_font_start(&self, ev: &PointerEvent) -> Option<FontSizeStart> {
        if ev.shift || ev.alt || ev.cmd || ev.ctrl || !self.text_font_shown() {
            return None;
        }
        let ds = self.ds().ok()?;
        let (h, inverse, height, min, max) = target(&ds.doc, &ds.selection, ev.zoom)?;
        if (Point::new(h.at[0], h.at[1]) - ev.p()).hypot() * ev.zoom > 6.0 {
            return None;
        }
        let start_y = (inverse * ev.p()).y;
        start_y.is_finite().then_some(FontSizeStart { id: h.id, point: ev.p(), inverse, start_y, height, min, max, angle: h.angle })
    }
    pub(super) fn text_font_drag(&mut self, ev: &PointerEvent, start: &FontSizeStart) -> Result<()> {
        let factor = 1.0 + ((start.inverse * ev.p()).y - start.start_y) / start.height;
        if !factor.is_finite() {
            return Ok(());
        }
        let factor = factor.clamp(1.0 / start.min, 5000.0 / start.max);
        self.preview(|doc, _| {
            let NodeKind::Text(t) = &mut doc.get_mut(start.id)?.kind else { return Ok(()) };
            // A hostile saved pixel value must not overflow when its font is enlarged.
            if styles(t).any(|s| matches!(s.line_height, LineHeight::Pixels(v) if !(v * factor).is_finite())) {
                return Ok(());
            }
            let scale = |s: &mut TextStyle| {
                s.font_size *= factor;
                if let LineHeight::Pixels(v) = &mut s.line_height {
                    *v *= factor;
                }
            };
            scale(&mut t.style);
            for run in &mut t.runs {
                scale(&mut run.style);
            }
            Ok(())
        })?;
        self.tool.overlay.text_font_scale = Some(factor);
        self.tool.overlay.cursor =
            if self.refused(&self.selection(), false) { super::Cursor::NotAllowed } else { super::Cursor::Resize(start.angle) };
        Ok(())
    }
}
