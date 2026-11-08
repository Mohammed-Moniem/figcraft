//! The dimensional parts of shared style values, using the editor's K-scale semantics.

use figcraft_color::{ImageScaleMode, Paint, PaintKind};
use figcraft_doc::{Effect, EffectKind, LayoutGrid, LetterSpacing, LineHeight, StyleKind, TextStyle};

fn number(value: &mut f64, factor: f64) -> bool {
    *value *= factor;
    value.is_finite()
}

pub(crate) fn paints(paints: &mut [Paint], factor: f64) -> bool {
    let mut finite = true;
    for paint in paints {
        if let PaintKind::Image(image) = &mut paint.kind
            && image.scale_mode == ImageScaleMode::Tile
        {
            finite &= number(&mut image.scale, factor);
        }
    }
    finite
}

pub(crate) fn grids(grids: &mut [LayoutGrid], factor: f64) -> bool {
    let mut finite = true;
    for grid in grids {
        for value in [&mut grid.size, &mut grid.gutter, &mut grid.offset] {
            finite &= number(value, factor);
        }
    }
    finite
}

pub(crate) fn effects(effects: &mut [Effect], factor: f64) -> bool {
    let mut finite = true;
    for effect in effects {
        match &mut effect.kind {
            EffectKind::DropShadow(shadow) | EffectKind::InnerShadow(shadow) => {
                for value in [&mut shadow.radius, &mut shadow.spread, &mut shadow.offset.0, &mut shadow.offset.1] {
                    finite &= number(value, factor);
                }
            }
            EffectKind::LayerBlur(blur) | EffectKind::BackgroundBlur(blur) => {
                *blur = blur.scaled(factor);
                finite &= blur.radius.is_finite() && blur.start_radius.is_finite();
            }
            EffectKind::Noise(noise) => {
                finite &= number(&mut noise.size, factor);
                if let Some(y) = &mut noise.size_y {
                    finite &= number(y, factor);
                }
            }
            EffectKind::Texture(texture) => {
                finite &= number(&mut texture.size, factor);
                if let Some(y) = &mut texture.size_y {
                    finite &= number(y, factor);
                }
                finite &= number(&mut texture.radius, factor);
            }
            EffectKind::Glass(glass) => {
                finite &= number(&mut glass.depth, factor);
                // The same sanitized frost and saturation as physical K-scale.
                glass.frost = (glass.sanitized().frost * factor).min(figcraft_doc::Glass::MAX_FROST);
                finite &= glass.frost.is_finite();
            }
        }
    }
    finite
}

pub(crate) fn text(style: &mut TextStyle, factor: f64) -> bool {
    let mut finite = number(&mut style.font_size, factor);
    if let LineHeight::Pixels(value) = &mut style.line_height {
        finite &= number(value, factor);
    }
    if let LetterSpacing::Pixels(value) = &mut style.letter_spacing {
        finite &= number(value, factor);
    }
    style.set_look(|look| {
        for measure in [&mut look.thickness, &mut look.offset] {
            if let LineHeight::Pixels(value) = measure {
                finite &= number(value, factor);
            }
        }
    });
    finite
}

pub(super) fn kind(kind: &StyleKind, factor: f64) -> Option<StyleKind> {
    if !factor.is_finite() || factor <= 0.0 {
        return None;
    }
    let mut next = kind.clone();
    if factor == 1.0 {
        return Some(next);
    }
    let finite = match &mut next {
        StyleKind::Paint { paints: list } => paints(list, factor),
        StyleKind::Text { style } => text(style, factor),
        StyleKind::Effect { effects: list } => effects(list, factor),
        StyleKind::Grid { grids: list } => grids(list, factor),
    };
    finite.then_some(next)
}

/// Inversion must preserve every stored value, allowing only floating-point round-off. A
/// saturated/nonrepresentable capture is refused instead of changing the layer's appearance.
pub(super) fn equivalent(a: &StyleKind, b: &StyleKind) -> bool {
    fn near(a: &serde_json::Value, b: &serde_json::Value, depth: usize) -> bool {
        use serde_json::Value;
        if depth > 64 {
            return false;
        }
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
                (Some(a), Some(b)) => a == b || (a - b).abs() <= a.abs().max(b.abs()) * f64::EPSILON * 16.0,
                _ => a == b,
            },
            (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| near(a, b, depth + 1)),
            (Value::Object(a), Value::Object(b)) => a.len() == b.len() && a.iter().all(|(key, a)| b.get(key).is_some_and(|b| near(a, b, depth + 1))),
            _ => a == b,
        }
    }
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(a), Ok(b)) => near(&a, &b, 0),
        _ => false,
    }
}
