//! #2076 investigation: separate live editor chrome from the page's saved background pixels.

use std::collections::HashMap;

use egui::{Context, RawInput, Rect, TextureId, Theme, pos2, vec2};
use figcraft_doc::NodeKind;
use figcraft_engine::Session;
use serde_json::json;

use crate::idle_tests::wait_for_render;
use crate::theme::Tokens;
use crate::{FigApp, NoServices};

struct Harness {
    ctx: Context,
    app: FigApp,
    time: f64,
    shapes: Vec<egui::epaint::ClippedShape>,
    images: HashMap<TextureId, egui::ColorImage>,
    system: Option<Theme>,
}

impl Harness {
    fn new(session: Session) -> Self {
        figcraft_text::scan_system_fonts();
        Self {
            ctx: Context::default(),
            app: FigApp::new(session, Box::new(NoServices)),
            time: 0.0,
            shapes: Vec::new(),
            images: HashMap::new(),
            system: Some(Theme::Light),
        }
    }

    fn frame(&mut self) {
        self.time += 1.0 / 60.0;
        self.app.clock.set_test_now(Some(self.time));
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1440.0, 900.0))),
            time: Some(self.time),
            system_theme: self.system,
            ..Default::default()
        };
        let app = &mut self.app;
        let mut output = self.ctx.run_ui(input, |ui| app.ui(ui));
        let canvas = self.app.canvas.texture.as_ref().map(egui::TextureHandle::id);
        for (id, deltas) in &output.textures_delta.set {
            if Some(*id) == canvas {
                for delta in deltas {
                    assert!(delta.pos.is_none(), "the canvas currently uploads complete images");
                    let egui::ImageData::Color(image) = &delta.image;
                    self.images.insert(*id, image.as_ref().clone());
                }
            }
        }
        for id in &output.textures_delta.free {
            self.images.remove(id);
        }
        self.shapes = output.shapes;
        output.textures_delta.clear();
    }

    fn settle(&mut self) {
        for index in 0..100 {
            self.frame();
            wait_for_render(&self.app);
            let ds = self.app.session.ds().unwrap();
            let canvas = &self.app.canvas;
            if index >= 3
                && canvas.tex_of == Some((ds.uid, ds.page))
                && canvas.tex_revision == Some(ds.revision)
                && !canvas.tex_coarse
                && canvas.texture.is_some()
            {
                return;
            }
        }
        panic!("the synthetic page never showed its refined image");
    }

    fn run(&mut self, command: &str, params: serde_json::Value) {
        self.app.session.execute(command, &params).unwrap();
        self.app.handle_ui_requests();
    }

    fn assert_paint(&self, dark: bool) {
        let expected = if dark { Tokens::dark() } else { Tokens::light() };
        assert_eq!(self.app.ui.dark, dark);
        assert_eq!(Tokens::get(&self.ctx).panel, expected.panel);
        let panel_point = pos2(60.0, 430.0);
        assert!(
            self.shapes.iter().any(|shape| shape.clip_rect.contains(panel_point)
                && rectangle(&shape.shape, |rect| {
                    rect.rect.contains(panel_point) && rect.rect.width() > 150.0 && rect.rect.height() > 300.0 && rect.fill == expected.panel
                })),
            "the drawn left editor panel must use the new theme, without a tab switch"
        );
        let ds = self.app.session.ds().unwrap();
        let NodeKind::Page { background, .. } = ds.doc.node(ds.page).unwrap().kind else { panic!("page") };
        let page_color = crate::theme::doc_c32(&self.ctx, background);
        assert!(
            self.shapes.iter().any(|shape| rectangle(&shape.shape, |rect| { rect.rect == self.app.canvas.rect && rect.fill == page_color })),
            "the canvas backdrop paint follows the saved page color"
        );
    }

    fn assert_texture_matches_document(&self) {
        let ds = self.app.session.ds().unwrap();
        let canvas = &self.app.canvas;
        assert_eq!(canvas.rasterizer, "cpu");
        let texture = canvas.texture.as_ref().unwrap().id();
        let shown = self.images.get(&texture).expect("capture the image actually uploaded to the presented canvas texture");
        let NodeKind::Page { background, .. } = ds.doc.node(ds.page).unwrap().kind else { panic!("page") };
        let dots = crate::board_bar::dots(&self.app, crate::theme::doc_c32(&self.ctx, background), &Tokens::get(&self.ctx));
        let reference = figcraft_render::render_view(
            &ds.doc,
            ds.page,
            canvas.tex_view.as_ref().unwrap(),
            &figcraft_render::Options { dots, lod: true, ..Default::default() },
        )
        .unwrap();
        assert_eq!(shown.size, [reference.width() as usize, reference.height() as usize]);
        let bytes: Vec<u8> = shown.pixels.iter().flat_map(|pixel| pixel.to_array()).collect();
        let different = bytes.iter().zip(reference.data()).filter(|(a, b)| a != b).count();
        assert!(
            bytes == reference.data(),
            "the retained uploaded page pixels match an independent fresh document render ({different} differing bytes)"
        );
    }

    fn documents(&self) -> Vec<(Vec<u8>, bool)> {
        self.app.session.docs.iter().map(|doc| (serde_json::to_vec(&doc.doc).unwrap(), doc.is_dirty())).collect()
    }
}

fn rectangle(shape: &egui::Shape, predicate: impl Fn(&egui::epaint::RectShape) -> bool + Copy) -> bool {
    match shape {
        egui::Shape::Rect(rect) => predicate(rect),
        egui::Shape::Vec(shapes) => shapes.iter().any(|shape| rectangle(shape, predicate)),
        _ => false,
    }
}

fn design() -> Session {
    let mut session = Session::empty();
    session.set_ui_dark(false);
    session.execute("file.new", &json!({"name":"Original theme fixture"})).unwrap();
    session.execute("shape.rectangle", &json!({"x":10,"y":10,"width":100,"height":80})).unwrap();
    session.ds_mut().unwrap().mark_saved();
    session
}

#[test]
fn theme_background_manual_toggles_repaint_chrome_and_keep_clean_or_dirty_design_pixels() {
    for dirty in [false, true] {
        let mut session = design();
        if dirty {
            session.execute("shape.rectangle", &json!({"x":200,"y":150,"width":40,"height":40})).unwrap();
        }
        let mut h = Harness::new(session);
        h.run("view.themeLight", json!({}));
        h.settle();
        let baseline = h.documents();
        let history = h.app.session.undo_depth();
        for dark in [true, false, true, false] {
            h.run(if dark { "view.themeDark" } else { "view.themeLight" }, json!({}));
            h.frame();
            h.assert_paint(dark);
            h.settle();
            h.assert_paint(dark);
            h.assert_texture_matches_document();
            assert_eq!(h.documents(), baseline, "theme changes preserve page bytes and dirty state");
            assert_eq!(h.app.session.undo_depth(), history, "theme changes make no document undo entry");
        }
        let saved = figcraft_format::save_bytes(h.app.session.doc().unwrap(), None).unwrap();
        let loaded = figcraft_format::load_bytes_with_versions(&saved).unwrap();
        let ds = h.app.session.ds().unwrap();
        assert_eq!(loaded.doc.node(ds.page).unwrap().kind, ds.doc.node(ds.page).unwrap().kind, "native serialization keeps the page color");
    }
}

#[test]
fn theme_background_system_changes_and_tab_switches_keep_each_explicit_page_color() {
    let mut session = design();
    session.execute("page.setBackground", &json!({"color":"#336699"})).unwrap();
    session.ds_mut().unwrap().mark_saved();
    session.execute("file.new", &json!({"name":"Original dark-valued explicit page"})).unwrap();
    session.execute("page.setBackground", &json!({"color":"#1E1E1E"})).unwrap();
    session.ds_mut().unwrap().mark_saved();
    let mut h = Harness::new(session);
    h.run("view.themeSystem", json!({}));
    h.settle();
    let baseline = h.documents();
    for system in [Theme::Dark, Theme::Light, Theme::Dark] {
        h.system = Some(system);
        h.frame();
        h.assert_paint(system == Theme::Dark);
        h.settle();
        h.assert_texture_matches_document();
        for index in [0, 1] {
            h.run("doc.switch", json!({"index":index}));
            h.settle();
            h.assert_paint(system == Theme::Dark);
            h.assert_texture_matches_document();
        }
        assert_eq!(h.documents(), baseline, "explicit colors, including a theme-default-valued choice, are document data");
    }
}

#[test]
fn theme_background_board_stays_light_and_its_presented_grid_matches_each_theme() {
    let mut session = Session::empty();
    session.set_ui_dark(false);
    session.execute("file.newBoard", &json!({"name":"Original board theme fixture"})).unwrap();
    let mut h = Harness::new(session);
    h.settle();
    let baseline = h.documents();
    for dark in [true, false, true, false] {
        h.run(if dark { "view.themeDark" } else { "view.themeLight" }, json!({}));
        h.frame();
        h.assert_paint(dark);
        h.settle();
        h.assert_texture_matches_document();
        assert_eq!(h.documents(), baseline, "the Board's intentionally light saved page never follows editor theme");
    }
}

#[test]
fn theme_background_a_real_page_edit_invalidates_retained_pixels_without_switching_tabs() {
    let mut h = Harness::new(design());
    h.settle();
    let original_document = serde_json::to_vec(h.app.session.doc().unwrap()).unwrap();
    let before = h.app.canvas.tex_seq;
    h.run("page.setBackground", json!({"color":"#225588"}));
    h.settle();
    assert!(h.app.canvas.tex_seq > before, "an actual page-background edit must render without a tab change");
    h.assert_paint(false);
    h.assert_texture_matches_document();
    h.run("edit.undo", json!({}));
    h.settle();
    h.assert_texture_matches_document();
    assert_eq!(serde_json::to_vec(h.app.session.doc().unwrap()).unwrap(), original_document, "undo restores the exact document content");
    assert_eq!(h.app.session.undo_depth().1, 1);
}

fn saved_blank(color: &str) -> Vec<u8> {
    let mut session = Session::new();
    session.execute("page.setBackground", &json!({"color":color})).unwrap();
    figcraft_format::save_bytes(session.doc().unwrap(), None).unwrap()
}

fn native_data(color: &str) -> Harness {
    use base64::Engine as _;
    let mut session = Session::empty();
    session
        .execute(
            "file.open",
            &json!({"name":"Original saved blank.figcraft","data":base64::engine::general_purpose::STANDARD.encode(saved_blank(color))}),
        )
        .unwrap();
    session.execute("view.themeDark", &json!({})).unwrap();
    Harness::new(session)
}

fn page_color(h: &Harness) -> figcraft_color::Color {
    let ds = h.app.session.ds().unwrap();
    match ds.doc.node(ds.page).unwrap().kind {
        NodeKind::Page { background, .. } => background,
        _ => panic!("page"),
    }
}

#[test]
fn theme_background_native_data_first_dark_frame_preserves_the_saved_light_default() {
    let mut h = native_data("#F5F5F5");
    assert_eq!(page_color(&h), figcraft_color::Color::DEFAULT_PAGE);
    let before = h.documents();
    h.settle();
    assert_eq!(
        page_color(&h),
        figcraft_color::Color::DEFAULT_PAGE,
        "saved native page authority does not depend on whether the input had a filesystem path"
    );
    assert_eq!(h.documents(), before);
    assert_eq!(h.app.session.undo_depth(), (0, 0));
    h.assert_paint(true);
    h.assert_texture_matches_document();
}

#[test]
fn theme_background_native_data_custom_color_is_already_authoritative_on_first_frame() {
    let mut h = native_data("#336699");
    let before = h.documents();
    h.settle();
    assert_eq!(page_color(&h), figcraft_color::Color::from_rgb8_const(0x33, 0x66, 0x99));
    assert_eq!(h.documents(), before);
    h.assert_paint(true);
    h.assert_texture_matches_document();
}

#[test]
fn theme_background_native_path_first_dark_frame_keeps_the_same_saved_light_default() {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let index = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch = Scratch(std::env::temp_dir().join(format!("figcraft-theme-native-{}-{index}", std::process::id())));
    std::fs::create_dir(&scratch.0).unwrap();
    let path = scratch.0.join("Original saved blank.figcraft");
    let bytes = saved_blank("#F5F5F5");
    std::fs::write(&path, &bytes).unwrap();
    let mut session = Session::empty();
    session.execute("file.open", &json!({"path":path.to_string_lossy()})).unwrap();
    session.execute("view.themeDark", &json!({})).unwrap();
    let mut h = Harness::new(session);
    let before = h.documents();
    h.settle();
    assert_eq!(page_color(&h), figcraft_color::Color::DEFAULT_PAGE);
    assert_eq!(h.documents(), before);
    assert_eq!(std::fs::read(path).unwrap(), bytes, "displaying the native input never rewrites its file");
    h.assert_paint(true);
    h.assert_texture_matches_document();
}

#[test]
fn theme_background_native_control_new_initial_blank_still_adopts_its_first_dark_theme() {
    let mut session = Session::new();
    session.execute("view.themeDark", &json!({})).unwrap();
    let mut h = Harness::new(session);
    h.settle();
    assert_eq!(page_color(&h), figcraft_color::Color::DARK_PAGE);
    assert!(!h.app.session.ds().unwrap().is_dirty());
    assert_eq!(h.app.session.undo_depth(), (0, 0));
    h.assert_paint(true);
    h.assert_texture_matches_document();
}

#[test]
fn theme_background_native_model_input_keeps_supplied_blank_page_authority() {
    let loaded = figcraft_format::load_bytes_with_versions(&saved_blank("#F5F5F5")).unwrap();
    let mut session = Session::with_document(loaded.doc);
    session.execute("view.themeDark", &json!({})).unwrap();
    let mut h = Harness::new(session);
    let before = h.documents();
    h.settle();
    assert_eq!(page_color(&h), figcraft_color::Color::DEFAULT_PAGE, "a supplied model is not a fresh Design creation");
    assert_eq!(h.documents(), before);
    h.assert_paint(true);
    h.assert_texture_matches_document();
}

#[test]
fn theme_background_native_control_explicit_new_design_before_first_ui_still_adopts_theme() {
    let mut session = Session::empty();
    session.execute("file.new", &json!({"name":"Original fresh Design"})).unwrap();
    session.execute("view.themeDark", &json!({})).unwrap();
    let mut h = Harness::new(session);
    h.settle();
    assert_eq!(page_color(&h), figcraft_color::Color::DARK_PAGE);
    assert!(!h.app.session.ds().unwrap().is_dirty());
    assert_eq!(h.app.session.undo_depth(), (0, 0));
    h.assert_texture_matches_document();
}
