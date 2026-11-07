//! Switching tabs: the canvas shows the image a file had when it was last on screen from the first
//! frame back, instead of a blank page (and, where renders are slow, a coarse preview) while the
//! file renders again.

use egui::{Pos2, RawInput, Rect, vec2};
use figcraft_engine::Session;
use serde_json::json;

use crate::idle_tests::wait_for_render;
use crate::{FigApp, NoServices};

#[test]
fn switching_back_to_a_page_shows_its_refined_image_on_the_first_frame() {
    let mut h = Harness::new(&["A"]);
    h.settle();
    let first = (h.shown(), h.app.canvas.tex_seq, h.app.canvas.texture.as_ref().map(egui::TextureHandle::id));
    h.app.run("page.add", json!({"name": "Second"})).unwrap();
    h.app.run("shape.rectangle", json!({"x": 30, "y": 20, "width": 120, "height": 80})).unwrap();
    h.settle();
    assert_ne!(h.shown(), first.0);
    let before = serde_json::to_vec(h.app.session.doc().unwrap()).unwrap();
    let dirty = h.app.session.ds().unwrap().is_dirty();
    let history = h.app.session.undo_depth().0;
    h.app.run("page.select", json!({"id": first.0.1})).unwrap();
    h.frame();
    let c = &h.app.canvas;
    assert_eq!(c.tex_of, Some(first.0));
    assert_eq!((c.tex_seq, c.texture.as_ref().map(egui::TextureHandle::id)), (first.1, first.2));
    assert!(!c.tex_coarse);
    assert_eq!(serde_json::to_vec(h.app.session.doc().unwrap()).unwrap(), before, "switching images changes no document bytes");
    assert_eq!(h.app.session.ds().unwrap().is_dirty(), dirty);
    assert_eq!(h.app.session.undo_depth().0, history);
    wait_for_render(&h.app);
    h.settle();
}

#[test]
fn a_coarse_preview_is_discarded_instead_of_cached() {
    let ctx = egui::Context::default();
    let mut c = super::CanvasState {
        texture: Some(ctx.load_texture("coarse", egui::ColorImage::filled([2, 2], egui::Color32::RED), egui::TextureOptions::LINEAR)),
        tex_view: Some(figcraft_render::View { origin: (0.0, 0.0), scale: 1.0, width: 2, height: 2 }),
        tex_of: Some((1, figcraft_doc::NodeId(1))),
        tex_coarse: true,
        ..Default::default()
    };
    c.switch_texture(Some((2, figcraft_doc::NodeId(2))), |_| true);
    assert!(c.kept.is_empty());
    assert!(c.texture.is_none() && c.tex_view.is_none() && c.tex_of.is_none());
    assert!(!c.tex_coarse);
}

#[test]
fn switching_retained_images_restores_revision_and_rasterizer() {
    let ctx = egui::Context::default();
    let visual = crate::render_worker::VisualState {
        motion: Some(crate::motion::Preview { uid: 1, revision: 37, page: figcraft_doc::NodeId(1), playhead: 500.0f64.to_bits() }),
        ..Default::default()
    };
    let mut c = super::CanvasState {
        texture: Some(ctx.load_texture("refined", egui::ColorImage::filled([2, 2], egui::Color32::RED), egui::TextureOptions::LINEAR)),
        tex_view: Some(figcraft_render::View { origin: (0.0, 0.0), scale: 1.0, width: 2, height: 2 }),
        tex_of: Some((1, figcraft_doc::NodeId(1))),
        tex_revision: Some(37),
        tex_visual: visual.clone(),
        rasterizer: "cpu",
        tex_seq: 19,
        ..Default::default()
    };
    c.switch_texture(Some((2, figcraft_doc::NodeId(2))), |_| true);
    assert_eq!(c.tex_revision, None);
    assert_eq!(c.rasterizer, "none");
    c.switch_texture(Some((1, figcraft_doc::NodeId(1))), |_| true);
    assert_eq!((c.tex_revision, c.rasterizer, c.tex_seq), (Some(37), "cpu", 19));
    assert!(c.texture.is_some());
    // Returning to a motion preview must retain its visual identity. A new low-resolution job
    // for the same playhead would otherwise replace a sharp cached image of that exact content.
    let view = c.tex_view.clone().unwrap();
    c.want_view = Some(view.clone());
    let preview = crate::render_worker::Done {
        image: Default::default(),
        #[cfg(not(target_arch = "wasm32"))]
        gpu: None,
        view,
        ms: 0.0,
        work: figcraft_render::Work::Full,
        uid: 1,
        page: figcraft_doc::NodeId(1),
        revision: 37,
        visual,
        coarse: true,
        at: web_time::Instant::now(),
        seq: 20,
    };
    assert!(!super::shows(&c, &preview, c.tex_of), "cached sharp motion content stays sharp on return");
}

/// The whole UI, driven headless, with files of their own names open.
struct Harness {
    ctx: egui::Context,
    app: FigApp,
    time: f64,
}

impl Harness {
    /// The UI with a file named after each of `names` (each with a rectangle); the last is shown.
    fn new(names: &[&str]) -> Self {
        figcraft_text::scan_system_fonts();
        let mut s = Session::empty();
        for n in names {
            s.execute("file.new", &json!({"name": n})).unwrap();
            s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 200, "height": 120})).unwrap();
        }
        Self { ctx: egui::Context::default(), app: FigApp::new(s, Box::new(NoServices)), time: 0.0 }
    }

    /// One frame with no input, 1/60 s after the last.
    fn frame(&mut self) {
        self.time += 1.0 / 60.0;
        self.app.clock.set_test_now(Some(self.time));
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))), time: Some(self.time), ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| app.ui(ui));
        out.textures_delta.clear();
    }

    /// Frames until the canvas shows a refined image of the file and page on screen.
    fn settle(&mut self) {
        for _ in 0..200 {
            self.frame();
            wait_for_render(&self.app);
            let ds = self.app.session.ds().unwrap();
            let c = &self.app.canvas;
            if c.tex_of == Some((ds.uid, ds.page)) && !c.tex_coarse && c.texture.is_some() {
                return;
            }
        }
        panic!("the canvas never showed a refined image");
    }

    /// Show the file named `name`, as clicking its tab does.
    fn switch(&mut self, name: &str) {
        let i = self.app.session.docs.iter().position(|d| d.doc.name == name).unwrap();
        self.app.run("doc.switch", json!({"index": i}));
    }

    /// The file and page on screen.
    fn shown(&self) -> (u64, figcraft_doc::NodeId) {
        let ds = self.app.session.ds().unwrap();
        (ds.uid, ds.page)
    }
}

#[test]
fn switching_back_to_a_tab_shows_its_image_on_the_first_frame() {
    let mut h = Harness::new(&["A", "B"]);
    h.switch("A");
    h.settle();
    let a = (h.shown(), h.app.canvas.tex_seq, h.app.canvas.texture.as_ref().map(egui::TextureHandle::id));
    h.switch("B");
    h.settle();
    assert_ne!(h.app.canvas.tex_of, Some(a.0));

    h.switch("A");
    // One frame, before the render of A asked for in it can have come back.
    h.frame();
    let c = &h.app.canvas;
    assert_eq!(c.tex_of, Some(a.0), "A's image is on screen at once");
    assert_eq!((c.tex_seq, c.texture.as_ref().map(egui::TextureHandle::id)), (a.1, a.2), "the image A had when it was last shown");
    assert!(!c.tex_coarse);
    assert!(c.kept.iter().any(|k| k.of.0 != a.0.0), "B's image is kept in turn");
    wait_for_render(&h.app);
    h.settle();
}

#[test]
fn kept_images_are_bounded_and_dropped_with_their_files() {
    let names = ["A", "B", "C", "D", "E", "F", "G"];
    let mut h = Harness::new(&names);
    for n in names {
        h.switch(n);
        h.settle();
    }
    assert_eq!(h.app.canvas.kept.len(), super::KEPT);

    let f = h.app.session.docs.iter().position(|d| d.doc.name == "F").unwrap();
    let f_uid = h.app.session.docs[f].uid;
    assert!(h.app.canvas.kept.iter().any(|k| k.of.0 == f_uid));
    // A tab closed in the background: the file shown stays the same.
    let before = h.shown();
    h.app.run("file.close", json!({"index": f, "force": true}));
    h.frame();
    assert_eq!(h.shown(), before);
    assert!(h.app.canvas.kept.iter().all(|k| k.of.0 != f_uid), "a closed file's image goes");
}
