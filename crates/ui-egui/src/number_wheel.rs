//! Focused field ownership and optional numeric wheel edits. The wheel uses the widgets' ordinary
//! Set/relative edit paths and groups its steps into one undo interaction.
use std::sync::Arc;
use std::time::Duration;

use egui::{Context, Event, Id, Modifiers, MouseWheelUnit, Rect, Ui};
use figcraft_doc::{Document, NodeId};

use crate::FigApp;

const IDLE: f64 = 0.4;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    document: u64,
    page: NodeId,
    selection: Vec<NodeId>,
}

#[derive(Clone)]
struct EditorOwner {
    field: Id,
    target: Option<Target>,
}

#[derive(Clone)]
struct Tail {
    until: f64,
    horizontal: bool,
}

#[derive(Clone)]
struct Settings {
    enabled: bool,
    target: Option<Target>,
}

#[derive(Clone)]
struct Gesture {
    field: Id,
    rect: Rect,
    modifiers: Modifiers,
    target: Option<Target>,
    fraction: f64,
    last: f64,
    consumed: u64,
    applying: Option<u64>,
    stepped: bool,
    applied: bool,
    /// Commit only the interaction this wheel actually started, never a text-edit gesture.
    snapshot: Option<Arc<Document>>,
}

fn key(ctx: &Context, name: &'static str) -> Id {
    Id::unique((name, ctx.viewport_id()))
}

fn target(app: &FigApp) -> Option<Target> {
    app.session.ds().ok().map(|d| Target { document: d.uid, page: d.page, selection: d.selection.clone() })
}

fn finish(app: &mut FigApp, ctx: &Context) {
    let id = key(ctx, "number-wheel-gesture");
    let state = ctx.data_mut(|d| {
        let state = d.get_temp::<Gesture>(id);
        d.remove::<Gesture>(id);
        state
    });
    let Some(state) = state else { return };
    let (Some(owner), Some(snapshot)) = (state.target, state.snapshot) else { return };
    let Some(index) = app.session.docs.iter().position(|d| d.uid == owner.document) else { return };
    let owns = app.session.docs.get(index).and_then(|d| d.interaction.as_ref()).is_some_and(|i| Arc::ptr_eq(&i.doc, &snapshot));
    if owns {
        let active = app.session.active;
        app.session.active = index;
        let _ = app.session.commit_interaction();
        app.session.active = active;
    }
}

fn clear_editor(ctx: &Context, field: Id) {
    ctx.data_mut(|d| {
        d.remove::<String>(field.with("editing"));
        d.remove::<String>(field.with("kept"));
        d.remove::<bool>(field.with("selectall"));
        d.remove::<bool>(field.with("wheel_refresh"));
    });
    ctx.memory_mut(|m| {
        m.surrender_focus(field.with("te"));
        m.surrender_focus(field.with("field"));
    });
}

/// Refresh the owner before fields draw. The layers panel can select a new layer in this pass.
pub(crate) fn sync_editor(app: &FigApp, ctx: &Context) {
    let settings = Settings { enabled: app.session.prefs.number_wheel, target: target(app) };
    set_editor_target(ctx, &settings);
}

fn set_editor_target(ctx: &Context, settings: &Settings) {
    let editor_key = key(ctx, "field-editor-owner");
    if let Some(editor) = ctx.data(|d| d.get_temp::<EditorOwner>(editor_key))
        && editor.target != settings.target
    {
        clear_editor(ctx, editor.field);
        ctx.data_mut(|d| d.remove::<EditorOwner>(editor_key));
    }
    let settings_key = key(ctx, "number-wheel-settings");
    ctx.data_mut(|d| d.insert_temp(settings_key, settings.clone()));
}

/// Shared sidebar field IDs follow the selected layers. Popups and windows own their editors'
/// lifetimes independently, such as the JPEG quality editor bound to an export row.
pub(crate) fn begin_editor(ui: &Ui, field: Id) {
    let container = ui.stack().iter().filter_map(|entry| entry.kind()).find(|kind| kind.is_panel() || kind.is_area());
    if !matches!(container, Some(egui::UiKind::LeftPanel | egui::UiKind::RightPanel)) {
        return;
    }
    let ctx = ui.ctx();
    let settings_key = key(ctx, "number-wheel-settings");
    let editor_key = key(ctx, "field-editor-owner");
    ctx.data_mut(|d| {
        if let Some(settings) = d.get_temp::<Settings>(settings_key) {
            d.insert_temp(editor_key, EditorOwner { field, target: settings.target });
        }
    });
}

/// End on another input, leaving/focus loss, owner changes or idle. Called before controls and
/// again after tab navigation, so a shared field id cannot carry a gesture to another document.
pub(crate) fn sync(app: &mut FigApp, ctx: &Context) {
    let id = key(ctx, "number-wheel-gesture");
    let state = ctx.data(|d| d.get_temp::<Gesture>(id));
    let settings = Settings { enabled: app.session.prefs.number_wheel, target: target(app) };
    set_editor_target(ctx, &settings);
    // Agent inspection must not split a human wheel gesture; commands/input still close it.
    let control_edit = app.control_queue.iter().any(|r| {
        !matches!(
            r.method.as_str(),
            "events.poll" | "engine.commands" | "document.inspect" | "node.get" | "window.list" | "ui.inspect" | "ui.render" | "ui.screenshot"
        )
    });
    if let Some(state) = state {
        let (now, modifiers, position, boundary, window_focused) = ctx.input(|i| {
            let boundary = i
                .events
                .iter()
                .any(|e| matches!(e, Event::PointerButton { .. } | Event::Key { .. } | Event::Text(_) | Event::Paste(_) | Event::Cut | Event::Copy));
            (i.time, i.modifiers, i.pointer.hover_pos(), boundary, i.focused)
        });
        let focused = ctx.memory(|m| m.has_focus(state.field.with("field")) || m.has_focus(state.field.with("te")));
        let owned = state.snapshot.as_ref().is_none_or(|snapshot| {
            app.session
                .docs
                .iter()
                .find(|d| Some(d.uid) == state.target.as_ref().map(|t| t.document))
                .and_then(|d| d.interaction.as_ref())
                .is_some_and(|i| Arc::ptr_eq(&i.doc, snapshot))
        });
        if !settings.enabled
            || settings.target != state.target
            || !owned
            || !focused
            || !window_focused
            || boundary
            || control_edit
            || modifiers != state.modifiers
            || position.is_none_or(|p| !state.rect.contains(p))
            || now - state.last >= IDLE
        {
            if settings.target != state.target || !owned {
                clear_editor(ctx, state.field);
            }
            finish(app, ctx);
        } else {
            // A scheduled repaint can arrive just before the idle deadline; rearm its remainder.
            let remaining = (IDLE - (now - state.last)).clamp(0.001, IDLE);
            if remaining.is_finite() {
                ctx.request_repaint_after(Duration::from_secs_f64(remaining));
            }
        }
    }
    // A consumed numeric notch must not scroll a panel later through egui's smoothing tail.
    // A fresh wheel event elsewhere ends suppression and follows the normal canvas/panel path.
    let tail = key(ctx, "number-wheel-tail");
    if let Some(suppression) = ctx.data(|d| d.get_temp::<Tail>(tail)) {
        let (now, raw) = ctx.input(|i| (i.time, i.events.iter().any(|e| matches!(e, Event::MouseWheel { .. }))));
        if now >= suppression.until || raw {
            ctx.data_mut(|d| d.remove::<Tail>(tail));
        } else {
            ctx.input_mut(|i| {
                i.smooth_scroll_delta.y = 0.0;
                if suppression.horizontal {
                    i.smooth_scroll_delta.x = 0.0;
                }
            });
        }
    }
}

pub(crate) fn end_frame(ctx: &Context) {
    let id = key(ctx, "number-wheel-gesture");
    if let Some(mut state) = ctx.data(|d| d.get_temp::<Gesture>(id)) {
        state.applying = None;
        ctx.data_mut(|d| d.insert_temp(id, state));
    }
}

pub(crate) fn applying(app: &FigApp) -> bool {
    app.ui_context.as_ref().is_some_and(|ctx| {
        let id = key(ctx, "number-wheel-gesture");
        ctx.data(|d| d.get_temp::<Gesture>(id)).is_some_and(|g| g.applying == Some(ctx.cumulative_frame_nr()))
    })
}

/// Called by the normal command path and mixed field adapter, immediately before the real edit.
/// An unrelated command closes the wheel first; numeric edits in this frame share its snapshot.
pub(crate) fn before_edit(app: &mut FigApp) {
    let Some(ctx) = app.ui_context.clone() else { return };
    if !applying(app) {
        finish(app, &ctx);
        return;
    }
    let id = key(&ctx, "number-wheel-gesture");
    let Some(mut state) = ctx.data(|d| d.get_temp::<Gesture>(id)) else { return };
    if state.applied || state.target != target(app) {
        finish(app, &ctx);
        return;
    }
    // One returned widget edit authorizes one command. A later command in this same pass
    // closes the gesture, so it cannot be folded into the numeric undo entry.
    state.applied = true;
    ctx.data_mut(|d| d.insert_temp(id, state.clone()));
    if state.snapshot.is_none() && app.session.ds().is_ok_and(|d| d.interaction.is_none()) {
        let snapshot = app.session.ds().ok().map(|d| d.doc.clone());
        if app.session.begin_interaction("Edit number").is_ok() {
            state.snapshot = snapshot;
            ctx.data_mut(|d| d.insert_temp(id, state));
        }
    }
}

/// Consume vertical raw wheel input only over a focused field. Whole line units become steps;
/// point input accumulates at the current egui line-scroll distance. Raw event direction already
/// incorporates the operating system's scroll direction. Never use smoothed deltas as edits.
pub(crate) fn steps(ui: &Ui, field: Id, rect: Rect, focused: bool) -> Option<(f64, Modifiers, bool)> {
    let ctx = ui.ctx();
    let settings_key = key(ctx, "number-wheel-settings");
    let settings = ctx.data(|d| d.get_temp::<Settings>(settings_key))?;
    if !settings.enabled || !focused || !ui.is_enabled() || !ui.rect_contains_pointer(rect) {
        return None;
    }
    let (now, modifiers, window_focused, busy) = ui.input(|i| {
        let busy = i.pointer.any_down()
            || i.events
                .iter()
                .any(|e| matches!(e, Event::PointerButton { .. } | Event::Key { .. } | Event::Text(_) | Event::Paste(_) | Event::Cut | Event::Copy));
        (i.time, i.modifiers, i.focused, busy)
    });
    // Cmd/Ctrl wheel retains the integration's zoom gesture.
    if !window_focused || busy || modifiers.command || modifiers.ctrl {
        return None;
    }
    let id = key(ctx, "number-wheel-gesture");
    let frame = ctx.cumulative_frame_nr();
    let old = ctx.data(|d| d.get_temp::<Gesture>(id));
    if old.as_ref().is_some_and(|g| g.consumed == frame) {
        return None;
    }
    let distance = ctx.options(|o| o.input_options.line_scroll_speed).max(1.0);
    let mut delta = 0.0;
    let mut consumed = false;
    ctx.input_mut(|i| {
        i.events.retain_mut(|e| {
            if let Event::MouseWheel { unit, delta: motion, .. } = e
                && motion.y.is_finite()
                && motion.y != 0.0
            {
                let units = match unit {
                    MouseWheelUnit::Point => motion.y / distance,
                    MouseWheelUnit::Line | MouseWheelUnit::Page => motion.y,
                };
                delta += f64::from(units);
                consumed = true;
                motion.y = 0.0;
                motion.x != 0.0
            } else {
                true
            }
        });
        if consumed {
            i.smooth_scroll_delta.y = 0.0;
            if modifiers.shift {
                i.smooth_scroll_delta.x = 0.0;
            }
        }
    });
    if !consumed {
        return None;
    }
    let mut state = old.filter(|g| g.field == field && g.target == settings.target && g.modifiers == modifiers).unwrap_or(Gesture {
        field,
        rect,
        modifiers,
        target: settings.target,
        fraction: 0.0,
        last: now,
        consumed: frame,
        applying: None,
        stepped: false,
        applied: false,
        snapshot: None,
    });
    state.rect = rect;
    state.last = now;
    state.consumed = frame;
    state.fraction = (state.fraction + delta).clamp(-10_000.0, 10_000.0);
    let whole = state.fraction.trunc();
    state.fraction -= whole;
    let continuing = state.stepped;
    state.stepped |= whole != 0.0;
    state.applied = false;
    state.applying = (whole != 0.0).then_some(frame);
    let editor_key = key(ctx, "field-editor-owner");
    let tail_key = key(ctx, "number-wheel-tail");
    ctx.data_mut(|d| {
        d.insert_temp(editor_key, EditorOwner { field, target: state.target.clone() });
        d.insert_temp(id, state);
        d.insert_temp(tail_key, Tail { until: now + IDLE, horizontal: modifiers.shift });
    });
    ctx.request_repaint_after(Duration::from_secs_f64(IDLE));
    (whole != 0.0).then_some((whole, modifiers, continuing))
}
