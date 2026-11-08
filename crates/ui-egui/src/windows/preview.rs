//! Native prototype companions never own editor tabs or a second design session.
use super::{
    Context, FigApp, Geometry, Pane, Value, Vec2, ViewportBuilder, ViewportCommand, Windows, exchange, focus, forget_control_input, guard, json, tr,
    viewport, with_app,
};
use crate::present;

const MAX_PREVIEWS: usize = 8;

pub(super) struct Preview {
    pub(super) id: u64,
    pub(super) uid: u64,
    pub(super) pane: Pane,
    geo: Geometry,
}

pub(crate) fn unavailable(ctx: &Context) -> Option<&'static str> {
    if ctx.embed_viewports() {
        Some(tr("Separate preview windows are unavailable in this host"))
    } else if crate::a11y::active(ctx) {
        Some(tr("Separate preview windows are unavailable while a screen reader is active"))
    } else {
        None
    }
}

impl Windows {
    pub(crate) fn is_preview(&self, id: u64) -> bool {
        self.previews.iter().any(|p| p.id == id)
    }
}

/// Resolve the command's captured UID, never the tab selected when its UI request is drained.
pub(crate) fn detach(app: &mut FigApp, ctx: &Context, uid: u64, start: figcraft_doc::NodeId) -> Result<u64, String> {
    if let Some(preview) = app.windows.previews.iter().find(|p| p.uid == uid) {
        let id = preview.id;
        focus(app, ctx, id)?;
        return Ok(id);
    }
    let ds = app.session.docs.iter().find(|d| d.uid == uid).ok_or("the preview's source file closed")?;
    if ds.doc.top_level_frame(start) != Some(start) {
        return Err("the preview's source frame is no longer open".into());
    }
    let fresh = present::Pane::for_document(start, ds);
    let owner = app.windows.of(uid);
    // Take only the inline player for this exact live UID. Its current navigation, overlays and
    // runtime variables transfer; a pane from another document is never retargeted.
    let mut pane = if owner == app.windows.current {
        app.ui.preview.take_if(|p| p.source_uid() == uid)
    } else {
        app.windows.list.iter_mut().find(|w| w.id == owner).and_then(|w| w.pane.ui.preview.take_if(|p| p.source_uid() == uid))
    }
    .unwrap_or(fresh);
    if let Some(reason) = unavailable(ctx) {
        put_inline(app, uid, pane);
        return Err(reason.into());
    }
    if app.windows.previews.len() >= MAX_PREVIEWS {
        put_inline(app, uid, pane);
        return Err(tr("At most eight separate preview windows can be open").into());
    }
    let Some(id) = app.windows.last_id.checked_add(1) else {
        put_inline(app, uid, pane);
        return Err(tr("No more window identities are available").into());
    };
    app.windows.last_id = id;
    pane.player.release_surface_input(ctx.input(|i| i.time), app.services.gamepad_held());
    let mut state = Pane::default();
    state.ui.preview = Some(pane);
    let pos = ctx.input(|i| i.viewport().outer_rect.map(|r| r.min + Vec2::splat(32.0)));
    app.windows.previews.push(Preview { id, uid, pane: state, geo: Geometry { pos, size: egui::vec2(320.0, 600.0), ..Geometry::default() } });
    focus(app, ctx, id)?;
    ctx.request_repaint();
    Ok(id)
}

fn put_inline(app: &mut FigApp, uid: u64, pane: present::Pane) {
    let owner = app.windows.of(uid);
    if owner == app.windows.current && app.session.ds().is_ok_and(|ds| ds.uid == uid) {
        app.ui.preview = Some(pane);
    } else if let Some(w) = app.windows.list.iter_mut().find(|w| w.id == owner && w.active == Some(uid)) {
        w.pane.ui.preview = Some(pane);
    }
}

pub(super) fn retire(app: &mut FigApp, ctx: &Context) {
    let retired: Vec<u64> =
        app.windows.previews.iter().filter(|p| app.ui.quit || !app.session.docs.iter().any(|d| d.uid == p.uid)).map(|p| p.id).collect();
    for id in retired {
        close(app, ctx, id);
    }
}

pub(super) fn close(app: &mut FigApp, ctx: &Context, id: u64) {
    // Drop queued text and its resource-bearing acknowledgements before retiring this viewport.
    app.native_menu_inputs.remove(&viewport(id));
    forget_control_input(app, ctx, viewport(id));
    app.windows.previews.retain(|p| p.id != id);
    app.windows.presented.remove(&id);
    app.ime.end_viewport(viewport(id));
    if app.windows.focused == id {
        app.windows.focused = 0;
    }
    ctx.send_viewport_cmd_to(viewport(id), ViewportCommand::Close);
}

pub(super) fn show(app: &FigApp, ctx: &Context) {
    for p in &app.windows.previews {
        let name = app.session.docs.iter().find(|d| d.uid == p.uid).map(|d| d.doc.name.as_str()).unwrap_or("");
        let mut builder = ViewportBuilder::default()
            .with_title(format!("{} – {}", tr("Prototype preview"), name))
            .with_decorations(true)
            .with_resizable(true)
            .with_inner_size(p.geo.egui(ctx).size)
            .with_min_inner_size([220.0, 260.0])
            .with_drag_and_drop(false);
        if let Some(pos) = p.geo.pos {
            builder = builder.with_position(pos / ctx.zoom_factor());
        }
        let id = p.id;
        ctx.show_viewport_deferred(viewport(id), builder, move |ui, _| {
            let ctx = ui.ctx().clone();
            with_app(|app| guard(app, &ctx, |app| draw(app, id, ui)));
        });
    }
    repaint_changed(app, ctx);
}

/// Wake only companions whose last drawn source changed. This also runs after logic-only
/// work, so live/control edits do not depend on an editor or child window painting first.
pub(crate) fn repaint_changed(app: &FigApp, ctx: &Context) {
    for p in &app.windows.previews {
        let Some(pane) = p.pane.ui.preview.as_ref() else { continue };
        let Some(ds) = app.session.docs.iter().find(|d| d.uid == p.uid) else { continue };
        if !pane.shows_source(ds.without_widget_hover()) {
            ctx.request_repaint_of(viewport(p.id));
        }
    }
}

/// A companion temporarily presents its UID for existing inspection/input/capture machinery;
/// its source remains in the editor that owns it and the previous tab is restored afterward.
pub(super) fn as_window<R>(app: &mut FigApp, id: u64, f: impl FnOnce(&mut FigApp) -> R) -> Option<R> {
    let index = app.windows.previews.iter().position(|p| p.id == id)?;
    let uid = app.windows.previews.get(index)?.uid;
    let target = app.session.docs.iter().position(|d| d.uid == uid)?;
    let before = app.session.ds().ok().map(|d| d.uid);
    let current = app.windows.current;
    let main_active = app.windows.main_active;
    if current == 0 {
        app.windows.main_active = before;
    }
    let mut pane = std::mem::take(&mut app.windows.previews.get_mut(index)?.pane);
    exchange(app, &mut pane);
    app.session.active = target;
    app.windows.current = id;
    app.observe_presentation();
    let result = f(app);
    exchange(app, &mut pane);
    if let Some(p) = app.windows.previews.iter_mut().find(|p| p.id == id) {
        p.pane = pane;
    }
    app.windows.current = current;
    app.windows.main_active = main_active;
    if let Some(index) = before.and_then(|uid| app.session.docs.iter().position(|d| d.uid == uid)) {
        app.session.active = index;
    }
    app.observe_presentation();
    Some(result)
}

pub(super) fn draw(app: &mut FigApp, id: u64, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if ctx.input(|i| i.viewport().close_requested()) {
        close(app, &ctx, id);
        return;
    }
    let mut keep = false;
    let drawn = as_window(app, id, |app| {
        app.ui_context = Some(ctx.clone());
        if ctx.input(|i| i.viewport().focused == Some(true)) {
            app.windows.focused = id;
        }
        // Root logic owns autosave/live/control admission. Only this viewport's setup, player,
        // captures and input receipts run here; there is no editor frame or global tick.
        guard(app, &ctx, |app| {
            app.release_replaced_input_resources(&ctx);
            app.viewport_setup(&ctx);
            crate::theme::set_doc_profile(&ctx, app.session.doc().map_or(figcraft_color::ColorProfile::Srgb, |d| d.color_profile));
            crate::control::drain(app, &ctx);
            keep = present::show_detached(app, ui, id);
        });
        app.observe_ime(&ctx, false);
        app.end_input_frame(&ctx);
    });
    if drawn.is_none() || !keep {
        close(app, &ctx, id);
        return;
    }
    if let Some(p) = app.windows.previews.iter_mut().find(|p| p.id == id) {
        p.geo.track(&ctx);
    }
}

pub(super) fn list(app: &FigApp) -> Vec<Value> {
    app.windows.previews.iter().map(|p| {
        let source=app.session.docs.iter().find(|d|d.uid==p.uid);
        let mut out=json!({"id":p.id,"kind":"preview","main":false,"sourceDocument":p.uid,"generation":p.id,"title":format!("{} – {}",tr("Prototype preview"),source.map(|d|d.doc.name.as_str()).unwrap_or("")),"active":source.map(|d|d.doc.name.as_str()),"files":[],"focused":p.id==app.windows.focused});
        if let (Some(out),Value::Object(geo))=(out.as_object_mut(),p.geo.json(&app.monitors)) { out.extend(geo); }
        out
    }).collect()
}
