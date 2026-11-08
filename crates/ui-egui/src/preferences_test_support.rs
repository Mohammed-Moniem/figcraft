//! Drive the scrollable Preferences form through visible, reachable controls.

use egui::accesskit::Role;
use egui::{Context, Event, FullOutput, Pos2, Rect, Shape, pos2, vec2};

fn bounds(node: &egui::accesskit::Node) -> Rect {
    let b = node.bounds().expect("the Preferences control has bounds");
    Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32))
}

fn painted(out: &FullOutput, text: &str, screen: Rect) -> bool {
    fn contains(shape: &Shape, clip: Rect, text: &str, screen: Rect) -> bool {
        match shape {
            Shape::Text(t) if t.galley.text() == text => {
                let rect = Rect::from_min_size(t.pos, t.galley.size());
                clip.contains_rect(rect) && screen.contains_rect(rect)
            }
            Shape::Vec(shapes) => shapes.iter().any(|s| contains(s, clip, text, screen)),
            _ => false,
        }
    }
    out.shapes.iter().any(|s| contains(&s.shape, s.clip_rect, text, screen))
}

/// Scroll using actual wheel input until the named control's full hit bounds are in the
/// dialog body. Static labels must also have an unclipped painted galley. Merely finding a
/// node or text outside the scroll viewport must never authorize a test click.
pub(crate) fn show(ctx: &Context, mut frame: impl FnMut(Vec<Event>) -> FullOutput, label: &str, role: Role) -> Rect {
    ctx.enable_accesskit();
    frame(vec![]);
    let mut last = None;
    for _ in 0..24 {
        let out = frame(vec![]);
        let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
        let window = nodes.iter().map(|(_, n)| n).find(|n| n.role() == Role::Window && n.label() == Some("Preferences")).unwrap();
        let window = bounds(window);
        // floating() gives the body a 20-point horizontal margin; left-aligned buttons
        // begin there. A larger inset can never be repaired by vertical scrolling.
        let visible = window.intersect(ctx.content_rect()).shrink2(vec2(20.0, 24.0));
        let node = nodes.iter().map(|(_, n)| n).find(|n| n.role() == role && (n.label() == Some(label) || n.value() == Some(label)));
        let target = node.map(bounds);
        if let Some(target) = target {
            let below_title = target.top() >= window.top() + 80.0;
            let static_text = matches!(role, Role::Label | Role::TextRun);
            if below_title && visible.contains_rect(target) && (!static_text || painted(&out, label, ctx.content_rect())) {
                return target;
            }
        }
        last = Some((target, visible));
        let delta = if target.is_none_or(|r| r.center().y > visible.center().y) { -140.0 } else { 140.0 };
        frame(vec![
            Event::PointerMoved(visible.center()),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                source: egui::MouseWheelSource::Unknown,
                delta: vec2(0.0, delta),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        for _ in 0..4 {
            frame(vec![]);
        }
    }
    panic!("Preferences {label:?} ({role:?}) did not become reachable through scrolling: {last:?}");
}

/// A reset button on the same row as the previously revealed field (several rows have Reset).
pub(crate) fn row_button(out: &FullOutput, field_label: &str, label: &str) -> Pos2 {
    let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
    let field = nodes.iter().map(|(_, n)| n).find(|n| n.role() == Role::TextInput && n.label() == Some(field_label)).unwrap();
    let field = bounds(field);
    let button = nodes
        .iter()
        .map(|(_, n)| n)
        .find(|n| n.role() == Role::Button && n.label() == Some(label) && (bounds(n).center().y - field.center().y).abs() < 4.0)
        .unwrap_or_else(|| {
            let buttons: Vec<_> = nodes.iter().map(|(_, n)| n).filter(|n| n.role() == Role::Button && n.label() == Some(label)).map(bounds).collect();
            panic!("the intended row's button is present: field {field:?}, buttons {buttons:?}");
        });
    assert!(!button.is_disabled(), "the intended row's button is enabled");
    bounds(button).center()
}
