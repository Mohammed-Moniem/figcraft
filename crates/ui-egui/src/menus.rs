//! The main menu (from the engine's menu catalog), the canvas/layer context menu and the
//! Actions palette (⌘K / ⌘/).

use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_engine::catalog::{MENU, SEP};
use serde_json::json;

use crate::FigApp;
use crate::a11y;
use crate::i18n::{self, tr, tr_noop, trf, trn};
use crate::icons::{self, Icon};
use crate::theme::{Tokens, regular};
use crate::toolbar::pretty_shortcut;

mod clipboard;
mod palette;
mod select_layer;
mod tree;

pub use clipboard::paste;
use clipboard::{figcraft_svg_export, text_layers_text};
use palette::palette;
#[cfg(test)]
pub(crate) use palette::{ActionRow, command_sections, command_sections_for, palette_sections, recent_row, run_action};
pub use palette::{PaletteTab, set_palette, toggle_palette};
pub use select_layer::open_select_layer;
use select_layer::{SELECT_LAYER, select_layer_ask_id, select_layer_menu};
#[cfg(test)]
pub(crate) use tree::{AIM_REST, HOVER_DELAY, heading_for, in_triangle};
use tree::{At, SubmenuOptions, menu_frame, plain_row_hover, set_open, submenu_at};
pub(crate) use tree::{forget_tree, item, separator, submenu};

/// Commands the UI layer implements itself (dialogs and platform actions).
pub const UI_COMMANDS: &[&str] = &[
    "plugin.import",
    "file.open",
    "file.import",
    "file.placeImage",
    "file.save",
    "file.saveAs",
    "file.export",
    "file.exportSvg",
    "edit.copyAsText",
    "edit.copyAsCss",
    "edit.copyAsSvg",
    "edit.copyAsPng",
    "edit.copyLink",
];

/// Dev Mode (the Inspect tab) refuses command `id` from the menus, shortcuts, clipboard, the
/// Pages list, the layers panel, files dropped on the canvas and the Assets panel: it edits the
/// design ([`figcraft_engine::devinspect::dev_mode_allows`]). Says so when it does.
pub fn dev_mode_refuses(app: &mut FigApp, id: &str) -> bool {
    let refused = app.ui.right_tab == crate::RightTab::Inspect && !figcraft_engine::devinspect::dev_mode_allows(id);
    if refused {
        app.toast(tr("Dev Mode only inspects: switch to Design (Shift+D) to edit"));
    }
    refused
}

/// Remember an action run from the palette for its Recents (newest first, capped).
pub fn record_recent(app: &mut FigApp, key: &str) {
    let r = &mut app.session.prefs.recent_actions;
    if r.first().is_some_and(|f| f == key) {
        return;
    }
    r.retain(|k| k != key);
    r.insert(0, key.to_string());
    r.truncate(figcraft_engine::MAX_RECENT_ACTIONS);
}

/// Run a menu, shortcut or palette action by id: actions that need the UI (dialogs, panels, the
/// clipboard) are handled here, the rest go to the engine.
pub fn invoke(app: &mut FigApp, id: &str) {
    if dev_mode_refuses(app, id) {
        return;
    }
    match id {
        // The design review is a window, not a one-shot command: it keeps its report while the
        // user selects layers and fixes what it found.
        "design.audit" | "design.fix" => {
            crate::design_review::open(app);
        }
        // Edit ▸ Delete (and the Delete binding) with an interaction open in the Prototype tab
        // removes that interaction, not the layer (#767).
        "edit.delete" if app.ui_context.clone().is_some_and(|ctx| crate::proto_panel::delete_selected_interaction(app, &ctx)) => {}
        // After a click in the Pages list, while the list is on screen and its file open, with no
        // layers selected, it deletes the current page (#859).
        "edit.delete" if crate::layers::pages_have_keyboard(app) && app.session.selection().is_empty() => {
            if let Ok(page) = app.session.page() {
                crate::layers::delete_page(app, page);
            }
        }
        // Otherwise the list gives up the keyboard: once Delete has taken layers selected since the
        // click (Select all, say), a second Delete must not take the page.
        "edit.delete" if app.ui.pages_focus.is_some() => {
            app.ui.pages_focus = None;
            app.run(id, json!({}));
        }
        "file.open" => app.open_dialog(),
        // From the keyboard or a menu, typing goes on in the new sticky.
        "board.nextSticky" => {
            app.run(id, json!({"edit": true}));
        }
        "help.checkForUpdates" | "app.checkForUpdates" => crate::updates_ui::check(app),
        "plugin.import" => crate::plugins_ui::import_dialog(app),
        "file.import" => app.import_dialog(false),
        "file.placeImage" => app.import_dialog(true),
        "file.save" => app.save(),
        "file.saveAs" => app.save_as_dialog(),
        "file.openRecent" => {
            app.ui.home = true;
            app.ui.home_view.section = crate::home::Section::Recents;
        }
        "file.saveVersion" => crate::versions::start_save(app),
        "branch.create" => crate::versions::start_branch(app),
        "file.export" | "file.exportSvg" => app.ui.export_preview = true,
        "file.exportFramesPdf" => export_frames_pdf(app),
        "edit.copyAsCss" => {
            let ids = app.session.selection();
            if let (Some(first), Ok(doc)) = (ids.first(), app.session.doc()) {
                let css = figcraft_engine::codegen::code_for(doc, *first, figcraft_engine::codegen::CodeLang::Css);
                app.clipboard_set_text(&css);
                app.toast(tr("Copied as CSS"));
            }
        }
        "edit.copyAsSvg" => {
            let ids = app.session.selection();
            if let Some(svg) = app.session.doc().ok().and_then(|d| figcraft_svg_export(d, &ids)) {
                app.clipboard_set_text(&svg);
                app.toast(tr("Copied as SVG"));
            }
        }
        "edit.copyAsPng" => {
            let ids = app.session.selection();
            if let Some((mut pix, profile)) =
                app.session.doc().ok().and_then(|d| Some((figcraft_engine::cmd::file::copy_png(d, &ids)?, d.color_profile)))
            {
                // Clipboard images are read as sRGB.
                figcraft_render::to_srgb(&mut pix, profile);
                let rgba = figcraft_render::to_rgba8(&pix);
                app.clipboard_set_png(pix.width(), pix.height(), &rgba);
                app.toast(tr("Copied as PNG"));
            }
        }
        "edit.copyAsText" => {
            let doc = app.session.doc().ok();
            let text: Vec<String> = app
                .session
                .selection()
                .iter()
                .filter_map(|i| doc.and_then(|d| d.node(*i)).and_then(|n| n.kind.text().map(|t| t.characters.clone())))
                .collect();
            app.clipboard_set_text(&text.join("\n"));
            app.toast(tr("Copied as text"));
        }
        "edit.copyLink" => {
            // Only a selected layer has a link, and the copy goes through the control-channel fence
            // (`clipboard_set_text`): a synthetic gesture never replaces the user's clipboard.
            if let Some(id) = app.session.selection().first() {
                app.clipboard_set_text(&format!("figcraft://node/{}", id.0));
                app.toast(tr("Link copied"));
            }
        }
        "edit.findReplace" => app.ui.find = Some((String::new(), String::new())),
        // The Actions palette invokes commands without parameters. These values are edited in
        // Preferences; sending an empty object would either fail or silently reset a preference.
        "pref.language"
        | "pref.userName"
        | "pref.publishServer"
        | "pref.nudge"
        | "pref.pixelGridSize"
        | "pref.accentColor"
        | "pref.selectionOutlineOpacity" => {
            app.run("app.preferences", json!({}));
        }
        // The file name in the left panel's header turns into its editor, the name selected.
        "file.rename" => {
            app.ui.left_tab = crate::LeftTab::File;
            app.ui.open_left_panel();
            app.ui.rename_file = true;
        }
        "text.link" => app.ui.menu_input = Some(crate::dialogs::MenuInput::Link(String::new())),
        "vector.offset" => app.ui.menu_input = Some(crate::dialogs::MenuInput::Offset(0.0)),
        "object.moveToPage" => app.ui.menu_input = Some(crate::dialogs::MenuInput::MoveToPage),
        "object.batchRename" => {
            let ids = app.session.selection();
            if !ids.is_empty() {
                crate::dialogs::open_batch_rename(app, &ids);
            }
        }
        "tool.set" => app.ui.tool_flyout = Some("move"),
        "page.select" => {
            app.ui.left_tab = crate::LeftTab::File;
            app.ui.open_left_panel();
            app.ui.pages_open = true;
        }
        // From a menu the link is pasted into the Join dialog (`collab.join {link}` joins directly).
        "collab.join" => crate::live::open_join(app, ""),
        // So is the server address for publishing (it carries the server key).
        "collab.publish" => crate::live::open_publish(app),
        "edit.copyProperties" => {
            if app.run(id, json!({})).is_some() {
                app.toast(tr("Properties copied"));
            }
        }
        "edit.copy" | "edit.cut" => {
            let ids = app.session.selection();
            // For other apps: text layers' text, other layers as SVG; taken before a cut removes them.
            let doc = app.session.doc().ok();
            let mirror = doc.and_then(|d| text_layers_text(d, &ids).or_else(|| figcraft_svg_export(d, &ids)));
            app.run(id, json!({}));
            // Mirror to the system clipboard (text or SVG, else a marker) and remember it, so a
            // later paste knows whether the clipboard still holds our layers. Control-channel
            // gestures keep the engine's copy but never replace the user's clipboard; `run` then
            // invalidates the old mirror because it no longer describes the engine clipboard.
            if !app.control_synthetic {
                let text = mirror.unwrap_or_else(|| format!("figcraft://clipboard/{}", ids.len()));
                app.clipboard_set_text(&text);
                app.ui.last_copy = Some(text);
                app.ui.last_copy_generation = app.session.clipboard.layers_generation;
            }
        }
        "edit.paste" => paste(app, None, None),
        "file.close" if app.session.ds().is_ok_and(|d| d.is_dirty() && d.branch_of.is_none()) => {
            let save = pretty_shortcut(crate::shortcuts::shortcut_for(app, "file.save").unwrap_or(""));
            app.toast(trf(
                "Unsaved changes: save first ({save}), or close the tab with {alt}-click to discard",
                &[("save", &save), ("alt", &pretty_shortcut("Alt"))],
            ));
        }
        _ => {
            app.run(id, json!({}));
        }
    }
}

/// Every visible top-level frame of the page as one PDF page each, through a save dialog.
fn export_frames_pdf(app: &mut FigApp) {
    let name = app.session.doc().map(|d| d.name.clone()).unwrap_or_else(|_| "Untitled".into());
    if app.services.downloads() {
        let file = format!("{name}.pdf");
        if let Some(r) = app.run("file.exportFramesPdf", json!({}))
            && app.download(&file, &r)
        {
            app.toast(trf("Exported {name}", &[("name", &file)]));
        }
        return;
    }
    app.pick_save(tr("Export"), &format!("{name}.pdf"), &["pdf"], |app, p| {
        if app.run("file.exportFramesPdf", json!({"path": p.to_string_lossy()})).is_some() {
            app.toast(trf("Exported {name}", &[("name", &p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())]));
        }
    });
}

/// Whether a menu id is available (registered in the engine or handled by the UI).
pub fn available(id: &str) -> bool {
    !id.is_empty() && (figcraft_engine::find_command(id).is_some() || UI_COMMANDS.contains(&id))
}

/// A checked main-menu option, including preference rows whose state lives outside the view.
fn main_menu_checked(app: &FigApp, id: &str) -> Option<bool> {
    let prefs = &app.session.prefs;
    match id {
        "pref.keepTool" => Some(prefs.keep_tool),
        "pref.highlightHover" => Some(prefs.highlight_on_hover),
        "pref.renameDuplicates" => Some(prefs.rename_duplicates),
        "pref.showDimensions" => Some(prefs.show_dimensions),
        "pref.numberKeysOpacity" => Some(prefs.number_keys_opacity),
        "text.spelling" => Some(prefs.spell_check),
        _ => figcraft_engine::cmd::view::checked(&app.session, id),
    }
}

/// Draw one level of the main menu: `prefix` is the menu path ("File", "Edit/Copy as").
fn menu_level(app: &mut FigApp, ui: &mut Ui, prefix: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    // The installed plugins head the Plugins menu.
    if prefix == "Plugins" && crate::plugins_ui::menu(app, ui, &t) {
        return true;
    }
    let mut clicked = None;
    let mut subs: Vec<String> = Vec::new();
    let mut last_sep = true;
    for m in MENU.iter() {
        if m.menu == prefix {
            if m.label == SEP {
                if !last_sep {
                    separator(ui);
                    last_sep = true;
                }
                continue;
            }
            last_sep = false;
            if m.id == "file.openRecent" {
                if recent_menu(app, ui, prefix, m.label, &i18n::menu_item(m.id, m.label), &t) {
                    return true;
                }
                continue;
            }
            // One window in the browser.
            let enabled = available(m.id) && !(m.id == "window.new" && ui.ctx().embed_viewports());
            let option = main_menu_checked(app, m.id);
            let checked = option.unwrap_or(false);
            if item(ui, &i18n::menu_item(m.id, m.label), crate::shortcuts::shortcut_for(app, m.id).unwrap_or(""), enabled, checked, &t) && enabled {
                clicked = Some((m.id, option.is_some()));
            }
        } else if let Some(rest) = m.menu.strip_prefix(prefix).and_then(|r| r.strip_prefix('/')) {
            let sub = rest.split('/').next().unwrap_or(rest).to_string();
            if !subs.contains(&sub) {
                subs.push(sub.clone());
                last_sep = false;
                let full = format!("{prefix}/{sub}");
                if submenu(ui, &full, tr(&sub), &t, |ui| menu_level(app, ui, &full).then_some(())).is_some() {
                    return true;
                }
            }
        }
    }
    if let Some((id, option)) = clicked {
        invoke(app, id);
        return !app.session.prefs.keep_menu_open || !option;
    }
    false
}

/// File ▸ Open recent: the recent files (greyed when the file is gone) and Clear recent. `key` is
/// the menu's English label, `label` the one shown. Returns whether an item ran.
fn recent_menu(app: &mut FigApp, ui: &mut Ui, prefix: &str, key: &str, label: &str, t: &Tokens) -> bool {
    enum Picked {
        Provider,
        Home,
        Device(serde_json::Value),
    }
    let picked = submenu(ui, &format!("{prefix}/{key}"), label, t, |ui| {
        if let Some(key) = crate::shortcuts::shortcut_for(app, "file.openRecent")
            && item(ui, tr("Show recent files"), key, true, false, t)
        {
            return Some(Picked::Home);
        }
        if app.file_providers.recent_ui(ui) {
            return Some(Picked::Provider);
        }
        let recents: Vec<&String> = crate::home_recents::listed(&app.session.prefs.recent_files).collect();
        let mut picked = None;
        for p in recents.iter().copied() {
            let path = std::path::Path::new(p);
            let name = path.file_name().map_or_else(|| p.clone(), |f| f.to_string_lossy().into_owned());
            let exists = path.exists();
            if item(ui, &name, "", exists, false, t) && exists {
                picked = Some(json!({"path": p}));
            }
        }
        if !recents.is_empty() {
            separator(ui);
        }
        if item(ui, tr("Clear recent"), "", !recents.is_empty(), false, t) && !recents.is_empty() {
            picked = Some(json!({"clear": true}));
        }
        picked.map(Picked::Device)
    });
    let Some(picked) = picked else { return false };
    if matches!(picked, Picked::Home) {
        invoke(app, "file.openRecent");
        return true;
    }
    let Picked::Device(params) = picked else { return true };
    // Through the user path, so an import report shows; `file.openRecent` also drops a file that is gone.
    if app.run_user("file.openRecent", params).is_some_and(|r| r.get("index").is_some()) {
        app.ui.home = false;
    }
    true
}

/// Open the main menu at top-level menu `top` (control channel).
pub fn open_main_menu(ctx: &egui::Context, top: &str) {
    set_open(ctx, main_menu_id(), "", Some(top));
}

/// The context menu's area (and menu tree).
fn ctx_menu_id() -> Id {
    Id::unique("ctx_menu")
}

/// The main menu's area.
pub fn main_menu_id() -> Id {
    Id::unique("main_menu")
}

/// Whether the pointer is over a menu (any foreground area).
fn over_menu(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.pointer.interact_pos()).and_then(|p| ctx.layer_id_at(p)).is_some_and(|l| l.order == egui::Order::Foreground)
}

/// The main menu's menus, in order (the in-window menu and the native menu bar).
pub const TOP_MENUS: &[&str] = &[
    tr_noop("File"),
    tr_noop("Edit"),
    tr_noop("View"),
    tr_noop("Object"),
    tr_noop("Text"),
    tr_noop("Arrange"),
    tr_noop("Vector"),
    tr_noop("Plugins"),
    tr_noop("Preferences"),
    tr_noop("Libraries"),
    tr_noop("Help"),
];

/// Draw the main and context menu trees: forget closed/stale submenu state, apply
/// control-channel Select-layer open/close, close on outside click or Escape, then draw the
/// colour picker and palette popups.
pub fn popups(app: &mut FigApp, ctx: &egui::Context) {
    let t = Tokens::get(ctx);
    // A closed menu forgets its open submenus (and a context menu opened somewhere else starts
    // afresh), so neither reopens with a stale submenu showing.
    if !app.ui.main_menu {
        forget_tree(ctx, main_menu_id());
    }
    let ctx_at = app.ui.context_menu.as_ref().map(|(p, _)| *p);
    let ctx_seen = Id::unique("ctx_menu_at");
    if ctx.data(|d| d.get_temp::<Option<egui::Pos2>>(ctx_seen)) != Some(ctx_at) {
        ctx.data_mut(|d| d.insert_temp(ctx_seen, ctx_at));
        forget_tree(ctx, ctx_menu_id());
    }
    // Select layer opened or closed from the control channel.
    if let Some(open) = ctx.data_mut(|d| d.remove_temp::<bool>(select_layer_ask_id())) {
        set_open(ctx, ctx_menu_id(), "", open.then_some(SELECT_LAYER));
    }
    // Main menu.
    if app.ui.main_menu {
        let mut close = false;
        let area = egui::Area::new(main_menu_id()).fixed_pos(pos2(8.0, crate::tabs::HEIGHT + 48.0)).order(egui::Order::Foreground).show(ctx, |ui| {
            menu_frame(ui, &t, At { root: main_menu_id(), path: String::new() }, |ui| {
                // Quick actions search row.
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                plain_row_hover(ui, &resp);
                a11y::menu_item(&resp, tr("Actions…"), crate::shortcuts::shortcut_for(app, "app.actions").unwrap_or(""), None);
                ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(5), Color32::from_white_alpha(18));
                icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + 16.0, r.center().y), vec2(14.0, 14.0)), Icon::Search, t.menu_dim);
                ui.painter().text(pos2(r.min.x + 32.0, r.center().y), Align2::LEFT_CENTER, tr("Actions…"), regular(12.0), t.menu_dim);
                ui.painter().text(
                    pos2(r.max.x - 10.0, r.center().y),
                    Align2::RIGHT_CENTER,
                    pretty_shortcut(crate::shortcuts::shortcut_for(app, "app.actions").unwrap_or("")),
                    regular(12.0),
                    t.menu_dim,
                );
                if resp.clicked() {
                    set_palette(&mut app.ui, true);
                    close = true;
                }
                ui.add_space(6.0);
                for top in TOP_MENUS {
                    if submenu_at(
                        ui,
                        SubmenuOptions { area: Id::unique(("menu", *top)), key: top, label: tr(top), gap: 12.0, constrain: false, tokens: &t },
                        |ui| menu_level(app, ui, top).then_some(()),
                    )
                    .is_some()
                    {
                        close = true;
                    }
                }
            });
        });
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let clicked_outside = ctx.input(|i| i.pointer.any_pressed()) && !over_menu(ctx) && !area.response.hovered();
        if close || escape || clicked_outside {
            app.ui.main_menu = false;
            forget_tree(ctx, main_menu_id());
        }
    }
    // Context menu (from the keyboard: at the focused layer row or the canvas selection, taking the
    // keyboard from the control with it).
    if std::mem::take(&mut app.ui.key_menu) {
        crate::canvas::locked::clear_context(app);
        a11y::context_menu(app, ctx);
    }
    crate::canvas::locked::validate_context(app);
    if let Some((pos, ids)) = app.ui.context_menu.clone() {
        if let Some(f) = ctx.memory(|m| m.focused()) {
            a11y::popup(&mut app.ui.nav, f, Id::unique("ctx_menu"));
        }
        let mut close = false;
        let area = egui::Area::new(ctx_menu_id()).fixed_pos(pos).order(egui::Order::Foreground).constrain(true).show(ctx, |ui| {
            menu_frame(ui, &t, At { root: ctx_menu_id(), path: String::new() }, |ui| {
                if let Some(unlocked) = crate::canvas::locked::menu(app, ui, &t) {
                    close |= unlocked;
                    return;
                }
                if spelling_entries(app, ui, pos, &t) {
                    close = true;
                }
                if select_layer_menu(app, ui, pos, &t) {
                    close = true;
                }
                let has_sel = !ids.is_empty();
                // Sections in the selection add a contents toggle naming what it will do.
                let section_toggle = app.session.doc().ok().and_then(|doc| {
                    let secs: Vec<bool> = ids
                        .iter()
                        .filter_map(|i| doc.node(*i))
                        .filter(|n| matches!(n.kind, figcraft_doc::NodeKind::Section { .. }))
                        .map(|n| n.hides_contents())
                        .collect();
                    (!secs.is_empty()).then(|| if secs.iter().all(|h| *h) { tr("Show section contents") } else { tr("Hide section contents") })
                });
                // Frames on the page (or in a section) convert to sections, and sections to frames.
                let convert = app.session.doc().ok().and_then(|doc| {
                    let nodes: Vec<_> = ids.iter().filter_map(|i| doc.node(*i)).collect();
                    let on_board = |n: &figcraft_doc::Node| {
                        n.parent.and_then(|p| doc.node(p)).is_some_and(|p| p.is_page() || matches!(p.kind, figcraft_doc::NodeKind::Section { .. }))
                    };
                    if nodes.is_empty() {
                        None
                    } else if nodes.iter().all(|n| matches!(n.kind, figcraft_doc::NodeKind::Section { .. })) {
                        Some((tr("Convert to frame"), "object.convertToFrame"))
                    } else if nodes.iter().all(|n| matches!(n.kind, figcraft_doc::NodeKind::Frame) && on_board(n)) {
                        Some((tr("Convert to section"), "object.convertToSection"))
                    } else {
                        None
                    }
                });
                // Two or more main components combine into a component set. Judged on the selection
                // the command acts on: a layer row right-clicked in a multi-selection passes only itself.
                let combine = app
                    .session
                    .doc()
                    .ok()
                    .filter(|doc| figcraft_engine::cmd::component::can_combine_variants(doc, &app.session.selection()))
                    .map(|_| i18n::command("component.combineVariants", "Combine as variants"));
                // One layer with an image fill can be cropped (judged on the selection, as above).
                let crop = app.session.doc().ok().is_some_and(|doc| {
                    matches!(app.session.selection().as_slice(), [id] if doc.node(*id).and_then(figcraft_engine::cmd::image::top_image_fill).is_some())
                });
                let copy_link = i18n::command("edit.copyLink", "Copy link to selection");
                let mut entries: Vec<(&str, &str)> = if has_sel {
                    vec![
                        (tr("Copy"), "edit.copy"),
                        (tr("Paste here"), "edit.paste"),
                        (tr("Paste to replace"), "edit.pasteReplace"),
                        (tr("Copy as"), COPY_AS),
                        ("-", ""),
                        (tr("Bring to front"), "arrange.front"),
                        (tr("Bring forward"), "arrange.forward"),
                        (tr("Send backward"), "arrange.backward"),
                        (tr("Send to back"), "arrange.back"),
                        ("-", ""),
                        (tr("Group selection"), "object.group"),
                        (tr("Frame selection"), "object.frameSelection"),
                        (tr("Wrap in new section"), "object.wrapInSection"),
                        (tr("Ungroup"), "object.ungroup"),
                        (tr("Flatten"), "vector.flatten"),
                        (tr("Outline stroke"), "vector.outlineStroke"),
                        (tr("Use as mask"), "object.mask"),
                        ("-", ""),
                        (tr("Add auto layout"), "layout.add"),
                        (tr("Remove all auto layout"), "layout.removeAll"),
                        (tr("Create component"), "component.create"),
                        (tr("Detach instance"), "component.detach"),
                        ("-", ""),
                        (tr("Show/Hide"), "object.toggleVisible"),
                        (tr("Lock/Unlock"), "object.toggleLock"),
                        ("-", ""),
                        (tr("Flip horizontal"), "object.flipH"),
                        (tr("Flip vertical"), "object.flipV"),
                        ("-", ""),
                        (tr("Delete"), "edit.delete"),
                    ]
                } else {
                    vec![
                        (tr("Paste here"), "edit.paste"),
                        ("-", ""),
                        (tr("Select all"), "select.all"),
                        ("-", ""),
                        (tr("Show/Hide UI"), "view.toggleUi"),
                        (tr("Zoom to fit"), "view.zoomFit"),
                        (tr("Rulers"), "view.rulers"),
                        (tr("Pixel grid"), "view.pixelGrid"),
                    ]
                };
                if let Some(label) = section_toggle
                    && let Some(at) = entries.iter().position(|e| e.1 == "object.toggleLock")
                {
                    entries.insert(at + 1, (label, "section.toggleContents"));
                }
                if let Some(entry) = convert
                    && let Some(at) = entries.iter().position(|e| e.1 == "object.wrapInSection")
                {
                    entries.insert(at + 1, entry);
                }
                if let Some(label) = &combine
                    && let Some(at) = entries.iter().position(|e| e.1 == "component.create")
                {
                    entries.insert(at + 1, (label, "component.combineVariants"));
                }
                if crop && let Some(at) = entries.iter().position(|e| e.1 == "object.mask") {
                    entries.insert(at + 1, (tr("Crop image"), "image.crop"));
                }
                // An instance's actions reach its main component: replace "Create component" with
                // "Go to main component" (as in Figma's context menu).
                if app
                    .session
                    .doc()
                    .ok()
                    .is_some_and(|doc| ids.iter().all(|i| doc.node(*i).is_some_and(|n| matches!(n.kind, figcraft_doc::NodeKind::Instance { .. }))))
                    && let Some(at) = entries.iter().position(|e| e.1 == "component.create")
                {
                    entries[at] = (tr("Go to main component"), "component.goToMain");
                }
                if has_sel {entries.push((tr("Highlight color"),"ui.highlightColor"));}
                // A right-clicked ruler guide (selected, with no layers) can be removed.
                if !has_sel && app.session.ds().is_ok_and(|d| d.selected_guide().is_some()) {
                    entries.splice(0..0, [(tr("Remove guide"), "guide.remove"), ("-", "")]);
                }
                for (label, id) in &entries {
                    if *id=="ui.highlightColor" {if highlight_color_menu(app,ui,&ids,&t){close=true;}continue;}
                    if *label == "-" {
                        separator(ui);
                        continue;
                    }
                    if *id == COPY_AS {
                        let copy_items = [
                            (tr("Copy as CSS"), "edit.copyAsCss"),
                            (tr("Copy as SVG"), "edit.copyAsSvg"),
                            (tr("Copy as PNG"), "edit.copyAsPng"),
                            (&*copy_link, "edit.copyLink"),
                        ];
                        let picked = submenu(ui, COPY_AS, label, &t, |ui| {
                            let mut picked = None;
                            for (label, id) in copy_items {
                                let sc = crate::shortcuts::shortcut_for(app, id).unwrap_or("");
                                if item(ui, label, sc, available(id), false, &t) {
                                    picked = Some(id);
                                }
                            }
                            picked
                        });
                        if let Some(id) = picked {
                            invoke(app, id);
                            close = true;
                        }
                        continue;
                    }
                    let sc = crate::shortcuts::shortcut_for(app, id).unwrap_or("");
                    if item(ui, label, sc, available(id), false, &t) {
                        if *id == "edit.paste" {
                            let p = app.to_page(pos);
                            paste(app, None, Some((p.x, p.y)));
                        } else {
                            invoke(app, id);
                        }
                        close = true;
                    }
                }
            });
        });
        // The press of a tap that opened the menu (press and release in one frame) is spared.
        let fresh = !std::mem::replace(&mut app.ui.context_menu_shown, true);
        let clicked_outside = !fresh && ctx.input(|i| i.pointer.any_pressed()) && !area.response.hovered() && !over_menu(ctx);
        if close || clicked_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.ui.context_menu = None;
            forget_tree(ctx, ctx_menu_id());
        }
    } else {
        app.ui.context_menu_shown = false;
    }
    crate::colorpicker::show(app, ctx);
    palette(app, ctx);
}

/// The context menu's entries for the misspelled word it was opened on at `pos`: its suggestions,
/// Ignore and Add to dictionary. Returns whether one was picked.
fn spelling_entries(app: &mut FigApp, ui: &mut Ui, pos: egui::Pos2, t: &Tokens) -> bool {
    let Some(crate::SpellingContext { index, word, suggestions, .. }) =
        app.ui.context_spelling.clone().filter(|_| spelling_context_current(app, pos))
    else {
        return false;
    };
    let mut picked = None;
    if suggestions.is_empty() {
        item(ui, "No suggestions", "", false, false, t);
    }
    for s in &suggestions {
        if item(ui, s, "", true, false, t) {
            picked = Some(("text.replaceWord", json!({"index": index, "with": s})));
        }
    }
    separator(ui);
    if item(ui, "Ignore", "", true, false, t) {
        picked = Some(("spelling.ignore", json!({"word": word})));
    }
    if item(ui, "Add to dictionary", "", true, false, t) {
        picked = Some(("spelling.addWord", json!({"word": word})));
    }
    separator(ui);
    picked.map(|(id, p)| app.run(id, p)).is_some()
}

/// A cached suggestion belongs to the exact document and text-edit snapshot it inspected.
pub(crate) fn spelling_context_current(app: &FigApp, pos: egui::Pos2) -> bool {
    app.session.prefs.spell_check
        && app.ui.context_spelling.as_ref().is_some_and(|context| {
            context.position == pos
                && app.session.ds().is_ok_and(|ds| {
                    ds.uid == context.document_uid
                        && ds.text_edit.as_ref().is_some_and(|te| te.id == context.layer)
                        && std::sync::Arc::ptr_eq(&ds.doc, &context.snapshot)
                })
        })
}

/// The context menu's Copy as submenu: its key in the tree, and the marker row in the entries.
const COPY_AS: &str = "Copy as";

/// A menu item with a color swatch indicator on the left.
fn color_item(ui: &mut Ui, label: &str, color: Color32, checked: bool, t: &Tokens) -> bool {
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
    a11y::menu_item(&resp, label, "", checked.then_some(true));
    a11y::own_focus(&resp);
    let hov = plain_row_hover(ui, &resp);
    if hov {
        ui.painter().rect_filled(r, CornerRadius::same(5), t.menu_hover);
    }
    let swatch_rect = Rect::from_center_size(pos2(r.min.x + 14.0, r.center().y), vec2(12.0, 12.0));
    ui.painter().rect_filled(swatch_rect, CornerRadius::same(3), color);
    if checked {
        icons::paint(ui.painter(), swatch_rect.shrink(1.0), Icon::Check, Color32::WHITE);
    }
    let galley = ui.painter().layout_no_wrap(label.to_string(), regular(12.0), t.menu_text);
    tree::need_width(ui, 32.0 + galley.size().x + 20.0);
    ui.painter().galley(pos2(r.min.x + 30.0, r.center().y - galley.size().y / 2.0), galley, t.menu_text);
    resp.clicked()
}

/// Returns true if a node is a top-level frame-like, section, or group directly on a page.
pub(crate) fn is_main_level_node(doc: &figcraft_doc::Document, id: figcraft_doc::NodeId) -> bool {
    figcraft_engine::cmd::highlights::is_main_level(doc, id)
}

/// The context menu's "Highlight color" submenu to customize layer / main frame colors.
fn highlight_color_menu(app: &mut FigApp, ui: &mut Ui, ids: &[figcraft_doc::NodeId], t: &Tokens) -> bool {
    enum Action {
        Set(Option<Color32>),
        ApplyAll(Option<Color32>),
        Reset,
        ResetAll,
    }
    let current_custom = ids.first().and_then(|id| app.session.doc().ok()?.node(*id)?.highlight_color).map(|c| {
        let [r, g, b, a] = c.to_rgba8();
        Color32::from_rgba_unmultiplied(r, g, b, a)
    });

    let picked = submenu_at(
        ui,
        SubmenuOptions {
            area: Id::unique("ctx_highlight_color"),
            key: "highlight_color",
            label: tr("Highlight color"),
            gap: 4.0,
            constrain: true,
            tokens: t,
        },
        |ui| {
            let mut act = None;
            let colors: [(&str, Option<Color32>, Color32); 10] = [
                (tr("Default"), None, t.accent_text),
                (tr("Blue"), Some(Color32::from_rgb(0x3B, 0x82, 0xF6)), Color32::from_rgb(0x3B, 0x82, 0xF6)),
                (tr("Purple"), Some(Color32::from_rgb(0xA8, 0x55, 0xF7)), Color32::from_rgb(0xA8, 0x55, 0xF7)),
                (tr("Pink"), Some(Color32::from_rgb(0xEC, 0x48, 0x99)), Color32::from_rgb(0xEC, 0x48, 0x99)),
                (tr("Red"), Some(Color32::from_rgb(0xEF, 0x44, 0x44)), Color32::from_rgb(0xEF, 0x44, 0x44)),
                (tr("Orange"), Some(Color32::from_rgb(0xF9, 0x73, 0x16)), Color32::from_rgb(0xF9, 0x73, 0x16)),
                (tr("Amber"), Some(Color32::from_rgb(0xF5, 0x9E, 0x0B)), Color32::from_rgb(0xF5, 0x9E, 0x0B)),
                (tr("Green"), Some(Color32::from_rgb(0x10, 0xB9, 0x81)), Color32::from_rgb(0x10, 0xB9, 0x81)),
                (tr("Teal"), Some(Color32::from_rgb(0x14, 0xB8, 0xA6)), Color32::from_rgb(0x14, 0xB8, 0xA6)),
                (tr("Gray"), Some(Color32::from_rgb(0x94, 0xA3, 0xB8)), Color32::from_rgb(0x94, 0xA3, 0xB8)),
            ];
            for (name, val, swatch_col) in colors {
                let is_checked = current_custom == val;
                if color_item(ui, name, swatch_col, is_checked, t) {
                    act = Some(Action::Set(val));
                }
            }
            separator(ui);
            if item(ui, tr("Apply to all main frames"), "", true, false, t) {
                act = Some(Action::ApplyAll(current_custom));
            }
            if item(ui, tr("Reset to default"), "", true, false, t) {
                act = Some(Action::Reset);
            }
            if item(ui, tr("Reset all main frames"), "", true, false, t) {
                act = Some(Action::ResetAll);
            }
            act
        },
    );

    if let Some(act) = picked {
        let color = |c: Option<Color32>| c.map(|c| format!("#{:02X}{:02X}{:02X}{:02X}", c.r(), c.g(), c.b(), c.a()));
        match act {
            Action::Set(c) => {
                app.run("layer.setHighlight", serde_json::json!({"ids":ids,"color":color(c)}));
            }
            Action::ApplyAll(c) => {
                app.run("layer.setHighlight", serde_json::json!({"allMain":true,"color":color(c)}));
            }
            Action::ResetAll => {
                app.run("layer.resetHighlight", serde_json::json!({"allMain":true}));
            }
            Action::Reset => {
                app.run("layer.resetHighlight", serde_json::json!({"ids":ids}));
            }
        }
        true
    } else {
        false
    }
}
