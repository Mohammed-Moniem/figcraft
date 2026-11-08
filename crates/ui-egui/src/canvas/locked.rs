//! A locked layer's original canvas control. Targets belong to a document and canvas epoch;
//! the existing command performs every edit, with an explicit false so stale input cannot relock.
use std::sync::Arc;

use egui::{Context, CornerRadius, Id, Pos2, Rect, Stroke, StrokeKind, Ui, vec2};
use figcraft_doc::{Document, NodeId};
use figcraft_engine::ToolId;
use serde_json::json;

use crate::FigApp;
use crate::i18n::trf;
use crate::icons::Icon;
use crate::theme::Tokens;

type Owner = (u64, u64, NodeId);

/// Resizing the viewport changes page projection without editing the document revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Presentation {
    view: [u64; 5],
    canvas: [u32; 4],
}

#[derive(Clone, Copy)]
struct Target {
    owner: Owner,
    epoch: u64,
    id: NodeId,
    revision: u64,
    presentation: Presentation,
}

#[derive(Default)]
pub(super) struct State {
    owner: Option<Owner>,
    epoch: u64,
    hover: Option<Target>,
    context: Option<Target>,
    /// Another widget may replace the global menu inside the same document.
    context_signature: Option<(Pos2, Vec<NodeId>)>,
    checked_doc: Option<Arc<Document>>,
    any_locks: bool,
    #[cfg(test)]
    queries: usize,
}

fn owner(app: &FigApp) -> Option<Owner> {
    app.session.ds().ok().map(|ds| (app.windows.current, ds.uid, ds.page))
}

fn presentation(app: &FigApp) -> Option<Presentation> {
    let view = &app.session.ds().ok()?.view;
    let rect = app.canvas.rect;
    Some(Presentation {
        view: [view.zoom, view.cx, view.cy, view.vw, view.vh].map(f64::to_bits),
        canvas: [rect.min.x, rect.min.y, rect.max.x, rect.max.y].map(f32::to_bits),
    })
}

/// An A→B→A switch must retire A's old response IDs as well as its visible hover.
pub(super) fn sync(app: &mut FigApp) {
    let next = owner(app);
    if app.canvas.unlock.owner != next {
        let context = app.canvas.unlock.context.is_some();
        let epoch = app.canvas.unlock.epoch.wrapping_add(1);
        app.canvas.unlock = State { owner: next, epoch, ..Default::default() };
        if context {
            app.ui.context_menu = None;
        }
    }
}

fn can_unlock(app: &FigApp) -> bool {
    let Ok(ds) = app.session.ds() else { return false };
    ds.preview.is_none()
        && ds.text_edit.is_none()
        && ds.vector_edit.is_none()
        && app.session.live_read_only(ds.uid).is_none()
        && app.ui.right_tab != crate::RightTab::Inspect
        && !app.session.tool.dev_mode
        && !app.session.tool.busy()
        && !app.canvas.dragging
}

fn hover_allowed(app: &FigApp, ctx: &Context) -> bool {
    can_unlock(app)
        && matches!(app.session.tool.tool, ToolId::Move | ToolId::Scale)
        && app.ui.context_menu.is_none()
        && app.ui.palette.is_none()
        && app.ui.picker.is_none()
        && !app.ui.preferences
        && !app.canvas.space_pan
        && !egui::Popup::is_any_open(ctx)
}

fn valid(app: &FigApp, target: Target) -> bool {
    if !can_unlock(app)
        || owner(app) != Some(target.owner)
        || app.canvas.unlock.epoch != target.epoch
        || presentation(app) != Some(target.presentation)
        || !app.windows.exists(target.owner.0)
        || app.windows.of(target.owner.1) != target.owner.0
    {
        return false;
    }
    let Ok(doc) = app.session.doc() else { return false };
    if !app.session.ds().is_ok_and(|d| d.revision == target.revision) {
        return false;
    }
    if doc.page_of(target.id) != Some(target.owner.2) || !doc.node(target.id).is_some_and(|n| n.locked) {
        return false;
    }
    let mut at = target.id;
    for _ in 0..=513 {
        let Some(node) = doc.node(at) else { return false };
        if !node.visible {
            return false;
        }
        let Some(parent) = node.parent else { return true };
        at = parent;
    }
    false
}

fn any_locks(app: &mut FigApp) -> bool {
    let Ok(ds) = app.session.ds() else { return false };
    let doc = ds.doc.clone();
    if app.canvas.unlock.checked_doc.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &doc)) {
        app.canvas.unlock.any_locks = doc.nodes().any(|n| n.locked);
        app.canvas.unlock.checked_doc = Some(doc);
    }
    app.canvas.unlock.any_locks
}

fn find(app: &mut FigApp, pos: Pos2) -> Option<Target> {
    if !pos.is_finite() || !app.canvas.rect.contains(pos) || !can_unlock(app) || !any_locks(app) {
        return None;
    }
    let ds = app.session.ds().ok()?;
    let doc = ds.doc.clone();
    let p = app.to_page(pos);
    let id = app.session.locked_at(&doc, ds.page, p, 2.0 / ds.view.zoom.max(1e-6))?;
    let target = Target { owner: owner(app)?, epoch: app.canvas.unlock.epoch, id, revision: ds.revision, presentation: presentation(app)? };
    #[cfg(test)]
    {
        app.canvas.unlock.queries += 1;
    }
    valid(app, target).then_some(target)
}

fn badge_rect(app: &FigApp, target: Target) -> Option<Rect> {
    let bounds = app.session.doc().ok()?.world_bounds(target.id)?;
    let at = app.to_screen(bounds.x0, bounds.y0);
    let canvas = app.canvas.rect;
    if !at.is_finite() || !canvas.min.is_finite() || !canvas.max.is_finite() || canvas.width() < 28.0 || canvas.height() < 28.0 {
        return None;
    }
    let at = Pos2::new((at.x + 4.0).clamp(canvas.min.x + 4.0, canvas.max.x - 24.0), (at.y + 4.0).clamp(canvas.min.y + 4.0, canvas.max.y - 24.0));
    Some(Rect::from_min_size(at, vec2(20.0, 20.0)))
}

fn label(app: &FigApp, target: Target) -> String {
    let name = app.session.doc().ok().and_then(|d| d.node(target.id)).map_or("", |n| n.name.as_str());
    trf("Unlock {name}", &[("name", &name)])
}

fn unlock(app: &mut FigApp, target: Target) {
    if valid(app, target) {
        app.run("object.toggleLock", json!({"ids":[target.id.0],"locked":false}));
    }
    app.canvas.unlock.hover = None;
    app.canvas.unlock.context = None;
    app.canvas.unlock.context_signature = None;
}

/// Called only after a fresh engine hover; the existing HoverKey gates stationary frames.
pub(super) fn hover(app: &mut FigApp, pos: Pos2) {
    sync(app);
    let Some(ctx) = app.ui_context.clone() else { return };
    if !hover_allowed(app, &ctx) {
        clear_hover(app);
        return;
    }
    // A group/vector may have no painted geometry beneath its upper-left button.
    if let Some(target) = app.canvas.unlock.hover
        && valid(app, target)
        && app.session.ds().is_ok_and(|d| d.revision == target.revision)
        && badge_rect(app, target).is_some_and(|r| r.contains(pos))
    {
        return;
    }
    app.canvas.unlock.hover = find(app, pos);
}

pub(super) fn clear_hover(app: &mut FigApp) {
    app.canvas.unlock.hover = None;
}

/// Interaction is registered after the canvas paints, so this early guard keeps its click from
/// starting an engine drag. The normal icon-button response owns release and keyboard activation.
pub(super) fn blocks_pointer(app: &mut FigApp, ui: &Ui, pos: Pos2) -> bool {
    sync(app);
    hover_allowed(app, ui.ctx())
        && app
            .canvas
            .unlock
            .hover
            .is_some_and(|t| valid(app, t) && badge_rect(app, t).is_some_and(|r| r.contains(pos) && ui.rect_contains_pointer(r)))
}

pub(super) fn show(app: &mut FigApp, ui: &mut Ui) {
    // Web repeats primary press/release for each finger. An acquired canvas navigation
    // gesture, including its last release frame, cannot activate this later-painted button.
    if super::input::touch_navigating(ui.ctx()) || !hover_allowed(app, ui.ctx()) {
        return;
    }
    let Some(target) = app.canvas.unlock.hover.filter(|t| valid(app, *t)) else { return };
    let Some(rect) = badge_rect(app, target) else { return };
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(rect, CornerRadius::same(5), t.panel);
    ui.painter().rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0, t.border), StrokeKind::Inside);
    let id = Id::unique(("canvas_unlock", target.owner, target.epoch, target.id, target.revision, target.presentation));
    if crate::widgets::icon_button_at(ui, id, rect, Icon::Lock, &label(app, target)) {
        unlock(app, target);
    }
}

/// A pointed locked owner has its own small menu; unrelated selected layers are not retargeted.
pub(super) fn open_context(app: &mut FigApp, pos: Pos2) -> bool {
    sync(app);
    clear_context(app);
    let Some(target) = find(app, pos) else { return false };
    app.canvas.unlock.context = Some(target);
    let selected = app.session.selection();
    app.canvas.unlock.context_signature = Some((pos, selected.clone()));
    app.canvas.unlock.hover = None;
    app.ui.context_spelling = None;
    app.ui.context_menu = Some((pos, selected));
    app.ui.context_menu_shown = false;
    true
}

pub(crate) fn validate_context(app: &mut FigApp) {
    sync(app);
    if app.ui.context_menu.is_none() {
        clear_context(app);
    } else if app.canvas.unlock.context.is_some() && app.canvas.unlock.context_signature.as_ref() != app.ui.context_menu.as_ref() {
        // Keep the newly requested normal menu; only retire this feature's old target.
        clear_context(app);
    } else if app.canvas.unlock.context.is_some_and(|t| !valid(app, t)) {
        clear_context(app);
        app.ui.context_menu = None;
    }
}

pub(crate) fn clear_context(app: &mut FigApp) {
    app.canvas.unlock.context = None;
    app.canvas.unlock.context_signature = None;
}

pub(crate) fn menu(app: &mut FigApp, ui: &mut Ui, tokens: &Tokens) -> Option<bool> {
    let target = app.canvas.unlock.context.filter(|t| valid(app, *t))?;
    // The keyboard's lock shortcut acts on the selection, not this pointed owner.
    let clicked = crate::menus::item(ui, &label(app, target), "", true, false, tokens);
    if clicked {
        unlock(app, target);
    }
    Some(clicked)
}

#[cfg(test)]
pub(crate) fn hover_queries(app: &FigApp) -> usize {
    app.canvas.unlock.queries
}
