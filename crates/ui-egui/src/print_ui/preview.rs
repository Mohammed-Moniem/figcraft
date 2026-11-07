//! Bounded raster preview of the same sheet placement as the saved/submitted print PDF.

use egui::{Color32, ColorImage, TextureHandle};
use figcraft_doc::{Document, NodeId};
use figcraft_engine::cmd::file::{PrintSheet, print_sheets};
use figcraft_pdf::Fit;
use serde_json::json;

use crate::FigApp;
use crate::i18n::{tr, trf};

#[derive(PartialEq)]
struct Key {
    uid: u64,
    revision: u64,
    page: NodeId,
    frames: Vec<NodeId>,
    fit: Option<Fit>,
    gray: bool,
    index: usize,
    fonts: u64,
}

pub(super) struct Cached {
    key: Key,
    texture: Option<TextureHandle>,
}

/// At most 320 by 240 pixels. Premultiplied frame pixels are composited on white paper before
/// applying exactly the print backend's grayscale conversion.
pub(super) fn image(doc: &Document, sheet: &PrintSheet, gray: bool) -> Option<ColorImage> {
    let (sw, sh) = sheet.sheet;
    if !sw.is_finite() || !sh.is_finite() || sw <= 0.0 || sh <= 0.0 || !sheet.scale.is_finite() || sheet.scale <= 0.0 {
        return None;
    }
    let k = (320.0 / sw).min(240.0 / sh);
    let (w, h) = ((sw * k).ceil().clamp(1.0, 320.0) as usize, (sh * k).ceil().clamp(1.0, 240.0) as usize);
    let mut pixels = vec![Color32::WHITE; w.checked_mul(h)?];
    let pix = figcraft_render::render_area(doc, &[sheet.frame], sheet.bounds, sheet.scale * k)?;
    let (px, py) = ((sheet.offset.0 * k).round() as i64, (sheet.offset.1 * k).round() as i64);
    for y in 0..pix.height() as usize {
        let cy = y as i64 + py;
        if cy < 0 || cy >= h as i64 {
            continue;
        }
        for x in 0..pix.width() as usize {
            let cx = x as i64 + px;
            if cx < 0 || cx >= w as i64 {
                continue;
            }
            let at = (y * pix.width() as usize + x) * 4;
            let src = pix.data().get(at..at + 4)?;
            let over = |c: u8| (u16::from(c) + 255 - u16::from(src[3])).min(255) as u8;
            let (r, g, b) = (over(src[0]), over(src[1]), over(src[2]));
            let color = if gray {
                let value = figcraft_pdf::print::gray_of(r, g, b);
                Color32::from_gray(value)
            } else {
                Color32::from_rgb(r, g, b)
            };
            *pixels.get_mut(cy as usize * w + cx as usize)? = color;
        }
    }
    Some(ColorImage::new([w, h], pixels))
}

pub(super) fn show(app: &mut FigApp, ui: &mut egui::Ui) {
    let Ok(ds) = app.session.ds() else { return };
    let (uid, revision, page, doc) = (ds.uid, ds.revision, ds.page, ds.doc.clone());
    let frames = super::frames(app, app.ui.print.scope);
    let p = &mut app.ui.print;
    let (fit, gray) = (p.fit(), p.grayscale());
    let Ok(sheets) = print_sheets(&doc, &frames, fit) else {
        p.preview = None;
        ui.label(tr("No print preview is available."));
        return;
    };
    p.preview_index = p.preview_index.min(sheets.len().saturating_sub(1));
    ui.label(tr("Print preview"));
    ui.horizontal(|ui| {
        if ui.add_enabled(p.preview_index > 0, egui::Button::new(tr("Previous sheet"))).clicked() {
            p.preview_index -= 1;
        }
        ui.label(trf("Sheet {page} of {total}", &[("page", &(p.preview_index + 1)), ("total", &sheets.len())]));
        if ui.add_enabled(p.preview_index + 1 < sheets.len(), egui::Button::new(tr("Next sheet"))).clicked() {
            p.preview_index += 1;
        }
    });
    let key = Key { uid, revision, page, frames, fit, gray, index: p.preview_index, fonts: figcraft_text::layout_generation() };
    if p.preview.as_ref().is_none_or(|cached| cached.key != key) {
        let texture = sheets
            .get(p.preview_index)
            .and_then(|sheet| image(&doc.in_srgb(), sheet, gray))
            .map(|image| ui.ctx().load_texture("print-preview", image, egui::TextureOptions::LINEAR));
        p.preview = Some(Cached { key, texture });
    }
    if let Some(texture) = p.preview.as_ref().and_then(|cached| cached.texture.as_ref()) {
        ui.image((texture.id(), texture.size_vec2()));
    } else {
        ui.label(tr("No print preview is available."));
    }
}

pub(super) fn save_pdf(app: &mut FigApp) {
    let Ok(ds) = app.session.ds() else { return };
    let uid = ds.uid;
    let name = format!("{}.print.pdf", figcraft_engine::cmd::file::safe_file_name(&ds.doc.name));
    let p = &app.ui.print;
    let mut params = json!({"ids":super::frames(app,p.scope),"grayscale":p.grayscale()});
    if let Some(fit) = p.fit() {
        params["paper"] = json!(fit.paper);
        params["orientation"] = json!(match fit.orientation {
            figcraft_pdf::FitOrientation::Auto => "auto",
            figcraft_pdf::FitOrientation::Portrait => "portrait",
            figcraft_pdf::FitOrientation::Landscape => "landscape",
        });
        params["scale"] = json!(match fit.scale {
            figcraft_pdf::FitScale::Shrink => "shrink",
            figcraft_pdf::FitScale::Fill => "fit",
        });
        params["margin"] = json!(fit.margin);
    }
    app.pick_save(tr("Save print PDF…"), &name, &["pdf"], move |app, path| {
        if app.session.ds().ok().is_none_or(|ds| ds.uid != uid) {
            app.toast(tr("The document to print changed. Open Print again."));
            return;
        }
        params["path"] = json!(path.to_string_lossy());
        if app.run("file.printPdf", params).is_some() {
            app.toast(trf("Exported {file}", &[("file", &path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())]));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers_drag_tests::Ui;
    use crate::live_tests::{flat, text_center};
    use figcraft_color::{Color, Paint};
    use figcraft_doc::NodeKind;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn print_preview_draws_the_sheet_in_color_and_true_gray_with_white_margins() {
        let mut doc = Document::new("Preview");
        let page = doc.first_page().unwrap();
        let f = doc.create(page, NodeKind::Frame, "Red", 0.0, 0.0, 100.0, 100.0).unwrap();
        doc.get_mut(f).unwrap().fills = vec![Paint::solid(Color::from_hex("ff0000").unwrap())];
        let fit =
            Fit { paper: (200.0, 300.0), orientation: figcraft_pdf::FitOrientation::Portrait, scale: figcraft_pdf::FitScale::Shrink, margin: 0.0 };
        let sheets = print_sheets(&doc, &[f], Some(fit)).unwrap();
        let color = image(&doc, &sheets[0], false).unwrap();
        let gray = image(&doc, &sheets[0], true).unwrap();
        assert_eq!(color.size, [160, 240]);
        assert_eq!(gray.size, color.size);
        let center = 120 * 160 + 80;
        assert_eq!(color.pixels[center], Color32::RED);
        assert_eq!(gray.pixels[center], Color32::from_gray(figcraft_pdf::print::gray_of(255, 0, 0)));
        assert_eq!(gray.pixels[0], Color32::WHITE);
        assert!(gray.pixels.iter().all(|c| c.r() == c.g() && c.g() == c.b()));
    }

    fn editor() -> (Ui, Arc<AtomicUsize>) {
        let mut session = figcraft_engine::Session::new();
        session.execute("frame.create", &json!({"width":200,"height":100,"name":"Preview"})).unwrap();
        let mut u = Ui::new(session);
        let sends = Arc::new(AtomicUsize::new(0));
        u.app.ui.print.discover = Box::new(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            tx.send(Vec::new()).unwrap();
            rx
        });
        let got = sends.clone();
        u.app.ui.print.run_job = Arc::new(move |_, _| {
            got.fetch_add(1, Ordering::SeqCst);
            Ok("synthetic".into())
        });
        u.app.run("file.print", json!({})).unwrap();
        for _ in 0..3 {
            u.frame(vec![]);
        }
        (u, sends)
    }

    #[test]
    fn print_dialog_grayscale_changes_the_cached_pixels_and_cancelling_pdf_sends_no_job() {
        let (mut u, sends) = editor();
        assert!(u.app.ui.print.preview.as_ref().unwrap().texture.is_some());
        let shapes = flat(&u.frame_output(vec![]).shapes);
        let gray = text_center(&shapes, "Grayscale");
        u.button(gray, true);
        u.button(gray, false);
        u.frame(vec![]);
        assert!(u.app.ui.print.preview.as_ref().unwrap().key.gray);
        let shapes = flat(&u.frame_output(vec![]).shapes);
        let save = text_center(&shapes, "Save print PDF…");
        u.button(save, true);
        u.button(save, false);
        for _ in 0..3 {
            u.frame(vec![]);
        }
        assert_eq!(sends.load(Ordering::SeqCst), 0, "NoServices cancelled the save dialog; no queue was contacted");
    }

    #[test]
    fn changing_the_document_dismisses_the_owned_print_draft_before_any_send() {
        let (mut u, sends) = editor();
        assert!(u.app.ui.print.owner.is_some());
        u.app.run("file.new", json!({})).unwrap();
        assert!(super::super::plan(&u.app).is_err(), "the old draft cannot plan the newly opened file");
        u.frame(vec![]);
        assert!(!u.app.ui.print.open);
        assert!(u.app.ui.print.preview.is_none());
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn print_failure_after_dialog_close_is_reported_once_without_resubmitting() {
        let (mut u, sends) = editor();
        let (tx, rx) = std::sync::mpsc::channel();
        assert!(u.app.ui.print.spooler.begin(rx).is_ok());
        u.app.ui.print.open = false;
        tx.send(Err("synthetic queue unavailable".into())).unwrap();
        super::super::poll(&mut u.app, &u.ctx);
        let text = u.app.ui.toast.as_ref().unwrap().0.clone();
        assert!(text.contains("synthetic queue unavailable"), "{text}");
        u.app.ui.toast = None;
        super::super::poll(&mut u.app, &u.ctx);
        assert!(u.app.ui.toast.is_none(), "the same failure is not repeated every frame");
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn print_draft_survives_another_native_window_and_programmatic_print_never_opens_it() {
        let (mut u, sends) = editor();
        u.ctx.set_embed_viewports(false);
        u.app.run("file.new", json!({"name":"Other window"})).unwrap();
        let window = crate::control::handle(&mut u.app, &u.ctx, "window.new", &json!({"index":1})).unwrap()["window"].as_u64().unwrap();
        u.frame(vec![]);
        // The previous draft correctly closed when this window temporarily switched to the new
        // file. Open a draft after the two windows have their stable, distinct documents.
        super::super::request(&mut u.app);
        u.frame(vec![]);
        crate::windows::as_window(&mut u.app, window, |app| {
            super::super::poll(app, &u.ctx);
            assert!(!app.ui.print.open, "the child has its own UI state and cannot steal the main window's print draft");
            assert!(!app.ui.print.is_here(window));
        })
        .unwrap();
        assert!(u.app.ui.print.is_here(0), "the main window's print draft remains open after the child's state is restored");
        u.app.ui.print.open = false;
        crate::control::handle(&mut u.app, &u.ctx, "engine.execute", &json!({"command":"file.print","params":{}})).unwrap();
        assert!(!u.app.ui.print.open, "programmatic commands discard the dialog request");
        assert!(u.app.session.ui_requests.is_empty());
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }
}
