//! Saved accent choices propagate through real egui frames without recoloring document paints.
use crate::{
    FigApp, NoServices,
    present_tests::Ui,
    theme::{self, Tokens},
};
use egui::{Color32, Context, Event, Pos2, RawInput, Rect, Shape, pos2, vec2};
use figcraft_engine::{Prefs, Session};
use serde_json::json;

fn custom_session(dark: bool, color: &str) -> Session {
    let mut s = Session::new();
    s.prefs = serde_json::from_value(json!({"theme":if dark {"dark"} else {"light"},"accent":color})).unwrap();
    s
}

fn color(c: Color32) -> figcraft_color::Color {
    figcraft_color::Color::new(f32::from(c.r()) / 255.0, f32::from(c.g()) / 255.0, f32::from(c.b()) / 255.0, 1.0)
}

fn visit(shape: &Shape, f: &mut impl FnMut(&Shape)) {
    f(shape);
    if let Shape::Vec(shapes) = shape {
        for s in shapes {
            visit(s, f);
        }
    }
}

fn has_stroke(out: &egui::FullOutput, expected: Color32) -> bool {
    let mut found = false;
    for s in &out.shapes {
        visit(&s.shape, &mut |s| {
            if let Shape::Rect(r) = s {
                found |= r.stroke.color == expected && r.stroke.width > 0.0;
            }
            if let Shape::Path(p) = s {
                found |= matches!(p.stroke.color, egui::epaint::ColorMode::Solid(c) if c==expected) && p.stroke.width > 0.0;
            }
        });
    }
    found
}

#[test]
fn saved_accent_is_applied_on_first_frame_and_dark_popovers_keep_it() {
    for dark in [false, true] {
        let ctx = Context::default();
        let mut app = FigApp::new(custom_session(dark, "#0D99FF"), Box::new(NoServices));
        let _ = frame_ui(&ctx, RawInput::default(), |ui| app.ui(ui));
        let t = Tokens::get(&ctx);
        assert_eq!(t.accent_base, Color32::from_rgb(13, 153, 255));
        assert_eq!(ctx.global_style().visuals.hyperlink_color, t.accent_text);
        let mut inside = None;
        let _ = frame_ui(&ctx, RawInput::default(), |ui| theme::on_dark(ui, |ui| inside = Some(Tokens::get(ui.ctx()))));
        assert_eq!(inside.unwrap().accent_base, t.accent_base, "a dark style editor keeps the custom choice");
        assert_eq!(Tokens::get(&ctx), t, "scoped dark styling restores the outer theme");
        app.session.execute(if dark { "view.themeLight" } else { "view.themeDark" }, &json!({})).unwrap();
        let _ = frame_ui(&ctx, RawInput::default(), |ui| app.ui(ui));
        let switched = Tokens::get(&ctx);
        assert_eq!(switched.dark, !dark);
        assert_eq!(switched.accent_base, t.accent_base);
        let mut restored = Session::new();
        restored.prefs = serde_json::from_value(serde_json::to_value(&app.session.prefs).unwrap()).unwrap();
        let restored_ctx = Context::default();
        let mut restored_app = FigApp::new(restored, Box::new(NoServices));
        let _ = frame_ui(&restored_ctx, RawInput::default(), |ui| restored_app.ui(ui));
        assert_eq!(Tokens::get(&restored_ctx), switched, "saved accent and theme survive a fresh frontend");
    }
}

#[test]
fn custom_canvas_outlines_and_focused_fields_are_painted_with_the_current_accent() {
    for dark in [false, true] {
        let mut u = Ui::new(custom_session(dark, "#0D99FF"));
        u.run("shape.rectangle", json!({"x":0,"y":0,"width":100,"height":70}));
        let doc = u.app.session.ds().unwrap().doc.clone();
        let out = u.frame_output(vec![]);
        let accent = theme::canvas_accent(&u.ctx);
        assert_ne!(accent, theme::ACCENT);
        assert_ne!(accent, theme::BRAND);
        assert!(has_stroke(&out, accent), "the selected rectangle paints the custom canvas outline");
        let selection = u.app.session.selection().to_vec();
        u.run("pref.accentColor", json!({"preset":"orange"}));
        let changed = theme::canvas_accent(&u.ctx);
        assert_ne!(changed, accent);
        assert!(has_stroke(&u.frame_output(vec![]), changed), "cached selection chrome updates without reselecting");
        assert_eq!(u.app.session.selection(), selection);
        let field = egui::Id::unique("accent-fixture-field");
        let mut time = 0.0;
        let mut frame = |events| {
            time += 0.1;
            frame_ui(
                &u.ctx,
                RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), time: Some(time), events, ..Default::default() },
                |ui| {
                    crate::widgets::text_field_named(ui, field, "Field", 160.0, "Accent test field");
                },
            )
        };
        frame(vec![]);
        for pressed in [true, false] {
            frame(vec![
                Event::PointerMoved(pos2(16.0, 16.0)),
                Event::PointerButton { pos: pos2(16.0, 16.0), button: egui::PointerButton::Primary, pressed, modifiers: Default::default() },
            ]);
        }
        let out = frame(vec![]);
        let t = Tokens::get(&u.ctx);
        assert!(has_stroke(&out, t.focus), "focused text field uses the accent focus ring");
        assert!(std::sync::Arc::ptr_eq(&doc, &u.app.session.ds().unwrap().doc));
    }
}

#[test]
fn hovered_popup_rows_use_the_custom_accent_including_the_light_theme() {
    for dark in [false, true] {
        let u = Ui::new(custom_session(dark, "#E67E22"));
        let t = Tokens::get(&u.ctx);
        assert_ne!(t.menu_hover, theme::ACCENT);
        let mut last = None;
        for k in 1..=4 {
            last = Some(frame_ui(
                &u.ctx,
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                    time: Some(1.0 + f64::from(k) * 0.1),
                    events: vec![Event::PointerMoved(pos2(16.0, 16.0))],
                    ..Default::default()
                },
                |ui| {
                    crate::right::menu_item(ui, "Accent popup row", "");
                },
            ));
        }
        let mut fill = false;
        let mut text = false;
        for s in &last.unwrap().shapes {
            visit(&s.shape, &mut |s| {
                if let Shape::Rect(r) = s {
                    fill |= r.fill == t.menu_hover;
                }
                if let Shape::Text(tx) = s {
                    text |= tx.galley.text() == "Accent popup row" && tx.fallback_color == t.menu_text;
                }
            });
        }
        assert!(fill && text, "hovered menu row paints the custom fill and readable text");
        assert!(color(t.menu_hover).contrast(color(t.menu_text)) >= 4.5);
    }
}

#[test]
fn old_settings_and_reset_restore_the_exact_existing_palettes() {
    for dark in [false, true] {
        let mut u = Ui::new(custom_session(dark, "#02DA96"));
        assert_eq!(Tokens::get(&u.ctx), if dark { Tokens::dark() } else { Tokens::light() });
        u.app.session.execute("pref.accentColor", &json!({"preset":"purple"})).unwrap();
        u.frame(vec![]);
        assert_eq!(Tokens::get(&u.ctx).accent_base, Color32::from_rgb(151, 71, 255));
        u.app.session.execute("pref.accentColor", &json!({"reset":true})).unwrap();
        u.frame(vec![]);
        assert_eq!(Tokens::get(&u.ctx), if dark { Tokens::dark() } else { Tokens::light() });
        let _old: Prefs = serde_json::from_value(json!({"theme":"system"})).unwrap();
    }
}

fn named(u: &mut Ui, label: &str) -> Rect {
    u.ctx.enable_accesskit();
    u.frame(vec![]);
    let mut last = None;
    for _ in 0..20 {
        let out = u.frame_output(vec![]);
        let update = out.platform_output.accesskit_update.as_ref().unwrap();
        let node = update
            .nodes
            .iter()
            .map(|(_, node)| node)
            .find(|node| node.label() == Some(label))
            .or_else(|| update.nodes.iter().map(|(_, node)| node).find(|node| node.value() == Some(label)))
            .unwrap_or_else(|| panic!("{label} is accessible"));
        let b = node.bounds().unwrap();
        let bounds = Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32));
        let window = preferences_bounds_window(&out);
        let title = preferences_bounds_text(&out, "Preferences", false).expect("Preferences title is painted");
        let mut visible = window.intersect(u.ctx.content_rect()).shrink(24.0);
        // The title's bottom margin and the content's top margin are 20 px each.
        visible.min.y = visible.min.y.max(title.max.y + 44.0);
        let static_text = matches!(node.role(), egui::accesskit::Role::Label | egui::accesskit::Role::TextRun);
        let static_text_visible = static_text && preferences_bounds_text(&out, label, true).is_some_and(|r| u.ctx.content_rect().contains_rect(r));
        if static_text_visible || (!static_text && visible.contains_rect(bounds)) {
            return bounds;
        }
        last = Some((node.role(), bounds, visible, preferences_bounds_text_clips(&out, label)));
        let delta = if bounds.center().y > visible.center().y { -180.0 } else { 180.0 };
        u.frame(vec![
            Event::PointerMoved(visible.center()),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                source: egui::MouseWheelSource::Unknown,
                delta: vec2(0.0, delta),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        u.frames(3, Vec::new);
    }
    panic!("{label} did not become visible after scrolling Preferences; role/bounds/guessed-body/text-clips: {last:?}");
}

fn enter_accent(u: &mut Ui, text: &str) {
    let at = named(u, "Custom accent hex").center();
    u.button(at, true);
    u.button(at, false);
    let key = |pressed| Event::Key { key: egui::Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND };
    u.frame(vec![key(true), key(false), Event::Text(text.into())]);
    u.key(at, egui::Key::Enter);
    u.frame(vec![]);
}

#[test]
fn preferences_presets_and_custom_hex_dispatch_and_reject_invalid_input() {
    let mut u = Ui::new(custom_session(false, "#02DA96"));
    u.app.ui.preferences = true;
    u.frames(3, Vec::new);
    let at = named(&mut u, "Blue").center();
    u.button(at, true);
    u.button(at, false);
    u.frame(vec![]);
    assert_eq!(serde_json::to_value(&u.app.session.prefs).unwrap()["accent"], "#0D99FF");
    enter_accent(&mut u, "#1122aa");
    assert_eq!(serde_json::to_value(&u.app.session.prefs).unwrap()["accent"], "#1122AA");
    enter_accent(&mut u, "#zzzzzz");
    assert_eq!(serde_json::to_value(&u.app.session.prefs).unwrap()["accent"], "#1122AA");
    let hint = "Use six hexadecimal digits, for example #0D99FF.";
    named(&mut u, hint);
    let out = u.frame_output(vec![]);
    assert!(preferences_bounds_text(&out, hint, true).is_some(), "invalid input keeps an explanatory hint inside the visible clip");
    let at = named(&mut u, "Reset accent color").center();
    u.button(at, true);
    u.button(at, false);
    u.frame(vec![]);
    assert_eq!(Tokens::get(&u.ctx), Tokens::light());
}

fn frame_ui(ctx: &Context, input: RawInput, f: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
    let mut out = ctx.run_ui(input, f);
    out.textures_delta.clear();
    out
}

#[test]
fn escape_closes_preferences_and_discards_uncommitted_hex_before_reopen() {
    for draft in ["#112233", "#invalid"] {
        let mut u = Ui::new(custom_session(false, "#0D99FF"));
        u.app.ui.preferences = true;
        u.frames(3, Vec::new);
        let at = named(&mut u, "Custom accent hex").center();
        u.button(at, true);
        u.button(at, false);
        let key = |pressed| Event::Key { key: egui::Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND };
        u.frame(vec![key(true), key(false), Event::Text(draft.into())]);
        u.key(at, egui::Key::Escape);
        assert!(!u.app.ui.preferences, "Escape closes the dialog");
        assert_eq!(serde_json::to_value(&u.app.session.prefs).unwrap()["accent"], "#0D99FF");
        u.app.ui.preferences = true;
        u.frames(3, Vec::new);
        u.ctx.enable_accesskit();
        u.frame(vec![]);
        let nodes = u.frame_output(vec![]).platform_output.accesskit_update.unwrap().nodes;
        let field = nodes.iter().map(|(_, n)| n).find(|n| n.label() == Some("Custom accent hex")).unwrap();
        assert_eq!(field.value(), Some("#0D99FF"), "the abandoned draft must not survive reopening: {draft}");
        assert_eq!(serde_json::to_value(&u.app.session.prefs).unwrap()["accent"], "#0D99FF");
    }
}

#[test]
fn custom_picker_changes_the_saved_opaque_accent_without_document_edits() {
    let mut u = Ui::new(custom_session(false, "#0D99FF"));
    let doc = u.app.session.ds().unwrap().doc.clone();
    let history = u.app.session.undo_depth();
    u.app.ui.preferences = true;
    u.frames(3, Vec::new);
    let at = named(&mut u, "Choose a custom accent color").center();
    u.button(at, true);
    u.button(at, false);
    u.frames(30, Vec::new);
    assert!(egui::Popup::is_any_open(&u.ctx));
    let seed = Color32::from_rgb(13, 153, 255);
    let mut marker = None;
    let mut circles = Vec::new();
    for s in &u.frame_output(vec![]).shapes {
        visit(&s.shape, &mut |s| {
            if let Shape::Circle(c) = s {
                circles.push((c.center, c.radius, c.fill));
                if c.radius > 10.0 && c.fill == seed {
                    marker = Some((c.center, c.radius));
                }
            }
        });
    }
    let (center, radius) = marker.unwrap_or_else(|| panic!("the color picker's saturation/value marker: {circles:?}"));
    let hsv = egui::ecolor::HsvaGamma::from(egui::ecolor::Hsva::from(seed));
    let size = radius * 12.0;
    let top_left = center - vec2(hsv.s * size, (1.0 - hsv.v) * size);
    let at = top_left + vec2(size * 0.3, size * 0.6);
    u.button(at, true);
    u.button(at, false);
    u.frame(vec![]);
    let saved = serde_json::to_value(&u.app.session.prefs).unwrap();
    let hex = saved["accent"].as_str().unwrap();
    assert_ne!(hex, "#0D99FF");
    assert_eq!(hex.len(), 7);
    assert!(Prefs::parse_accent(hex).is_some());
    assert!(std::sync::Arc::ptr_eq(&doc, &u.app.session.ds().unwrap().doc));
    assert_eq!(history, u.app.session.undo_depth());
}

#[test]
fn accent_labels_are_baked_in_the_spanish_ui_and_command_catalogs() {
    let es = crate::i18n::languages().iter().find(|l| l.code == "es").unwrap();
    for (key, expected) in [
        ("Accent color", "Color de acento"),
        ("Custom accent hex", "Acento hexadecimal personalizado"),
        ("Choose a custom accent color", "Elegir un color de acento personalizado"),
        ("Reset accent color", "Restablecer color de acento"),
        ("Use six hexadecimal digits, for example #0D99FF.", "Usa seis dígitos hexadecimales, por ejemplo #0D99FF."),
    ] {
        let (_, value) = es.ui.iter().find(|(k, _)| *k == key).unwrap_or_else(|| panic!("Spanish UI key {key}"));
        assert!(matches!(value,crate::i18n::Text::Plain(s) if *s==expected));
    }
    let (_, label) = es.commands.iter().find(|(k, _)| *k == "pref.accentColor").unwrap();
    assert!(matches!(label, crate::i18n::Text::Plain("Color de acento")));
}

#[test]
fn actions_accent_color_opens_preferences_without_mutating_design_or_saved_color() {
    let mut u = Ui::new(custom_session(false, "#0D99FF"));
    u.run("shape.rectangle", json!({"width":80,"height":40}));
    let doc = u.app.session.ds().unwrap().doc.clone();
    let history = u.app.session.undo_depth();
    let selection = u.app.session.selection().to_vec();
    let accent = u.app.session.prefs.accent.clone();
    assert!(!u.app.ui.preferences);
    crate::menus::set_palette(&mut u.app.ui, true);
    // The query differs from the result label, so the pointer targets the actual command row.
    u.app.ui.palette = Some("accent".into());
    u.frames(3, Vec::new);
    let shapes = crate::live_tests::flat(&u.frame_output(vec![]).shapes);
    let at = crate::live_tests::text_center(&shapes, "Accent color");
    u.button(at, true);
    u.button(at, false);
    u.frames(3, Vec::new);
    assert!(u.app.ui.preferences, "the Actions accent row opens the editable preference instead of a silent query");
    assert!(u.app.ui.palette.is_none(), "the selected Actions row closes the palette");
    named(&mut u, "Custom accent hex");
    named(&mut u, "Choose a custom accent color");
    assert_eq!(u.app.session.prefs.accent, accent);
    assert!(std::sync::Arc::ptr_eq(&doc, &u.app.session.ds().unwrap().doc));
    assert_eq!(u.app.session.undo_depth(), history);
    assert_eq!(u.app.session.selection(), selection);
}

fn preferences_bounds_frame(ctx: &Context, app: &mut FigApp, height: f32, time: &mut f64, events: Vec<Event>) -> egui::FullOutput {
    *time += 1.0 / 60.0;
    frame_ui(
        ctx,
        RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, height))), time: Some(*time), events, ..Default::default() },
        |ui| app.ui(ui),
    )
}

fn preferences_bounds_window(out: &egui::FullOutput) -> Rect {
    let update = out.platform_output.accesskit_update.as_ref().unwrap();
    let window = update
        .nodes
        .iter()
        .map(|(_, node)| node)
        .find(|node| node.role() == egui::accesskit::Role::Window && node.label() == Some("Preferences"))
        .expect("the actual Preferences window is accessible");
    let b = window.bounds().unwrap();
    Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32))
}

fn preferences_bounds_text_clips(out: &egui::FullOutput, text: &str) -> Vec<(Rect, Rect)> {
    let mut found = Vec::new();
    for clipped in &out.shapes {
        visit(&clipped.shape, &mut |shape| {
            if let Shape::Text(t) = shape
                && t.galley.text() == text
            {
                found.push((Rect::from_min_size(t.pos, t.galley.size()), clipped.clip_rect));
            }
        });
    }
    found
}

fn preferences_bounds_text(out: &egui::FullOutput, text: &str, require_visible: bool) -> Option<Rect> {
    let mut found = None;
    for clipped in &out.shapes {
        visit(&clipped.shape, &mut |shape| {
            if let Shape::Text(t) = shape
                && t.galley.text() == text
            {
                let rect = Rect::from_min_size(t.pos, t.galley.size());
                if !require_visible || clipped.clip_rect.contains_rect(rect) {
                    found = Some(rect);
                }
            }
        });
    }
    found
}

fn preferences_bounds_case(height: f32) {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut app = FigApp::new(custom_session(false, "#0D99FF"), Box::new(NoServices));
    app.ui.preferences = true;
    let doc = app.session.ds().unwrap().doc.clone();
    let history = app.session.undo_depth();
    let mut time = 0.0;
    for _ in 0..5 {
        let _ = preferences_bounds_frame(&ctx, &mut app, height, &mut time, vec![]);
    }
    let mut out = preferences_bounds_frame(&ctx, &mut app, height, &mut time, vec![]);
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1440.0, height));
    let window = preferences_bounds_window(&out);
    assert!(screen.contains_rect(window), "Preferences window must stay in the {height}px viewport: {window:?}");
    let title = preferences_bounds_text(&out, "Preferences", false).expect("Preferences title is painted");
    assert!(screen.contains_rect(title), "the title must stay reachable: {title:?}");
    let mut language = None;
    for _ in 0..16 {
        if let Some(rect) = preferences_bounds_text(&out, "Language", true).filter(|r| screen.contains_rect(*r)) {
            language = Some(rect);
            break;
        }
        out = preferences_bounds_frame(
            &ctx,
            &mut app,
            height,
            &mut time,
            vec![
                Event::PointerMoved(window.center()),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    source: egui::MouseWheelSource::Unknown,
                    delta: vec2(0.0, -180.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        for _ in 0..3 {
            out = preferences_bounds_frame(&ctx, &mut app, height, &mut time, vec![]);
        }
    }
    let language = language.expect("the lower Language control must become visible through real scrolling");
    let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
    let dropdown = nodes
        .iter()
        .map(|(_, node)| node)
        .filter(|node| node.role() == egui::accesskit::Role::ComboBox && node.value() == Some("System"))
        .filter_map(|node| node.bounds())
        .map(|b| Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32)))
        .find(|r| (r.center().y - language.center().y).abs() < 16.0 && r.center().x > language.right() && screen.contains_rect(*r))
        .unwrap_or_else(|| {
            let combos: Vec<_> = nodes
                .iter()
                .map(|(_, node)| node)
                .filter(|node| node.role() == egui::accesskit::Role::ComboBox)
                .map(|node| (node.label(), node.value(), node.bounds()))
                .collect();
            panic!("the visible Language dropdown is reachable at {height}px; label={language:?}, combo label/value/bounds={combos:?}");
        });
    for pressed in [true, false] {
        let at = dropdown.center();
        let _ = preferences_bounds_frame(
            &ctx,
            &mut app,
            height,
            &mut time,
            vec![
                Event::PointerMoved(at),
                Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
            ],
        );
    }
    assert!(egui::Popup::is_any_open(&ctx), "clicking the scrolled Language control opens its options");
    assert!(std::sync::Arc::ptr_eq(&doc, &app.session.ds().unwrap().doc));
    assert_eq!(app.session.undo_depth(), history);
}

#[test]
fn preferences_bounds_title_and_lower_controls_fit_a_900_pixel_viewport() {
    preferences_bounds_case(900.0);
}

#[test]
fn preferences_bounds_title_and_lower_controls_fit_a_700_pixel_viewport() {
    preferences_bounds_case(700.0);
}
