//! Help ▸ Keyboard shortcuts: a panel along the bottom of the window with a tab per group of the
//! keymap (Essential, Tools, View…), each command's keys as keycaps for this platform, and a search
//! across every group.

use egui::{Align2, Context, CornerRadius, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_engine::keymap::{self, GESTURES, Group, KEYS, Platform};
use serde_json::json;

use crate::FigApp;
use crate::i18n::{self, tr};
use crate::theme::{Tokens, regular};

/// Most keys a row shows (a command's first bindings; the rest stay in the menus' alternates).
const MAX_KEYS: usize = 2;
const ROW_H: f32 = 26.0;
const COLUMNS: usize = 3;
/// Rows a column shows before the panel scrolls.
const MAX_ROWS: usize = 9;

/// One row: the action and its keys as written on this platform.
struct Row {
    id: Option<&'static str>,
    customized: bool,
    label: std::borrow::Cow<'static, str>,
    keys: Vec<String>,
}

/// Rows per column of the fullest tab, so switching tabs doesn't resize the panel.
fn tallest_column() -> usize {
    let count = |g: Group| KEYS.iter().filter(|b| b.group == g).count() + GESTURES.iter().filter(|x| x.0 == g).count();
    Group::ALL.iter().map(|g| count(*g).div_ceil(COLUMNS)).max().unwrap_or(1)
}

fn rows(app: &FigApp, group: Option<Group>, query: &str, platform: Platform) -> Vec<Row> {
    let q = query.trim().to_lowercase();
    let wanted = |g: Group, label: &str, english: &str, id: &str| {
        group.is_none_or(|x| x == g)
            && (q.is_empty() || label.to_lowercase().contains(&q) || english.to_lowercase().contains(&q) || id.to_lowercase().contains(&q))
    };
    let defaults = KEYS.iter().map(|b| b.id);
    let menu = figcraft_engine::catalog::items().filter(|m| !m.id.is_empty() && m.label != "---").map(|m| m.id);
    let commands = figcraft_engine::all_commands().iter().filter(|c| !figcraft_engine::cmd::needs_params(c.id)).map(|c| c.id);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for id in defaults.chain(menu).chain(commands).filter(|id| seen.insert(*id)) {
        if !crate::menus::available(id) {
            continue;
        }
        let Some(b) = keymap::command_binding(id) else { continue };
        let label = i18n::command(b.id, b.title());
        if !wanted(b.group, &label, b.title(), b.id) {
            continue;
        }
        let mut keys = Vec::new();
        for k in app.session.prefs.shortcuts.keys(b.id, platform) {
            let shown = keymap::display(k, platform);
            if keys.len() < MAX_KEYS && !keys.contains(&shown) {
                keys.push(shown);
            }
        }
        out.push(Row { id: Some(b.id), customized: app.session.prefs.shortcuts.overridden(b.id, platform), label, keys });
    }
    out.extend(GESTURES.iter().filter(|(g, label, ..)| wanted(*g, tr(label), label, "")).map(|(_, label, mac, other)| Row {
        id: None,
        customized: false,
        label: tr(label).into(),
        keys: vec![tr(if platform == Platform::Mac { mac } else { other }).to_string()],
    }));
    out
}

/// Paint `text` as a keycap ending at `right`; returns its left edge.
fn keycap(ui: &Ui, t: &Tokens, text: &str, right: f32, cy: f32) -> f32 {
    let galley = ui.painter().layout_no_wrap(text.to_string(), regular(11.0), t.text);
    let w = galley.size().x + 10.0;
    let r = egui::Rect::from_min_size(pos2(right - w, cy - 10.0), vec2(w, 20.0));
    ui.painter().rect_filled(r, CornerRadius::same(4), t.field);
    ui.painter().rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, t.border), StrokeKind::Inside);
    ui.painter().galley(pos2(r.min.x + 5.0, cy - galley.size().y / 2.0), galley, t.text);
    r.min.x
}

fn row(ui: &mut Ui, t: &Tokens, row: &Row, width: f32) -> (bool, bool, bool) {
    let (r, resp) = ui.allocate_exact_size(vec2(width, ROW_H), if row.id.is_some_and(keymap::editable) { Sense::click() } else { Sense::hover() });
    crate::a11y::button(&resp, &row.label);
    if row.id == Some("select.none") {
        resp.clone().on_hover_text(tr("Escape is reserved for cancelling interactions"));
    }
    let mut reset = false;
    let mut unbind = false;
    if row.id.is_some_and(keymap::editable) {
        resp.context_menu(|ui| {
            if ui.button(tr("Reset shortcut")).clicked() {
                reset = true;
                ui.close();
            }
            if ui.button(tr("Remove shortcut")).clicked() {
                unbind = true;
                ui.close();
            }
        });
    }
    let clicked = resp.clicked();
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(4), t.hover);
    }
    let cy = r.center().y;
    let mut right = r.max.x - 6.0;
    if row.keys.is_empty() {
        right = keycap(ui, t, tr("Unbound"), right, cy);
    }
    for (i, k) in row.keys.iter().enumerate().rev() {
        right = keycap(ui, t, k, right, cy);
        if i > 0 {
            let or = ui.painter().layout_no_wrap(tr("or").into(), regular(10.0), t.text_tertiary);
            right -= 4.0 + or.size().x;
            ui.painter().galley(pos2(right, cy - or.size().y / 2.0), or, t.text_tertiary);
            right -= 4.0;
        }
    }
    let label_w = (right - r.min.x - 14.0).max(20.0);
    // Measured in the font it's painted in: a long translation ends with "…" before the keycaps and
    // shows in full on hover.
    let label = crate::widgets::elide_font(ui, &row.label, label_w, regular(12.0));
    if label != row.label {
        resp.clone().on_hover_text(&*row.label);
    }
    ui.painter().text(pos2(r.min.x + 6.0, cy), Align2::LEFT_CENTER, label, regular(12.0), if row.customized { t.accent } else { t.text });
    (clicked, reset, unbind)
}

#[derive(Clone, Default)]
struct Recorder {
    edit: Option<Edit>,
    held: Vec<egui::Key>,
    /// Some hosts replace Cmd/Ctrl+C/X/V with a clipboard event, including its key-up. Keep the
    /// modifier's release as the boundary for those events instead of inventing a missing key-up.
    clipboard: bool,
    swallowed: Option<u64>,
}

#[derive(Clone)]
struct Edit {
    id: String,
    /// None restores defaults; Some([]) unbinds. A pending proposal waits for a conflict choice.
    keys: Option<Vec<String>>,
    pending: bool,
    conflicts: Vec<String>,
    rejected: bool,
}

fn recorder_id(ctx: &Context) -> egui::Id {
    recorder_viewport_id(ctx.viewport_id())
}
fn recorder_viewport_id(viewport: egui::ViewportId) -> egui::Id {
    egui::Id::unique(("shortcut_recorder", viewport))
}
fn state(ctx: &Context) -> Recorder {
    let id = recorder_id(ctx);
    ctx.data(|d| d.get_temp(id)).unwrap_or_default()
}
fn save(ctx: &Context, state: Recorder) {
    let id = recorder_id(ctx);
    ctx.data_mut(|d| d.insert_temp(id, state));
}
fn platform_name(p: Platform) -> &'static str {
    if p == Platform::Mac { "mac" } else { "other" }
}

#[cfg(test)]
pub(crate) fn recording(ctx: &Context) -> bool {
    recording_in(ctx, ctx.viewport_id())
}

/// The native menu is sampled from the root frame even when another window owns the keyboard.
pub(crate) fn recording_in(ctx: &Context, viewport: egui::ViewportId) -> bool {
    let state: Recorder = ctx.data(|d| d.get_temp(recorder_viewport_id(viewport))).unwrap_or_default();
    state.edit.is_some() || !state.held.is_empty() || state.clipboard
}

fn cancel(ctx: &Context) {
    let mut state = state(ctx);
    state.edit = None;
    save(ctx, state);
}

/// Also used by keyboard/accessibility activation of a row.
pub(crate) fn begin_capture(app: &mut FigApp, ctx: &Context, id: &str) {
    if !keymap::editable(id) {
        return;
    }
    app.ui.shortcuts_panel = true;
    let mut state = state(ctx);
    state.held = ctx.input(|i| i.keys_down.iter().copied().collect());
    state.edit = Some(Edit { id: id.to_string(), keys: None, pending: false, conflicts: Vec::new(), rejected: false });
    save(ctx, state);
    if let Some(id) = ctx.memory(|m| m.focused()) {
        ctx.memory_mut(|m| m.surrender_focus(id));
    }
    ctx.request_repaint();
}

fn propose(app: &mut FigApp, ctx: &Context, id: &str, keys: Option<Vec<String>>, replace: bool) {
    let mut p = json!({"id": id, "platform": platform_name(crate::shortcuts::platform()), "replace": replace});
    if let Some(keys) = &keys {
        p["keys"] = json!(keys);
    }
    let result = app.run(if keys.is_some() { "pref.shortcut" } else { "pref.resetShortcut" }, p);
    let mut state = state(ctx);
    let conflicts: Vec<String> =
        result.as_ref().and_then(|v| v.get("conflicts")).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    state.edit = if result.is_some() && conflicts.is_empty() {
        None
    } else {
        Some(Edit { id: id.to_string(), keys, pending: result.is_some(), conflicts, rejected: result.is_none() })
    };
    save(ctx, state);
    ctx.request_repaint();
}

/// Run before any field or canvas handler. Swallow the entire batch, including its Text and OS
/// clipboard events, the release and any discarded re-render of the same batch (#2036).
pub(crate) fn capture_input(app: &mut FigApp, ctx: &Context) {
    crate::shortcuts::note_platform(ctx);
    let mut state = state(ctx);
    if !app.ui.shortcuts_panel {
        state.edit = None;
    }
    if !ctx.input(|i| i.modifiers.command || i.modifiers.mac_cmd) {
        state.clipboard = false;
    }
    let frame = ctx.cumulative_frame_nr();
    if state.edit.is_none() && state.held.is_empty() && !state.clipboard && state.swallowed != Some(frame) {
        save(ctx, state);
        return;
    }
    let events = ctx.input(|i| i.events.clone());
    let edit = state.edit.clone().filter(|e| !e.pending);
    let mut candidate = None;
    let mut escape = false;
    for event in &events {
        match event {
            egui::Event::Key { key, pressed, repeat, physical_key, modifiers } => {
                if *pressed && !state.held.contains(key) {
                    state.held.push(*key);
                }
                if !pressed {
                    state.held.retain(|held| held != key);
                }
                if *pressed && *key == egui::Key::Escape {
                    escape = true;
                }
                if *pressed && !repeat && candidate.is_none() {
                    candidate = keymap::chord(crate::shortcuts::platform(), key.name(), crate::shortcuts::key_mods(*key, *physical_key, *modifiers));
                }
            }
            egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) if candidate.is_none() => {
                state.clipboard = true;
                let key = match event {
                    egui::Event::Copy => "C",
                    egui::Event::Cut => "X",
                    _ => "V",
                };
                let m = ctx.input(|i| i.modifiers);
                candidate = keymap::chord(crate::shortcuts::platform(), key, crate::shortcuts::mods(egui::Modifiers { command: true, ..m }));
            }
            egui::Event::WindowFocused(false) => {
                state.held.clear();
                state.clipboard = false;
                escape = true;
            }
            _ => {}
        }
    }
    if escape {
        state.edit = None;
    }
    state.swallowed = Some(frame);
    save(ctx, state);
    let keyboard = |e: &egui::Event| {
        matches!(
            e,
            egui::Event::Key { .. }
                | egui::Event::Text(_)
                | egui::Event::Copy
                | egui::Event::Cut
                | egui::Event::Paste(_)
                | egui::Event::PasteImage(_)
                | egui::Event::Ime(_)
        )
    };
    ctx.input_mut(|i| {
        i.events.retain(|e| !keyboard(e));
        i.raw.events.retain(|e| !keyboard(e));
        i.keys_down.clear();
        // Hosts send modifiers only when they change. Retain their physical state so a
        // clipboard repeat in the next raw batch still belongs to this recording gesture.
    });
    if !escape && let (Some(edit), Some(key)) = (edit, candidate) {
        propose(app, ctx, &edit.id, Some(vec![key]), false);
    }
}

fn recorder_ui(app: &mut FigApp, ctx: &Context, ui: &mut Ui) {
    let Some(edit) = state(ctx).edit else { return };
    ui.separator();
    let title = keymap::command_binding(&edit.id).map_or(edit.id.as_str(), |b| b.title());
    ui.label(i18n::command(&edit.id, title));
    if edit.pending {
        if let Some(keys) = &edit.keys {
            ui.label(keys.iter().map(|k| keymap::display(k, crate::shortcuts::platform())).collect::<Vec<_>>().join(" / "));
        }
        ui.label(tr("This shortcut is already used by:"));
        for id in &edit.conflicts {
            let title = keymap::command_binding(id).map_or(id.as_str(), |b| b.title());
            ui.label(i18n::command(id, title));
        }
        ui.horizontal(|ui| {
            if ui.button(tr("Replace shortcut")).clicked() {
                propose(app, ctx, &edit.id, edit.keys.clone(), true);
            }
            if ui.button(tr("Cancel")).clicked() {
                cancel(ctx);
            }
        });
    } else {
        ui.label(if edit.rejected { tr("This shortcut is unavailable. Choose another key.") } else { tr("Press a shortcut. Escape cancels.") });
        if ui.button(tr("Cancel")).clicked() {
            cancel(ctx);
        }
    }
}

pub fn show(app: &mut FigApp, ctx: &Context) {
    if !app.ui.shortcuts_panel {
        return;
    }
    let t = Tokens::get(ctx);
    let platform = crate::shortcuts::platform();
    let width = (ctx.content_rect().width() - 48.0).clamp(480.0, 1040.0);
    let mut open = true;
    crate::dialogs::window(tr("Keyboard shortcuts"), ctx).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -16.0)).open(&mut open).show(ctx, |ui| {
        ui.set_width(width);
        ui.horizontal(|ui| {
            crate::dialogs::tab_strip(ui, &t, &Group::ALL, |g| tr(g.label()), &mut app.ui.shortcuts_tab);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut app.ui.shortcuts_query).hint_text(tr("Search shortcuts")).desired_width(180.0));
                crate::widgets::select_all_on_focus(ui, &r, &app.ui.shortcuts_query);
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Click a command, then press a shortcut. Right-click to reset or remove."));
            if ui.button(tr("Reset all shortcuts")).clicked() {
                app.run("pref.resetShortcut", json!({"platform": platform_name(platform)}));
                cancel(ctx);
            }
        });
        recorder_ui(app, ctx, ui);
        ui.add_space(10.0);
        let searching = !app.ui.shortcuts_query.trim().is_empty();
        let list = rows(app, (!searching).then_some(app.ui.shortcuts_tab), &app.ui.shortcuts_query, platform);
        let body_h = (ROW_H + 2.0) * tallest_column().min(MAX_ROWS) as f32;
        if list.is_empty() {
            ui.allocate_ui(vec2(width, body_h), |ui| {
                ui.set_min_height(body_h);
                ui.label(egui::RichText::new(tr("No shortcuts match")).color(t.text_secondary).font(regular(12.0)));
            });
            return;
        }
        let gap = 24.0;
        let col_w = (width - gap * (COLUMNS as f32 - 1.0)) / COLUMNS as f32;
        let per = list.len().div_ceil(COLUMNS);
        egui::ScrollArea::vertical().min_scrolled_height(body_h).max_height(body_h).show(ui, |ui| {
            ui.set_min_height(body_h);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(gap, 0.0);
                for chunk in list.chunks(per.max(1)) {
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for r in chunk {
                            let (clicked, reset, unbind) = row(ui, &t, r, col_w);
                            if let Some(id) = r.id {
                                if clicked {
                                    begin_capture(app, ctx, id);
                                }
                                if reset {
                                    propose(app, ctx, id, None, false);
                                }
                                if unbind {
                                    propose(app, ctx, id, Some(Vec::new()), false);
                                }
                            }
                        }
                    });
                }
            });
        });
    });
    if !open {
        app.ui.shortcuts_panel = false;
        cancel(ctx);
    }
}
