//! The main menu in the macOS menu bar (craft-menu): installed with the window, synced each frame,
//! acting on the window in front.

use craft_menu::{Layout, LayoutOptions, NativeMenu};
use egui::ViewportId;
use figcraft_ui_egui::FigApp;
use figcraft_ui_egui::native_menu;
use figcraft_ui_egui::windows::with_app;
use std::hash::{DefaultHasher, Hash, Hasher};

fn sync_state(input: u64, front: ViewportId, shortcuts: &figcraft_engine::keymap::ShortcutOverrides) -> u64 {
    // A control command can rebind keys while no physical input or window change occurs.
    // Include the saved mappings so native key equivalents cannot retain the previous owner.
    let mut state = DefaultHasher::new();
    (input, front.0.value(), shortcuts).hash(&mut state);
    state.finish()
}

/// The Mac layout of the main menu for the window in front, with the labels the layout writes
/// translated.
fn layout(fig: &mut FigApp, windows: bool) -> Layout {
    let label = |english: &'static str| native_menu::mac_label(english);
    craft_menu::mac_layout(&native_menu::menu_bar_in_front(fig, windows), &LayoutOptions { label: &label, ..LayoutOptions::new("FigCraft") })
}

/// Put the main menu in the menu bar. `None` when AppKit refused it: the in-window menu stays.
pub fn install(ctx: &egui::Context) -> Option<NativeMenu> {
    let bar = with_app(|fig| layout(fig, !ctx.embed_viewports()).bar)?;
    craft_menu::mac::install(ctx, &bar)
}

/// Before a window's frame: the menu bar's key equivalents go into the window in front's input
/// only. In the main window's input while another window is in front, a paste the menu bar asked
/// for goes on to that window.
pub fn raw_input(menu: &mut NativeMenu, ctx: &egui::Context, raw: &mut egui::RawInput) {
    let front = with_app(|fig| native_menu::front(fig)).unwrap_or(ViewportId::ROOT);
    if raw.viewport_id == front {
        menu.raw_input(raw);
    } else if raw.viewport_id == ViewportId::ROOT {
        let pastes = native_menu::take_pastes(&mut raw.events);
        if !pastes.is_empty() {
            with_app(|fig| native_menu::send_keys(fig, ctx, pastes));
        }
    }
}

/// One frame (the main window's): hand the window in front the key equivalents waiting for it, run
/// the items chosen since the last frame there, then bring the menus up to date (laid out again
/// only after input, a menu event, shortcut changes or another window coming to the front).
pub fn frame(menu: &mut NativeMenu, ctx: &egui::Context) {
    menu.set_gate(with_app(|fig| native_menu::gate(fig, ctx)).unwrap_or_default());
    let front = with_app(|fig| native_menu::front(fig)).unwrap_or(ViewportId::ROOT);
    if front != ViewportId::ROOT {
        // A menu event wakes only the main window: the window in front may not draw until its next input.
        let mut keys = egui::RawInput { viewport_id: front, ..Default::default() };
        menu.raw_input(&mut keys);
        if !keys.events.is_empty() {
            with_app(|fig| native_menu::send_keys(fig, ctx, keys.events));
        }
    }
    for id in menu.take_clicks(ctx) {
        with_app(|fig| native_menu::run_in_front(fig, &id.id));
    }
    let input = if craft_menu::had_input(ctx) { ctx.cumulative_frame_nr() } else { 0 };
    let state = with_app(|fig| sync_state(input, front, &fig.session.prefs.shortcuts)).unwrap_or(input ^ front.0.value());
    let windows = !ctx.embed_viewports();
    let clashes = menu.sync_with(state, || {
        with_app(|fig| layout(fig, windows))
            .unwrap_or_else(|| Layout { bar: craft_menu::MenuBar { menus: Vec::new(), documents: Vec::new() }, clashes: Vec::new() })
    });
    for c in clashes {
        log::info!("menu: {} is {} on macOS and {} in FigCraft; {}", c.shortcut, c.system, c.command, c.resolution);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figcraft_engine::Session;
    use figcraft_ui_egui::NoServices;
    use serde_json::json;

    #[test]
    fn native_menu_cache_refreshes_after_idle_shortcut_commands_and_reset() {
        let mut app = FigApp::new(Session::new(), Box::new(NoServices));
        let state = |app: &FigApp| sync_state(0, ViewportId::ROOT, &app.session.prefs.shortcuts);
        let initial = state(&app);
        for key in ["Alt+F8", "Alt+F9"] {
            let previous = state(&app);
            app.session.execute("pref.shortcut", &json!({"id": "file.versionHistory", "platform": "mac", "keys": [key]})).unwrap();
            let bar = native_menu::menu_bar(&app, true);
            assert_eq!(bar.find(&"file.versionHistory".into()).unwrap().shortcut, craft_menu::Chord::parse(key).ok());
            assert_ne!(state(&app), previous, "a changed native accelerator must refresh even without physical input");
        }
        app.session.execute("pref.resetShortcut", &json!({"id": "file.versionHistory", "platform": "mac"})).unwrap();
        let bar = native_menu::menu_bar(&app, true);
        assert_eq!(bar.find(&"file.versionHistory".into()).unwrap().shortcut, None);
        assert_eq!(state(&app), initial, "reset restores the canonical menu state");
    }
}
