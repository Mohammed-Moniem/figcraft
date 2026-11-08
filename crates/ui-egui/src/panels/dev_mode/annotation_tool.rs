//! A canvas-only draft. The document changes only after the existing annotation.add succeeds.

use std::sync::Arc;

use egui::{Context, Event, Id, ImeEvent, Key, KeyboardShortcut, Modifiers, ViewportId, pos2};
use figcraft_doc::{Annotation, Node, NodeId};
use figcraft_engine::{ToolId, annotations};
use serde_json::json;

use crate::i18n::tr;
use crate::{FigApp, RightTab};

#[derive(Default)]
pub(crate) struct Composer {
    draft: Option<Draft>,
    retired: Option<Retired>,
    serial: u64,
    popup_frame: Option<(ViewportId, u64)>,
    last_focus: Option<Id>,
    popup_open_last_frame: bool,
}

struct Retired {
    id: Id,
    submit: Option<Id>,
    popups: [Option<Id>; 2],
    controls: [Option<Id>; 3],
}

struct Draft {
    window: u64,
    viewport: ViewportId,
    presentation: (u64, u64),
    page: NodeId,
    target: Arc<Node>,
    id: Id,
    annotation: Annotation,
    focus: bool,
    rendered: bool,
    composing: bool,
    submit_id: Option<Id>,
    popups: [Option<Id>; 2],
    controls: [Option<Id>; 3],
}

// These are existing editor/dialog owners. Opening one retires this canvas draft, without
// changing how that UI handles its keys or commits its own values.
fn another_editor(app: &FigApp) -> bool {
    let ui = &app.ui;
    ui.preferences
        || ui.shortcuts_panel
        || ui.about
        || ui.color_profile.is_some()
        || ui.menu_input.is_some()
        || ui.find.is_some()
        || ui.batch_rename.is_some()
        || ui.import_report.is_some()
        || ui.missing_fonts.is_some()
        || ui.confirm_close
        || ui.export_preview
        || ui.motion.export.is_some()
        || ui.print.is_here(app.windows.current)
        || ui.mcp_window
        || ui.variables.is_some()
        || ui.picker.is_some()
        || ui.layout_settings.is_some()
        || ui.stroke_settings.is_some()
        || ui.preview.is_some()
        || ui.home
        || ui.icon_gallery
        || app.updates_rt.manual_window_open
}

fn menu_open(app: &FigApp) -> bool {
    app.ui.main_menu || app.ui.palette.is_some() || app.ui.context_menu.is_some() || app.ui.tool_flyout.is_some()
}

fn picker_open(ctx: &Context, draft: &Draft) -> bool {
    draft.popups.iter().flatten().any(|id| egui::Popup::is_id_open(ctx, *id))
}

pub(crate) fn end_frame(app: &mut FigApp, ctx: &Context) {
    app.ui.dev.annotation_tool.last_focus = ctx.memory(|memory| memory.focused());
    app.ui.dev.annotation_tool.popup_open_last_frame = app.ui.dev.annotation_tool.draft.as_ref().is_some_and(|draft| picker_open(ctx, draft));
}

fn valid(app: &mut FigApp, draft: &Draft) -> bool {
    let window = app.windows.current;
    if window != draft.window
        || app.windows.presentation(&app.session, window) != draft.presentation
        || app.ui.present.is_some()
        || another_editor(app)
        || !matches!(app.ui.right_tab, RightTab::Design | RightTab::Inspect)
        || app.session.tool.tool != ToolId::Annotation
        || !annotations::can_start(&app.session, draft.target.id)
    {
        return false;
    }
    app.session.ds().is_ok_and(|d| d.page == draft.page && d.doc.node_arc(draft.target.id).is_some_and(|node| Arc::ptr_eq(node, &draft.target)))
}

fn retire(app: &mut FigApp, draft: Draft) {
    if draft.rendered {
        app.ui.dev.annotation_tool.retired = Some(Retired { id: draft.id, submit: draft.submit_id, popups: draft.popups, controls: draft.controls });
    }
}

/// Called at every observed command boundary, including A→B→A presentations.
pub(crate) fn revalidate(app: &mut FigApp) {
    if let Some(draft) = app.ui.dev.annotation_tool.draft.take() {
        if valid(app, &draft) {
            app.ui.dev.annotation_tool.draft = Some(draft);
        } else {
            retire(app, draft);
        }
    }
}

pub(crate) fn start(app: &mut FigApp, target: NodeId) {
    if app.session.tool.tool != ToolId::Annotation || !annotations::can_start(&app.session, target) {
        return;
    }
    if let Some(old) = app.ui.dev.annotation_tool.draft.take() {
        retire(app, old);
    }
    let window = app.windows.current;
    let presentation = app.windows.presentation(&app.session, window);
    let Ok(ds) = app.session.ds() else { return };
    let Some(target) = ds.doc.node_arc(target).cloned() else { return };
    let page = ds.page;
    let state = &mut app.ui.dev.annotation_tool;
    state.serial = state.serial.wrapping_add(1);
    state.draft = Some(Draft {
        window,
        viewport: crate::windows::viewport(window),
        presentation,
        page,
        target,
        id: Id::unique(("annotation_draft", window, presentation, state.serial)),
        annotation: Annotation { label: String::new(), properties: Vec::new(), category: None },
        focus: true,
        rendered: false,
        composing: false,
        submit_id: None,
        popups: [None; 2],
        controls: [None; 3],
    });
}

fn cleanup(app: &mut FigApp, ctx: &Context) {
    if let Some(retired) = app.ui.dev.annotation_tool.retired.take() {
        let note = retired.id.with("note");
        let owned_popup = retired.popups.iter().flatten().any(|id| egui::Popup::is_id_open(ctx, *id));
        for id in retired.popups.iter().flatten() {
            egui::Popup::close_id(ctx, *id);
        }
        ctx.memory_mut(|memory| {
            let focused = memory
                .focused()
                .filter(|f| *f == note || Some(*f) == retired.submit || retired.controls.iter().flatten().any(|id| *id == *f) || owned_popup);
            if let Some(focused) = focused {
                memory.surrender_focus(focused);
            }
        });
        ctx.data_mut(|data| data.remove::<egui::text_edit::TextEditState>(note));
    }
}

/// Run before navigation and again before painting: invalid owners never see this batch's keys.
pub(crate) fn sync(app: &mut FigApp, ctx: &Context) {
    let frame = (ctx.viewport_id(), ctx.cumulative_frame_nr());
    let picker_is_open = app.ui.dev.annotation_tool.draft.as_ref().is_some_and(|draft| picker_open(ctx, draft));
    if picker_is_open || app.ui.dev.annotation_tool.popup_open_last_frame {
        app.ui.dev.annotation_tool.popup_frame = Some(frame);
    }
    revalidate(app);
    let focused = ctx.input(|i| i.focused && !i.events.iter().any(|e| matches!(e, Event::WindowFocused(false))));
    let cancel = app.ui.dev.annotation_tool.draft.as_ref().is_some_and(|d| d.viewport != ctx.viewport_id() || !focused);
    let owns_focus = app.ui.dev.annotation_tool.draft.as_ref().is_some_and(|d| {
        app.ui
            .dev
            .annotation_tool
            .last_focus
            .is_none_or(|id| id == d.id.with("note") || Some(id) == d.submit_id || d.controls.iter().flatten().any(|control| *control == id))
    });
    let escape = owns_focus
        && !menu_open(app)
        && app.ui.dev.annotation_tool.draft.is_some()
        && app.ui.dev.annotation_tool.popup_frame != Some(frame)
        && !egui::Popup::is_any_open(ctx)
        && consume_plain(ctx, Key::Escape);
    if (cancel || escape)
        && let Some(draft) = app.ui.dev.annotation_tool.draft.take()
    {
        retire(app, draft);
    }
    cleanup(app, ctx);
    if app.ui.dev.annotation_tool.draft.is_some() {
        finish_popup_keys(ctx, app.ui.dev.annotation_tool.popup_frame == Some(frame), false);
    }
}

// consume_key deliberately ignores extra Shift/Alt; note submission requires exact modifiers.
fn consume_plain(ctx: &Context, key: Key) -> bool {
    ctx.input_mut(|input| {
        let mut used = false;
        input.events.retain(|event| {
            let ours = matches!(event, Event::Key { key: pressed_key, pressed: true, modifiers, .. } if *pressed_key == key && *modifiers == Modifiers::NONE);
            used |= ours;
            !ours
        });
        used
    })
}

// egui may close a popup before this pass, or while its rows draw, without consuming
// its key. A completed picker action must not replay as a canvas Escape or Enter.
fn finish_popup_keys(ctx: &Context, popup_owned: bool, after_rows: bool) {
    if !popup_owned {
        return;
    }
    if after_rows {
        // The picker already handled this batch. Leave no events for the canvas dispatcher,
        // including modifiers that could otherwise keep its earlier paste-image flag live.
        // Raw events and derived pointer/key state remain available to the input owner.
        ctx.input_mut(|input| input.events.clear());
    } else if !egui::Popup::is_any_open(ctx) {
        consume_plain(ctx, Key::Escape);
        consume_plain(ctx, Key::Enter);
    }
}

fn keys(ctx: &Context, draft: &mut Draft, popup_owned: bool) -> bool {
    let before_ime = draft.composing;
    let note_owns_input = ctx.memory(|memory| memory.has_focus(draft.id.with("note")));
    let mut ime_batch = false;
    ctx.input_mut(|input| {
        for event in &mut input.events {
            if let Event::Ime(ime) = event {
                ime_batch = true;
                if note_owns_input {
                    match ime {
                        ImeEvent::Preedit { text, .. } => draft.composing = !text.is_empty(),
                        ImeEvent::Commit(_) => draft.composing = false,
                        #[expect(deprecated, reason = "Native integrations can still send the pinned cancellation event")]
                        ImeEvent::Disabled => {
                            // The pinned TextEdit ignores Disabled. Its supported empty Preedit
                            // removes the uncommitted candidate while preserving its cursor state.
                            draft.composing = false;
                            *ime = ImeEvent::Preedit { text: String::new(), active_range_chars: None };
                        }
                        _ => {}
                    }
                }
            }
        }
    });
    if popup_owned {
        return false;
    }
    let owns_enter = ctx.memory(|m| m.focused().is_some_and(|id| id == draft.id.with("note") || Some(id) == draft.submit_id));
    // A focused submit button must not process an IME-confirming Enter itself.
    if ctx.memory(|m| m.focused().is_some_and(|id| Some(id) == draft.submit_id)) && (before_ime || draft.composing || ime_batch) {
        consume_plain(ctx, Key::Enter);
        return false;
    }
    owns_enter && !before_ime && !draft.composing && !ime_batch && !egui::Popup::is_any_open(ctx) && consume_plain(ctx, Key::Enter)
}

fn pickers(ui: &mut egui::Ui, doc: &figcraft_doc::Document, draft: &mut Draft) {
    let current = annotations::category_of(doc, &draft.annotation);
    let label = current.map_or(tr("No category"), |category| category.name.as_str());
    let response = ui.push_id(draft.id.with("category"), |ui| crate::widgets::dropdown(ui, label, ui.available_width(), None)).inner;
    draft.controls[0] = Some(response.id);
    draft.popups[0] = Some(egui::Popup::default_response_id(&response));
    crate::a11y::combo(&response, tr("Annotation category"), label, crate::a11y::menu_open(&response));
    crate::right::menu_popup(&response).show(|ui| {
        if crate::right::menu_check(ui, current.is_none(), tr("No category"), "") {
            draft.annotation.category = None;
            ui.close();
        }
        for category in annotations::categories(doc) {
            if super::category_item(ui, category, current.is_some_and(|c| c.id == category.id)) {
                draft.annotation.category = Some(category.id.clone());
                ui.close();
            }
        }
    });
    let sections = figcraft_engine::devinspect::properties(doc, draft.target.id);
    let response = ui.push_id(draft.id.with("properties"), |ui| crate::widgets::dropdown(ui, tr("Pin a property"), ui.available_width(), None)).inner;
    draft.controls[1] = Some(response.id);
    draft.popups[1] = Some(egui::Popup::default_response_id(&response));
    crate::right::menu_popup(&response).show(|ui| {
        for property in annotations::PINNABLE {
            let on = draft.annotation.properties.iter().any(|p| p == property.id);
            if (on || annotations::applies(&sections, property)) && crate::right::menu_check(ui, on, tr(property.label), "") {
                if on {
                    draft.annotation.properties.retain(|p| p != property.id);
                } else {
                    draft.annotation.properties.push(property.id.to_owned());
                }
                ui.close();
            }
        }
    });
}

pub(crate) fn show(app: &mut FigApp, ctx: &Context) {
    sync(app, ctx);
    if menu_open(app) {
        return;
    }
    let popup_owned = app.ui.dev.annotation_tool.popup_frame == Some((ctx.viewport_id(), ctx.cumulative_frame_nr()));
    let Some(mut draft) = app.ui.dev.annotation_tool.draft.take() else { return };
    if draft.composing && !ctx.memory(|memory| memory.has_focus(draft.id.with("note"))) {
        // An uncommitted composition cannot transfer to another widget.
        retire(app, draft);
        cleanup(app, ctx);
        return;
    }
    let Ok(doc) = app.session.doc().cloned() else { return };
    let mut submit = keys(ctx, &mut draft, popup_owned);
    let mut cancel = false;
    let canvas = app.canvas.rect;
    let at = doc.world_bounds(draft.target.id).map(|r| app.to_screen(r.x1, r.y0)).unwrap_or(canvas.center());
    let x = (at.x + 24.0).clamp(canvas.left() + 8.0, (canvas.right() - 312.0).max(canvas.left() + 8.0));
    let y = at.y.clamp(canvas.top() + 8.0, (canvas.bottom() - 220.0).max(canvas.top() + 8.0));
    egui::Window::new(tr("Add annotation"))
        .id(draft.id)
        .fixed_pos(pos2(x, y))
        .default_width(296.0)
        .max_height((canvas.height() - 24.0).max(100.0))
        .vscroll(true)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(&draft.target.name);
            let response = ui.add(
                egui::TextEdit::multiline(&mut draft.annotation.label)
                    .id(draft.id.with("note"))
                    .hint_text(tr("Annotation note"))
                    .char_limit(annotations::MAX_LABEL)
                    .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))
                    .desired_rows(3)
                    .desired_width(280.0),
            );
            crate::a11y::text(&response, tr("Annotation note"), &draft.annotation.label);
            if std::mem::take(&mut draft.focus) {
                response.request_focus();
            }
            pickers(ui, &doc, &mut draft);
            let picker_owns_batch = popup_owned || picker_open(ctx, &draft);
            if picker_owns_batch {
                submit = false;
            }
            finish_popup_keys(ctx, picker_owns_batch, true);
            for property in &draft.annotation.properties {
                if let Some(pin) = annotations::pinnable(property) {
                    ui.label(tr(pin.label));
                }
            }
            let enabled = !draft.annotation.label.trim().is_empty() || !draft.annotation.properties.is_empty();
            ui.push_id(draft.id.with("actions"), |ui| {
                ui.horizontal(|ui| {
                    let response =
                        ui.push_id("submit", |ui| ui.add_enabled(enabled && !draft.composing, egui::Button::new(tr("Add annotation")))).inner;
                    draft.submit_id = Some(response.id);
                    submit |= response.clicked();
                    let response = ui.push_id("cancel", |ui| ui.button(tr("Cancel"))).inner;
                    draft.controls[2] = Some(response.id);
                    cancel = response.clicked();
                });
            });
            submit &= enabled;
        });
    draft.rendered = true;
    if cancel || !valid(app, &draft) {
        retire(app, draft);
    } else if submit {
        let params = json!({"id": draft.target.id, "label": draft.annotation.label, "properties": draft.annotation.properties, "category": draft.annotation.category});
        if app.run("annotation.add", params).is_some() {
            retire(app, draft);
        } else {
            app.ui.dev.annotation_tool.draft = Some(draft);
        }
    } else {
        app.ui.dev.annotation_tool.draft = Some(draft);
    }
    cleanup(app, ctx);
}
