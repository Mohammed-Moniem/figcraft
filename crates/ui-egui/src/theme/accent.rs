//! Contrast roles for a user-selected opaque sRGB accent. No document colors or brand assets.
use super::{BRAND, Color32, Tokens, c};

fn ratio(a: Color32, b: Color32) -> f32 {
    c(a).contrast(c(b))
}

fn mix(a: Color32, b: Color32, amount: f32) -> Color32 {
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * amount).round() as u8;
    Color32::from_rgb(channel(a.r(), b.r()), channel(a.g(), b.g()), channel(a.b(), b.b()))
}

fn score(fg: Color32, backgrounds: &[Color32]) -> f32 {
    backgrounds.iter().map(|bg| ratio(fg, *bg)).fold(f32::INFINITY, f32::min)
}

/// Keep the requested hue when it already reads; otherwise move the minimum number of 8-bit
/// steps toward the black/white endpoint that contrasts best with all these adjacent surfaces.
/// A fixed eight-step binary search bounds work even for white and black custom choices.
pub(super) fn readable(seed: Color32, backgrounds: &[Color32], minimum: f32) -> Color32 {
    if score(seed, backgrounds) >= minimum {
        return seed;
    }
    let end = if score(Color32::WHITE, backgrounds) > score(Color32::BLACK, backgrounds) { Color32::WHITE } else { Color32::BLACK };
    let (mut low, mut high) = (0_u16, 255_u16);
    while low < high {
        let mid = (low + high) / 2;
        if score(mix(seed, end, f32::from(mid) / 255.0), backgrounds) >= minimum {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    mix(seed, end, f32::from(high) / 255.0)
}

fn hover(fill: Color32, on: Color32, backgrounds: &[Color32]) -> Color32 {
    for end in
        [if on == Color32::WHITE { Color32::BLACK } else { Color32::WHITE }, if on == Color32::WHITE { Color32::WHITE } else { Color32::BLACK }]
    {
        let candidate = mix(fill, end, 0.12);
        if ratio(on, candidate) >= 4.5 && score(candidate, backgrounds) >= 3.0 {
            return candidate;
        }
    }
    fill
}

pub(super) fn ink(surface: Color32) -> Color32 {
    if ratio(Color32::WHITE, surface) > ratio(Color32::BLACK, surface) { Color32::WHITE } else { Color32::BLACK }
}

/// Tint only as far as the existing row text can read, including the dim tertiary labels.
fn selected(t: Tokens, seed: Color32, amount: f32) -> Color32 {
    let mut amount = amount;
    for _ in 0..9 {
        let row = mix(t.panel, seed, amount);
        if ratio(t.text_secondary, row) >= 4.5 && ratio(t.text_tertiary, row) >= 3.0 {
            return row;
        }
        amount *= 0.5;
    }
    t.panel
}

impl Tokens {
    /// Original green keeps the existing light/dark palettes byte for byte. Other accents have
    /// distinct fill, readable-text, border and white-label menu roles; the saved choice stays raw.
    pub fn with_accent(mut self, seed: Color32) -> Self {
        let seed = Color32::from_rgb(seed.r(), seed.g(), seed.b());
        if seed == BRAND {
            return if self.dark { Self::dark() } else { Self::light() };
        }
        self.accent_base = seed;
        self.accent = readable(seed, &[self.panel, self.field], 3.0);
        self.accent_on = ink(self.accent);
        self.accent_hover = hover(self.accent, self.accent_on, &[self.panel, self.field]);
        self.selected_row = selected(self, self.accent, 0.14);
        self.selected_row_child = selected(self, self.accent, 0.06);
        self.accent_text = readable(
            seed,
            &[
                self.panel,
                self.field,
                self.selected_row,
                self.selected_row_child,
                mix(self.panel, self.accent, 0.3),
                mix(self.field, self.accent, 0.3),
            ],
            4.6,
        );
        self.accent_border = readable(seed, &[self.panel, self.field, self.canvas], 3.0);
        self.focus = self.accent_border;
        self.menu_hover = readable(seed, &[self.menu_text], 4.5);
        self.menu_dim = readable(self.menu_dim, &[self.menu, self.menu_hover], 4.5);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_role_contrast_covers_presets_rgb_extremes_and_selected_text() {
        let mut seeds = vec![Color32::from_rgb(13, 153, 255), Color32::from_rgb(151, 71, 255), Color32::from_rgb(230, 126, 34)];
        for r in [0, 32, 64, 96, 128, 160, 192, 224, 255] {
            for g in [0, 32, 64, 96, 128, 160, 192, 224, 255] {
                for b in [0, 32, 64, 96, 128, 160, 192, 224, 255] {
                    seeds.push(Color32::from_rgb(r, g, b));
                }
            }
        }
        for base in [Tokens::light(), Tokens::dark()] {
            for seed in &seeds {
                let t = base.with_accent(*seed);
                assert_eq!(t.accent_base, *seed);
                for bg in [t.panel, t.field, t.selected_row, t.selected_row_child, mix(t.panel, t.accent, 0.3), mix(t.field, t.accent, 0.3)] {
                    assert!(ratio(t.accent_text, bg) >= 4.5, "text {seed:?} on {bg:?}, dark {}", t.dark);
                }
                for bg in [t.panel, t.field, t.canvas] {
                    assert!(ratio(t.focus, bg) >= 3.0, "focus {seed:?} dark {}", t.dark);
                }
                for bg in [t.accent, t.accent_hover] {
                    assert!(score(bg, &[t.panel, t.field]) >= 3.0);
                    assert!(ratio(t.accent_on, bg) >= 4.5, "button {seed:?} dark {}", t.dark);
                }
                assert!(ratio(t.menu_text, t.menu_hover) >= 4.5);
                assert!(ratio(t.menu_dim, t.menu_hover) >= 4.5);
                assert!(ratio(t.menu_dim, t.menu) >= 4.5);
                for bg in [t.selected_row, t.selected_row_child] {
                    assert!(ratio(t.text_secondary, bg) >= 4.5);
                    assert!(ratio(t.text_tertiary, bg) >= 3.0);
                }
                assert_eq!(
                    (t.component, t.component_row, t.danger, t.success, t.warning, t.guide),
                    (base.component, base.component_row, base.danger, base.success, base.warning, base.guide)
                );
                assert_eq!(t.with_accent(BRAND), base);
            }
        }
    }

    #[test]
    fn custom_canvas_outlines_contrast_with_each_reported_page_background() {
        let ctx = egui::Context::default();
        for seed in [
            Color32::BLACK,
            Color32::WHITE,
            Color32::GRAY,
            Color32::RED,
            Color32::from_rgb(13, 153, 255),
            Color32::from_rgb(151, 71, 255),
            Color32::from_rgb(230, 126, 34),
        ] {
            super::super::apply(&ctx, Tokens::light().with_accent(seed));
            for page in [
                Color32::BLACK,
                Color32::WHITE,
                Color32::GRAY,
                Color32::RED,
                Color32::GREEN,
                Color32::BLUE,
                Color32::from_gray(30),
                Color32::from_gray(245),
            ] {
                super::super::set_canvas_page(&ctx, page);
                let outline = super::super::canvas_accent(&ctx);
                assert!(ratio(outline, page) >= 3.0, "{seed:?} outline {outline:?} on {page:?}");
                let label = super::super::under_white(outline);
                assert!(ratio(label, Color32::WHITE) >= 4.5);
            }
        }
    }
    #[test]
    fn custom_menu_shortcuts_stay_readable_on_idle_and_highlighted_rows() {
        for base in [Tokens::light(), Tokens::dark()] {
            for seed in [
                Color32::BLACK,
                Color32::WHITE,
                Color32::GRAY,
                Color32::from_rgb(13, 153, 255),
                Color32::from_rgb(151, 71, 255),
                Color32::from_rgb(230, 126, 34),
            ] {
                let t = base.with_accent(seed);
                for bg in [t.menu, t.menu_hover] {
                    assert!(ratio(t.menu_dim, bg) >= 4.5, "shortcut text {seed:?} dark {}: {:.3}:1", t.dark, ratio(t.menu_dim, bg));
                }
            }
        }
    }
}
