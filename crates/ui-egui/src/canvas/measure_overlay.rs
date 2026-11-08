//! Active alignment lines follow the page-adaptive accent. Measurements keep their red lines,
//! dashed extensions, point markers, measured outline and distance labels; labels never overlap
//! one another and stay the same size at every zoom.

use egui::{Color32, CornerRadius, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};

use crate::FigApp;
use crate::theme::{Tokens, canvas_accent, semibold};

/// Space between a label and the line it labels (or another label), in screen pixels.
const GAP: f32 = 4.0;
/// Padding inside a label's pill.
const PAD: Vec2 = vec2(4.0, 2.0);

/// Draw the tool overlay's guides, measurement labels and measured box, and in Dev Mode the
/// measurements kept on the canvas; labels keep clear of `avoid` (the selection's size label) and
/// of one another.
pub fn draw(app: &FigApp, painter: &egui::Painter, t: &Tokens, avoid: Option<Rect>) {
    let ov = &app.session.tool.overlay;
    let line = Stroke::new(1.0, t.guide);
    let alignment = Stroke::new(1.0, canvas_accent(painter.ctx()));
    if let Some([x0, y0, x1, y1]) = ov.measure_box {
        painter.rect_stroke(Rect::from_two_pos(app.to_screen(x0, y0), app.to_screen(x1, y1)), CornerRadius::ZERO, line, StrokeKind::Middle);
    }
    let mut labels: Vec<(Pos2, Pos2, String)> = Vec::new();
    for g in ov.guides.iter().chain(crate::dev_mode::saved_lines(app)) {
        let (a, b) = (app.to_screen(g.x0, g.y0), app.to_screen(g.x1, g.y1));
        if g.x0 == g.x1 && g.y0 == g.y1 {
            for d in [vec2(4.0, 4.0), vec2(4.0, -4.0)] {
                painter.line_segment([a - d, a + d], Stroke::new(1.5, t.guide));
            }
        } else if g.dashed {
            painter.extend(Shape::dashed_line(&[a, b], line, 3.0, 2.0));
        } else {
            // Labelled lines are distances; a plain line is active alignment feedback,
            // including matches to object, layout-grid and ruler-guide snap targets.
            painter.line_segment([a, b], if g.label.is_none() { alignment } else { line });
        }
        if let Some(l) = &g.label {
            labels.push((a, b, l.clone()));
        }
    }
    let galleys: Vec<_> = labels.iter().map(|(_, _, l)| painter.layout_no_wrap(l.clone(), semibold(10.0), Color32::WHITE)).collect();
    let lines: Vec<(Pos2, Pos2, Vec2)> = labels.iter().zip(&galleys).map(|((a, b, _), g)| (*a, *b, g.size() + PAD * 2.0)).collect();
    for (r, g) in place_labels(&lines, avoid.as_slice()).into_iter().zip(galleys) {
        painter.rect_filled(r, CornerRadius::same(3), t.guide);
        painter.galley(r.min + PAD, g, Color32::WHITE);
    }
}

/// Label boxes for lines `(a, b, label size)` in screen space: centred on the line when it is long
/// enough to show around the label, else (or when that spot is taken) slid along it, then beside it
/// (below a horizontal line, right of a vertical one, then the other side, further out each time)
/// until it clears `avoid` and every label placed before it.
pub fn place_labels(lines: &[(Pos2, Pos2, Vec2)], avoid: &[Rect]) -> Vec<Rect> {
    let mut placed: Vec<Rect> = avoid.to_vec();
    for (a, b, size) in lines {
        let mid = pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let d = *b - *a;
        let horizontal = d.x.abs() >= d.y.abs();
        // Along the line and out from it: the unit direction and one label's step.
        let (dir, out) = if horizontal { (vec2(1.0, 0.0), vec2(0.0, 1.0)) } else { (vec2(0.0, 1.0), vec2(1.0, 0.0)) };
        let (along, across) = if horizontal { (size.x, size.y) } else { (size.y, size.x) };
        let room = (d.length() - along) / 2.0 - GAP;
        let mut centres: Vec<Pos2> = Vec::new();
        if room >= 0.0 {
            centres.push(mid);
            // Slid along the line, both ways, while it still lies on the line.
            let mut n = 1.0;
            while n * (along + GAP) <= room && centres.len() < 32 {
                centres.extend([mid + dir * n * (along + GAP), mid - dir * n * (along + GAP)]);
                n += 1.0;
            }
        }
        for n in 0..32 {
            let off = across / 2.0 + GAP + n as f32 * (across + GAP);
            centres.extend([mid + out * off, mid - out * off]);
        }
        let free = |r: &Rect| !placed.iter().any(|p| p.expand(GAP / 2.0).intersects(*r));
        // Past every candidate (dozens of labels on one spot): stack below everything placed.
        let r = centres.into_iter().map(|c| Rect::from_center_size(c, *size)).find(free).unwrap_or_else(|| {
            let bottom = placed.iter().map(|p| p.max.y).fold(mid.y, f32::max);
            Rect::from_min_size(pos2(mid.x - size.x / 2.0, bottom + GAP), *size)
        });
        placed.push(r);
    }
    placed.split_off(avoid.len())
}

#[cfg(test)]
mod tests {
    use super::place_labels;
    use egui::{Pos2, Rect, pos2, vec2};

    const SIZE: egui::Vec2 = vec2(24.0, 14.0);

    fn no_overlaps(r: &[Rect]) -> bool {
        r.iter().enumerate().all(|(i, a)| r.iter().skip(i + 1).all(|b| !a.intersects(*b)))
    }

    #[test]
    fn long_lines_carry_their_label_in_the_middle() {
        let r = place_labels(&[(pos2(0.0, 50.0), pos2(200.0, 50.0), SIZE), (pos2(50.0, 0.0), pos2(50.0, 300.0), SIZE)], &[]);
        assert_eq!(r[0].center(), pos2(100.0, 50.0));
        assert_eq!(r[1].center(), pos2(50.0, 150.0));
    }

    #[test]
    fn short_lines_put_the_label_beside_them() {
        let r = place_labels(&[(pos2(0.0, 50.0), pos2(10.0, 50.0), SIZE), (pos2(50.0, 0.0), pos2(50.0, 6.0), SIZE)], &[]);
        assert!(r[0].min.y > 50.0, "below a short horizontal line: {:?}", r[0]);
        assert!(r[1].min.x > 50.0, "right of a short vertical line: {:?}", r[1]);
    }

    #[test]
    fn labels_never_overlap_even_when_crowded() {
        // Zoomed far out: every line collapses onto one spot.
        let crowd: Vec<(Pos2, Pos2, egui::Vec2)> =
            (0..100).map(|i| (pos2(10.0, 10.0), pos2(10.0 + (i % 2) as f32 * 0.5, 10.0 + (i % 3) as f32 * 0.5), SIZE)).collect();
        assert!(no_overlaps(&place_labels(&crowd, &[])));
        // Parallel lines a few pixels apart.
        let close: Vec<(Pos2, Pos2, egui::Vec2)> = (0..6).map(|i| (pos2(0.0, i as f32 * 3.0), pos2(100.0, i as f32 * 3.0), SIZE)).collect();
        let r = place_labels(&close, &[]);
        assert!(no_overlaps(&r));
        assert_eq!(r[0].center(), pos2(50.0, 0.0), "the first keeps its place");
    }

    #[test]
    fn labels_keep_clear_of_the_size_label() {
        let size = Rect::from_center_size(pos2(50.0, 100.0), vec2(60.0, 16.0));
        // A long line slides its label along itself, past the size label.
        let r = place_labels(&[(pos2(50.0, 80.0), pos2(50.0, 200.0), SIZE)], &[size]);
        assert_eq!(r.len(), 1);
        assert!(!r[0].intersects(size) && r[0].center().x == 50.0, "{:?}", r[0]);
        // A short one moves beside it.
        let r = place_labels(&[(pos2(50.0, 90.0), pos2(50.0, 110.0), SIZE)], &[size]);
        assert!(!r[0].intersects(size), "{:?}", r[0]);
    }

    use egui::{Color32, Event, Modifiers, RawInput, Shape};
    use figcraft_doc::NodeId;
    use figcraft_engine::{Session, snap::Guide};
    use serde_json::json;

    use crate::layers_drag_tests::Ui;
    use crate::live_tests::flat;
    use crate::theme::{self, Tokens};

    fn smart_guide_scene(dark: bool, accent: &str, page: &str, zoom: f64) -> (Ui, NodeId, NodeId) {
        let mut s = Session::new();
        s.prefs = serde_json::from_value(json!({"theme":if dark {"dark"} else {"light"},"accent":accent})).unwrap();
        s.execute("page.setBackground", &json!({"color":page})).unwrap();
        let fixed = s.execute("shape.rectangle", &json!({"x":100,"y":100,"width":140,"height":100})).unwrap();
        let moved = s.execute("shape.rectangle", &json!({"x":350,"y":350,"width":80,"height":50})).unwrap();
        let fixed = figcraft_engine::params::id_of(&fixed["id"]).unwrap();
        let moved = figcraft_engine::params::id_of(&moved["id"]).unwrap();
        s.execute("select.set", &json!({"ids":[moved]})).unwrap();
        s.execute("view.snapObjects", &json!({"on":true})).unwrap();
        s.execute("view.snapPixel", &json!({"on":true})).unwrap();
        let mut u = Ui::new(s);
        let view = &mut u.app.session.ds_mut().unwrap().view;
        view.cx = 260.0;
        view.cy = 260.0;
        view.zoom = zoom;
        u.frame(vec![]);
        (u, fixed, moved)
    }

    fn guide_strokes(u: &Ui, out: &egui::FullOutput, guide: &Guide) -> Vec<(Color32, f32)> {
        let a = u.app.to_screen(guide.x0, guide.y0);
        let b = u.app.to_screen(guide.x1, guide.y1);
        flat(&out.shapes)
            .iter()
            .filter_map(|shape| match shape {
                Shape::LineSegment { points, stroke } if *points == [a, b] || *points == [b, a] => Some((stroke.color, stroke.width)),
                _ => None,
            })
            .collect()
    }

    fn drag_to_center_and_check(dark: bool, accent: &str, page: &str, zoom: f64, vertical: bool) {
        let (mut u, fixed, moved) = smart_guide_scene(dark, accent, page, zoom);
        let before = u.app.session.doc().unwrap().clone();
        let history = u.app.session.undo_depth();
        let fixed_center = before.world_bounds(fixed).unwrap().center();
        let moving_center = before.world_bounds(moved).unwrap().center();
        let start = u.app.to_screen(moving_center.x, moving_center.y);
        let target = if vertical {
            u.app.to_screen(fixed_center.x + 0.5 / zoom, moving_center.y)
        } else {
            u.app.to_screen(moving_center.x, fixed_center.y + 0.5 / zoom)
        };
        assert!(u.app.canvas.rect.contains(start) && u.app.canvas.rect.contains(target), "raw pointer fixture stays inside canvas");
        u.drag_to(start, target);
        let out = u.frame_output(vec![]);
        let center = u.app.session.doc().unwrap().world_bounds(moved).unwrap().center();
        assert!(
            if vertical { (center.x - fixed_center.x).abs() < 1e-6 } else { (center.y - fixed_center.y).abs() < 1e-6 },
            "actual drag snaps centers before its color is checked"
        );
        let guide = u
            .app
            .session
            .tool
            .overlay
            .guides
            .iter()
            .find(|guide| {
                guide.label.is_none()
                    && !guide.dashed
                    && if vertical {
                        (guide.x0 - fixed_center.x).abs() < 1e-6 && guide.x0 == guide.x1 && guide.y0 != guide.y1
                    } else {
                        (guide.y0 - fixed_center.y).abs() < 1e-6 && guide.y0 == guide.y1 && guide.x0 != guide.x1
                    }
            })
            .expect("actual engine center-alignment guide");
        let strokes = guide_strokes(&u, &out, guide);
        assert!(!strokes.is_empty(), "the presented frame paints the exact engine guide endpoints");
        let expected = theme::canvas_accent(&u.ctx);
        assert_ne!(expected, Tokens::get(&u.ctx).guide, "fixture distinguishes accent and measurement roles");
        assert!(
            strokes.iter().all(|stroke| *stroke == (expected, 1.0)),
            "center guide must use canvas accent: dark={dark}, accent={accent}, page={page}, zoom={zoom}, vertical={vertical}, actual={strokes:?}, expected={expected:?}"
        );
        u.button(target, false);
        assert_eq!(u.app.session.undo_depth().0, history.0 + 1, "color changes add no extra edit");
        u.app.session.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(u.app.session.doc().unwrap(), &before, "one Undo restores the exact original document");
    }

    #[test]
    fn smart_guide_accent_default_center_alignment_uses_the_page_adaptive_accent() {
        for dark in [false, true] {
            for page in ["#F5F5F5", "#1E1E1E"] {
                for zoom in [1.0, 2.0] {
                    for vertical in [false, true] {
                        drag_to_center_and_check(dark, "#02DA96", page, zoom, vertical);
                    }
                }
            }
        }
    }

    #[test]
    fn smart_guide_accent_custom_center_alignment_keeps_its_current_ui_role() {
        for dark in [false, true] {
            for accent in ["#0D99FF", "#9747FF"] {
                for page in ["#F5F5F5", "#101820"] {
                    for vertical in [false, true] {
                        drag_to_center_and_check(dark, accent, page, 1.0, vertical);
                    }
                }
            }
        }
    }

    #[test]
    fn smart_guide_accent_alt_hover_measurements_keep_red_lines_extensions_and_labels() {
        for dark in [false, true] {
            let (mut u, fixed, _) = smart_guide_scene(dark, "#0D99FF", "#F5F5F5", 1.0);
            let center = u.app.session.doc().unwrap().world_bounds(fixed).unwrap().center();
            let at = u.app.to_screen(center.x, center.y);
            let out = u.frame_with(vec![Event::PointerMoved(at)], Modifiers::ALT);
            let guides = &u.app.session.tool.overlay.guides;
            assert!(guides.iter().any(|guide| guide.dashed), "actual diagonal Alt measurement includes extension lines");
            let labelled: Vec<_> = guides.iter().filter(|guide| guide.label.is_some()).collect();
            assert!(!labelled.is_empty(), "actual Alt hover measures between original rectangles");
            let red = Color32::from_rgb(242, 72, 34);
            assert_eq!(Tokens::get(&u.ctx).guide, red);
            for guide in labelled {
                let strokes = guide_strokes(&u, &out, guide);
                assert!(!strokes.is_empty());
                assert!(strokes.iter().all(|stroke| *stroke == (red, 1.0)), "distance line keeps measurement role");
            }
            assert!(
                flat(&out.shapes).iter().any(|shape| matches!(shape,Shape::Rect(rect) if rect.fill==red)),
                "distance label pill keeps measurement role"
            );
        }
    }

    #[test]
    fn smart_guide_accent_point_markers_keep_their_existing_measurement_color() {
        let (mut u, _, _) = smart_guide_scene(false, "#0D99FF", "#F5F5F5", 1.0);
        let marker = figcraft_geom::Point::new(200.0, 200.0);
        u.app.session.tool.overlay.guides = vec![Guide::new(marker, marker)];
        let at = u.app.to_screen(marker.x, marker.y);
        let app = &u.app;
        let ctx = u.ctx.clone();
        let out = ctx.run_ui(RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))), ..Default::default() }, |ui| {
            super::draw(app, ui.painter(), &Tokens::get(ui.ctx()), None);
        });
        let marker_strokes: Vec<_> = flat(&out.shapes)
            .into_iter()
            .filter_map(|shape| match shape {
                Shape::LineSegment { points, stroke } if points[0].distance(at) < 6.0 && points[1].distance(at) < 6.0 => Some(stroke),
                _ => None,
            })
            .collect();
        assert_eq!(marker_strokes.len(), 2);
        assert!(marker_strokes.iter().all(|stroke| stroke.color == Color32::from_rgb(242, 72, 34) && stroke.width == 1.5));
    }
}
