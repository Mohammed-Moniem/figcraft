//! Motion export controls. Encoding and file access belong to the engine command.

use egui::Context;
use figcraft_doc::NodeId;
use serde_json::{Value, json};

use crate::FigApp;
use crate::i18n::{tr, trf};

#[derive(Clone)]
pub struct ExportUi {
    owner: (u64, NodeId),
    frame: NodeId,
    duration: u32,
    fps: u32,
    looping: bool,
    scale: f64,
    frame_picker: Option<egui::Id>,
}

impl ExportUi {
    fn params(&self) -> Value {
        json!({"id":self.frame,"format":"gif","duration":self.duration,"fps":self.fps,"loop":self.looping,"scale":self.scale})
    }

    fn close_picker(&self, ctx: &Context) {
        if let Some(button) = self.frame_picker {
            egui::Popup::close_id(ctx, button.with("popup"));
        }
    }

    fn current(&self, app: &FigApp) -> bool {
        app.session.ds().is_ok_and(|d| (d.uid, d.page) == self.owner)
    }
}

pub fn open(app: &mut FigApp) {
    let Ok(ds) = app.session.ds() else { return };
    let frames = figcraft_engine::cmd::animation_export::frames(&ds.doc, ds.page);
    let Some(mut frame) = frames.first().copied() else {
        app.toast(tr("Add a visible top-level frame to export animation."));
        return;
    };
    if let [selected] = app.session.selection().as_slice() {
        let mut candidate = Some(*selected);
        for _ in 0..=figcraft_render::MAX_EXPORT_DEPTH {
            let Some(id) = candidate else { break };
            if frames.contains(&id) {
                frame = id;
                break;
            }
            candidate = ds.doc.node(id).and_then(|n| n.parent);
        }
    }
    let settings = ds.doc.timelines.get(&ds.page).copied().unwrap_or_default();
    app.ui.motion.export = Some(ExportUi {
        owner: (ds.uid, ds.page),
        frame,
        duration: settings.duration_ms,
        fps: settings.fps.min(100),
        looping: settings.looping,
        scale: 1.0,
        frame_picker: None,
    });
    app.ui.motion.playing = false;
}

/// The exporter owns canvas keys through its closing frame without consuming other widgets' events.
pub(crate) fn owns_input(app: &FigApp, ctx: &Context) -> bool {
    app.ui.motion.export.is_some() || app.ui.motion.export_input_frame == Some((ctx.viewport_id(), ctx.cumulative_frame_nr()))
}

/// An unavailable timeline cannot keep an invisible editor or its picker alive.
pub(crate) fn retire(app: &mut FigApp, ctx: &Context) {
    let Some(state) = app.ui.motion.export.take() else { return };
    app.ui.motion.export_input_frame = Some((ctx.viewport_id(), ctx.cumulative_frame_nr()));
    state.close_picker(ctx);
}

/// Drawn by the timeline; its owning document/page and the async save callback are both checked.
pub fn show(app: &mut FigApp, ctx: &Context) {
    let Some(mut state) = app.ui.motion.export.take() else { return };
    app.ui.motion.export_input_frame = Some((ctx.viewport_id(), ctx.cumulative_frame_nr()));
    if !app.ui.motion_open || !state.current(app) {
        state.close_picker(ctx);
        return;
    }
    let mut open = true;
    let mut save = false;
    let popup_was_open = egui::Popup::is_any_open(ctx);
    let mut picker_owned_batch = state.frame_picker.is_some_and(|id| craft_ui::ComboBox::is_open(ctx, id));
    let frames = app.session.doc().map(|d| figcraft_engine::cmd::animation_export::frames(d, state.owner.1)).unwrap_or_default();
    let name = |id| app.session.doc().ok().and_then(|d| d.node(id)).map(|n| n.name.clone()).unwrap_or_default();
    crate::dialogs::window(tr("Export animation"), ctx).id(egui::Id::unique(("motion_export", state.owner))).open(&mut open).show(ctx, |ui| {
        ui.set_min_width(320.0);
        egui::Grid::new("motion_export_settings").num_columns(2).spacing([18.0, 10.0]).show(ui, |ui| {
            ui.label(tr("Frame"));
            let response = craft_ui::ComboBox::from_id_salt("motion_export_frame").selected_text(name(state.frame)).show_ui(ui, |ui| {
                for id in &frames {
                    ui.selectable_value(&mut state.frame, *id, name(*id));
                }
            });
            state.frame_picker = Some(response.response.id);
            picker_owned_batch |= response.inner.is_some() || craft_ui::ComboBox::is_open(ctx, response.response.id);
            ui.end_row();
            ui.label(tr("Duration"));
            ui.add(egui::DragValue::new(&mut state.duration).range(10..=600000).suffix(" ms"));
            ui.end_row();
            ui.label(tr("Frame rate"));
            ui.add(egui::DragValue::new(&mut state.fps).range(1..=100).suffix(" fps"));
            ui.end_row();
            ui.label(tr("Scale"));
            ui.add(egui::DragValue::new(&mut state.scale).range(0.01..=64.0).speed(0.1).suffix("×"));
            ui.end_row();
            ui.label(tr("Playback"));
            ui.checkbox(&mut state.looping, tr("Loop"));
            ui.end_row();
        });
        ui.add_space(12.0);
        ui.label(tr("GIF uses 256 colours per frame, binary transparency and 10 ms timing. Export keeps the frame's viewport fixed."));
        ui.add_space(12.0);
        save = ui.add_enabled(frames.contains(&state.frame), egui::Button::new(tr("Export GIF…"))).clicked();
    });
    // A picker owns its Escape batch even after egui closes the popup. Foreign popups keep
    // their own keys; the exporter does not cancel while one held this batch.
    if picker_owned_batch {
        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    } else if !popup_was_open && !egui::Popup::is_any_open(ctx) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        open = false;
    }
    // A cancellation in this input batch vetoes a captured release.
    if open && save {
        state.close_picker(ctx);
        save_one(app, state);
    } else if open {
        app.ui.motion.export = Some(state);
    } else {
        state.close_picker(ctx);
    }
}

fn save_one(app: &mut FigApp, state: ExportUi) {
    let title = app.session.doc().ok().and_then(|d| d.node(state.frame)).map(|n| n.name.clone()).unwrap_or_else(|| "Animation".into());
    let file = format!("{}.gif", figcraft_engine::cmd::file::safe_file_name(&title));
    let mut params = state.params();
    if app.services.downloads() {
        if let Some(result) = app.run("motion.export", params)
            && app.download(&file, &result)
        {
            app.toast(trf("Exported {name}", &[("name", &file)]));
        }
        return;
    }
    app.pick_save(tr("Export animation"), &file, &["gif"], move |app, path| {
        if !state.current(app) {
            app.toast(tr("Animation export cancelled because the document or page changed."));
            return;
        }
        params["path"] = json!(path.to_string_lossy());
        if app.run("motion.export", params).is_some() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            app.toast(trf("Exported {name}", &[("name", &name)]));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DialogFuture, NoServices, Services};
    use figcraft_doc::NodeKind;
    use figcraft_engine::Session;

    fn scene() -> (Session, NodeId, NodeId) {
        let mut session = Session::new();
        let page = session.page().unwrap();
        let doc = session.doc_mut().unwrap();
        doc.create(page, NodeKind::Frame, "First", 0.0, 0.0, 16.0, 12.0).unwrap();
        let second = doc.create(page, NodeKind::Frame, "Selected", 50.0, 0.0, 16.0, 12.0).unwrap();
        let child = doc.create(second, NodeKind::Rectangle, "Child", 0.0, 0.0, 4.0, 4.0).unwrap();
        (session, second, child)
    }

    #[test]
    fn motion_export_prefers_the_selected_childs_frame_and_page_settings() {
        let (mut session, frame, child) = scene();
        session.execute("select.set", &json!({"ids":[child]})).unwrap();
        session.execute("motion.timeline", &json!({"duration":800,"fps":25,"loop":true})).unwrap();
        let mut app = FigApp::new(session, Box::new(NoServices));
        open(&mut app);
        let state = app.ui.motion.export.as_ref().unwrap();
        assert_eq!(state.frame, frame);
        assert_eq!(state.params(), json!({"id":frame,"format":"gif","duration":800,"fps":25,"loop":true,"scale":1.0}));
    }

    struct Picker(std::path::PathBuf);
    impl Services for Picker {
        fn pick_save(&mut self, _title: &str, _name: &str, exts: &[&str]) -> DialogFuture {
            assert_eq!(exts, ["gif"]);
            Box::pin(std::future::ready(Some(self.0.clone())))
        }
    }

    #[test]
    fn motion_export_pending_save_cannot_export_another_document_with_the_same_ids() {
        let (session, _, _) = scene();
        let file = std::env::temp_dir().join(format!("figcraft-motion-owner-{}.gif", std::process::id()));
        assert!(!file.exists());
        let mut app = FigApp::new(session, Box::new(Picker(file.clone())));
        open(&mut app);
        let state = app.ui.motion.export.take().unwrap();
        save_one(&mut app, state);
        app.session.execute("file.new", &json!({"name":"Other document"})).unwrap();
        app.session.execute("file.close", &json!({"index":0,"force":true})).unwrap();
        app.poll_dialog(&egui::Context::default());
        assert!(!file.exists());
    }

    #[test]
    fn motion_export_dialog_closes_on_escape_and_page_switch() {
        let (session, _, _) = scene();
        let mut ui = crate::layers_drag_tests::Ui::new(session);
        crate::motion::set_open(&mut ui.app, true);
        open(&mut ui.app);
        ui.frame(vec![]);
        ui.frame(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(ui.app.ui.motion.export.is_none());
        open(&mut ui.app);
        let new = ui.app.session.execute("page.add", &json!({"name":"Other page"})).unwrap()["id"].clone();
        ui.app.session.execute("page.select", &json!({"id":new})).unwrap();
        ui.frame(vec![]);
        assert!(ui.app.ui.motion.export.is_none());
    }
}
