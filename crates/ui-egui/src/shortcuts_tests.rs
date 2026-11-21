//! Keyboard shortcuts driven through egui headless: number keys for opacity (two quick digits make
//! an exact percentage), Shift+0/1/2 zoom, alignment and Shift+E/Shift+D panel keys, Cmd/Ctrl+K on
//! the canvas and while editing text, text formatting keys while editing, and the hold-Z zoom tool.
//! Modifiers are Cmd/Alt/Shift only, which mean the same on every platform.

use egui::{Event, Key, Modifiers, pos2};
use figcraft_doc::{NodeId, TextDecoration};
use figcraft_engine::Session;
use figcraft_engine::keymap::KEYS;
use serde_json::json;

use crate::RightTab;
use crate::layers_drag_tests::Ui;

fn custom_text(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Pos2 {
    fn find(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.job.text == label => Some(t.pos + t.galley.size() / 2.0),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    shapes.iter().find_map(|shape| find(&shape.shape, label)).unwrap_or_else(|| panic!("missing visible text {label}"))
}

fn custom_click(u: &mut Ui, label: &str) {
    let output = u.frame_output(vec![]);
    let p = custom_text(&output.shapes, label);
    u.button(p, true);
    u.button(p, false);
}

fn custom_panel(u: &mut Ui, id: &str) {
    u.ctx.set_os(egui::os::OperatingSystem::Mac);
    u.app.ui.shortcuts_panel = true;
    u.app.ui.shortcuts_query = id.to_string();
    u.frame(vec![]);
    u.frame(vec![]);
}

#[test]
fn custom_shortcut_unbound_row_records_persists_reopens_dispatches_and_resets() {
    let (mut u, _, _) = setup();
    let command = "file.versionHistory";
    custom_panel(&mut u, command);
    let title = figcraft_engine::keymap::command_binding(command).unwrap().title();
    custom_click(&mut u, title);
    assert!(crate::shortcuts_panel::recording(&u.ctx));
    press(&mut u, Key::F8, Modifiers::ALT);
    assert!(!crate::shortcuts_panel::recording(&u.ctx));
    assert!(!u.app.ui.versions.open, "recording the key does not invoke it");
    assert_eq!(u.app.ui.shortcuts_query, command, "no text leaked into search");
    assert_eq!(crate::shortcuts::shortcut_for(&u.app, command), Some("Alt+F8"));
    let palette = crate::menus::command_sections_for(&u.app.session.prefs, command);
    let hint = &palette.iter().flat_map(|(_, rows)| rows).find(|r| r.key == format!("command:{command}")).unwrap().hint;
    assert_eq!(hint, &crate::toolbar::pretty_shortcut("Alt+F8"));
    let bar = crate::native_menu::menu_bar(&u.app, true);
    assert_eq!(bar.find(&command.into()).unwrap().shortcut, craft_menu::Chord::parse("Alt+F8").ok());
    let commands = crate::control::handle(&mut u.app, &u.ctx, "engine.commands", &json!({})).unwrap();
    assert_eq!(commands.as_array().unwrap().iter().find(|c| c["id"] == command).unwrap()["shortcut"], "Alt+F8");
    u.app.ui.shortcuts_panel = false;
    press(&mut u, Key::F8, Modifiers::ALT);
    assert!(u.app.ui.versions.open, "previously unbound command invokes its actual panel");
    let saved = serde_json::to_value(&u.app.session.prefs).unwrap();
    let mut session = Session::new();
    session.prefs = serde_json::from_value(saved).unwrap();
    let mut reopened = Ui::new(session);
    custom_panel(&mut reopened, command);
    assert_eq!(crate::shortcuts::shortcut_for(&reopened.app, command), Some("Alt+F8"));
    custom_click(&mut reopened, "Reset all shortcuts");
    assert!(crate::shortcuts::shortcut_for(&reopened.app, command).is_none());
    assert_eq!(serde_json::to_value(&reopened.app.session.prefs).unwrap()["shortcuts"], json!({}));
}

#[test]
fn custom_shortcut_conflict_buttons_and_escape_do_not_run_the_captured_command() {
    let (mut u, _, _) = setup();
    let command = "file.versionHistory";
    custom_panel(&mut u, command);
    let title = figcraft_engine::keymap::command_binding(command).unwrap().title();
    custom_click(&mut u, title);
    press(&mut u, Key::K, Modifiers::COMMAND);
    assert!(u.app.ui.palette.is_none());
    assert!(crate::shortcuts::shortcut_for(&u.app, command).is_none(), "conflict is pending");
    assert!(crate::native_menu::gate(&mut u.app, &u.ctx).recording_shortcut);
    custom_click(&mut u, "Cancel");
    assert_eq!(crate::shortcuts::shortcut_for(&u.app, "app.actions"), Some("Cmd+K"));
    custom_click(&mut u, title);
    press(&mut u, Key::K, Modifiers::COMMAND);
    custom_click(&mut u, "Replace shortcut");
    assert_eq!(crate::shortcuts::shortcut_for(&u.app, command), Some("Cmd+K"));
    assert_eq!(crate::shortcuts::shortcut_for(&u.app, "app.actions"), Some("Cmd+/"), "other alternate survives explicit replacement");
    custom_click(&mut u, title);
    let before = u.app.session.prefs.clone();
    press(&mut u, Key::Escape, Modifiers::NONE);
    assert!(!crate::shortcuts_panel::recording(&u.ctx));
    assert_eq!(u.app.session.prefs, before);
    assert!(!u.app.ui.versions.open);
}

#[test]
fn custom_shortcut_capture_swallows_text_clipboard_and_release_batches() {
    let (mut u, _, _) = setup();
    custom_panel(&mut u, "view.zoomIn");
    crate::shortcuts_panel::begin_capture(&mut u.app, &u.ctx, "view.zoomIn");
    let before = zoom(&u);
    u.frame(vec![
        Event::Key { key: Key::F8, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::ALT },
        Event::Text("leak".into()),
        Event::Paste("leak".into()),
    ]);
    assert_eq!(zoom(&u), before);
    assert_eq!(u.app.ui.shortcuts_query, "view.zoomIn");
    u.frame(vec![Event::Key { key: Key::F8, physical_key: None, pressed: true, repeat: true, modifiers: Modifiers::ALT }]);
    assert_eq!(zoom(&u), before, "held captured key is not dispatched on repeat");
    u.frame(vec![Event::Key { key: Key::F8, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::ALT }]);
    press(&mut u, Key::F8, Modifiers::ALT);
    assert!(zoom(&u) > before, "a new press dispatches after release");
}

#[test]
fn custom_shortcuts_do_not_steal_text_editing_and_recording_does_not_insert_text() {
    let mut s = Session::new();
    let t = id(s.execute("text.create", &json!({"text": "Hello", "x": 0, "y": 0})).unwrap());
    s.execute("select.set", &json!({"ids": [t]})).unwrap();
    s.execute("text.edit.begin", &json!({"id": t})).unwrap();
    s.execute("pref.shortcut", &json!({"id": "view.zoomIn", "platform": "mac", "keys": ["Alt+F8"]})).unwrap();
    assert_eq!(s.prefs.shortcuts.primary("view.zoomIn", figcraft_engine::keymap::Platform::Mac), Some("Alt+F8"));
    let mut u = Ui::new(s);
    u.ctx.set_os(egui::os::OperatingSystem::Mac);
    let before = zoom(&u);
    press(&mut u, Key::F8, Modifiers::ALT);
    assert_eq!(zoom(&u), before, "canvas command is not live in a text editor");
    press(&mut u, Key::A, Modifiers::COMMAND);
    press(&mut u, Key::U, Modifiers::COMMAND);
    assert_eq!(u.app.session.doc().unwrap().node(t).unwrap().kind.text().unwrap().style_at(1).decoration, TextDecoration::Underline);
    custom_panel(&mut u, "file.versionHistory");
    crate::shortcuts_panel::begin_capture(&mut u.app, &u.ctx, "file.versionHistory");
    u.frame(vec![
        Event::Key { key: Key::F9, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::ALT },
        Event::Text("leak".into()),
        Event::Paste("leak".into()),
    ]);
    assert_eq!(u.app.session.doc().unwrap().node(t).unwrap().kind.text().unwrap().characters, "Hello");
    assert!(!u.app.ui.versions.open);
}

#[test]
fn custom_shortcut_capture_swallows_os_clipboard_repeats_until_modifier_release() {
    let (mut u, _, _) = setup();
    custom_panel(&mut u, "edit.copy");
    crate::shortcuts_panel::begin_capture(&mut u.app, &u.ctx, "edit.copy");
    let before = u.app.session.clipboard.layers_generation;
    u.frame_with(vec![Event::Copy], Modifiers::COMMAND);
    assert_eq!(u.app.session.clipboard.layers_generation, before);
    u.frame_with(vec![Event::Copy], Modifiers::COMMAND);
    assert_eq!(u.app.session.clipboard.layers_generation, before, "no physical key-up is delivered for an OS copy event");
    u.frame_with(vec![], Modifiers::NONE);
    u.frame_with(vec![Event::Copy], Modifiers::COMMAND);
    assert!(u.app.session.clipboard.layers_generation > before, "a fresh OS copy works after releasing the modifier");
}

#[test]
fn custom_shortcut_capture_preserves_modifiers_across_raw_clipboard_batches() {
    let (mut u, _, _) = setup();
    custom_panel(&mut u, "edit.copy");
    crate::shortcuts_panel::begin_capture(&mut u.app, &u.ctx, "edit.copy");
    let before = u.app.session.clipboard.layers_generation;
    u.frame_with(vec![Event::Copy], Modifiers::COMMAND);
    assert_eq!(u.app.session.clipboard.layers_generation, before);
    // A real host reports a modifier only when it changes. The next clipboard repeat has no
    // ModifiersChanged event, so it must inherit the command key still held by the first batch.
    u.frame_raw(egui::RawInput { events: vec![Event::Copy], ..Default::default() });
    assert_eq!(u.app.session.clipboard.layers_generation, before, "recording still owns this repeat");
    assert!(u.ctx.input(|i| i.modifiers.command), "capture must preserve the physical modifier state");
    u.frame_raw(egui::RawInput { events: vec![Event::ModifiersChanged(Modifiers::NONE)], ..Default::default() });
    u.frame_with(vec![Event::Copy], Modifiers::COMMAND);
    assert!(u.app.session.clipboard.layers_generation > before, "a new copy works after modifier release");
}

#[test]
fn custom_shortcut_row_context_menu_removes_and_restores_defaults() {
    let (mut u, _, _) = setup();
    custom_panel(&mut u, "view.zoomIn");
    u.app.run("pref.shortcut", json!({"id": "view.zoomIn", "platform": "mac", "keys": ["Alt+F8"]}));
    let title = figcraft_engine::keymap::command_binding("view.zoomIn").unwrap().title();
    for action in ["Remove shortcut", "Reset shortcut"] {
        let output = u.frame_output(vec![]);
        let p = custom_text(&output.shapes, title);
        for pressed in [true, false] {
            u.frame(vec![
                Event::PointerMoved(p),
                Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: Modifiers::NONE },
            ]);
        }
        custom_click(&mut u, action);
        if action == "Remove shortcut" {
            assert!(crate::shortcuts::shortcut_for(&u.app, "view.zoomIn").is_none());
        } else {
            assert_eq!(crate::shortcuts::shortcut_for(&u.app, "view.zoomIn"), Some("Cmd+="));
            assert!(!u.app.session.prefs.shortcuts.overridden("view.zoomIn", figcraft_engine::keymap::Platform::Mac));
        }
    }
}

#[test]
fn custom_shortcut_open_recent_has_a_native_accelerator_and_opens_recents() {
    let (mut u, _, _) = setup();
    custom_panel(&mut u, "file.openRecent");
    u.app.run("pref.shortcut", json!({"id": "file.openRecent", "platform": "mac", "keys": ["Alt+F10"]}));
    let bar = crate::native_menu::menu_bar(&u.app, true);
    let item = bar.find(&"file.openRecent".into()).unwrap();
    assert_eq!(item.shortcut, craft_menu::Chord::parse("Alt+F10").ok());
    assert_eq!(item.label, "Show recent files");
    u.app.ui.shortcuts_panel = false;
    press(&mut u, Key::F10, Modifiers::ALT);
    assert!(u.app.ui.home);
    assert_eq!(u.app.ui.home_view.section, crate::home::Section::Recents);
}

#[test]
fn custom_shortcut_capture_drains_gesture_keys_held_before_recording() {
    let (mut u, _, _) = setup();
    custom_panel(&mut u, "view.zoomIn");
    u.frame(vec![Event::Key { key: Key::Space, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    crate::shortcuts_panel::begin_capture(&mut u.app, &u.ctx, "view.zoomIn");
    press(&mut u, Key::F9, Modifiers::ALT);
    assert!(crate::shortcuts_panel::recording(&u.ctx), "the earlier Space still belongs to recording until release");
    u.frame(vec![Event::Key { key: Key::Space, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert!(!crate::shortcuts_panel::recording(&u.ctx));
}

#[test]
fn custom_shortcut_dispatch_replaces_default_and_can_be_disabled() {
    check_zoom_shortcut(egui::os::OperatingSystem::Nix, "other");
    check_zoom_shortcut(egui::os::OperatingSystem::Mac, "mac");
}

fn check_zoom_shortcut(os: egui::os::OperatingSystem, platform: &str) {
    let (mut u, _, _) = setup();
    u.ctx.set_os(os);
    assert!(u.app.run("pref.shortcut", json!({"id": "view.zoomIn", "platform": platform, "keys": ["Alt+F8"]})).is_some());
    let before = zoom(&u);
    press(&mut u, Key::F8, Modifiers::ALT);
    assert!(zoom(&u) > before, "custom chord dispatches the command");
    let before = zoom(&u);
    press(&mut u, Key::Plus, Modifiers::SHIFT);
    assert_eq!(zoom(&u), before, "default was replaced");
    u.app.run("pref.shortcut", json!({"id": "view.zoomIn", "platform": platform, "keys": []}));
    press(&mut u, Key::F8, Modifiers::ALT);
    assert_eq!(zoom(&u), before, "empty keys unbinds");
    u.app.run("pref.resetShortcut", json!({"id": "view.zoomIn", "platform": platform}));
    press(&mut u, Key::Plus, Modifiers::SHIFT);
    assert!(zoom(&u) > before, "row reset restores the default");
}

fn id(v: serde_json::Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

fn press(u: &mut Ui, key: Key, modifiers: Modifiers) {
    u.frame(vec![
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers },
        Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers },
    ]);
}

fn opacity(u: &Ui, id: NodeId) -> f32 {
    u.app.session.doc().unwrap().node(id).unwrap().opacity
}

fn zoom(u: &Ui) -> f64 {
    u.app.session.ds().unwrap().view.zoom
}

/// Longer than the window for a second opacity digit.
fn pause(u: &mut Ui) {
    for _ in 0..60 {
        u.frame(vec![]);
    }
}

/// Two rectangles, both selected.
fn setup() -> (Ui, NodeId, NodeId) {
    let mut s = Session::new();
    let a = id(s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 40, "height": 40})).unwrap());
    let b = id(s.execute("shape.rectangle", &json!({"x": 100, "y": 30, "width": 20, "height": 20})).unwrap());
    s.execute("select.set", &json!({"ids": [a, b]})).unwrap();
    (Ui::new(s), a, b)
}

#[test]
fn every_shortcut_reaches_something_the_app_runs() {
    for b in KEYS {
        assert!(crate::menus::available(b.id), "{} is bound but nothing runs it", b.id);
    }
}

#[test]
fn number_keys_set_opacity_and_two_quick_digits_an_exact_percentage() {
    let (mut u, a, b) = setup();
    press(&mut u, Key::Num5, Modifiers::NONE);
    assert_eq!((opacity(&u, a), opacity(&u, b)), (0.5, 0.5));
    pause(&mut u);
    // 4 then 5 straight after: 45%.
    press(&mut u, Key::Num4, Modifiers::NONE);
    assert_eq!(opacity(&u, a), 0.4);
    press(&mut u, Key::Num5, Modifiers::NONE);
    assert_eq!(opacity(&u, a), 0.45);
    // A pause starts over: 0 alone is 100%.
    pause(&mut u);
    press(&mut u, Key::Num0, Modifiers::NONE);
    assert_eq!(opacity(&u, a), 1.0);
    // Off in Preferences: digits do nothing.
    u.app.run("pref.numberKeysOpacity", json!({"on": false}));
    pause(&mut u);
    press(&mut u, Key::Num3, Modifiers::NONE);
    assert_eq!(opacity(&u, a), 1.0);
}

#[test]
fn shift_digits_zoom() {
    let (mut u, _, b) = setup();
    u.app.run("view.set", json!({"zoom": 0.5}));
    press(&mut u, Key::Num0, Modifiers::SHIFT);
    assert_eq!(zoom(&u), 1.0, "Shift+0: 100%");
    // Shift+1 arrives as `!` on most layouts.
    press(&mut u, Key::Exclamationmark, Modifiers::SHIFT);
    let fit = zoom(&u);
    assert!(fit > 1.0, "Shift+1 fits the two small rectangles: {fit}");
    u.app.run("select.set", json!({"ids": [b]}));
    press(&mut u, Key::Num2, Modifiers::SHIFT);
    assert!(zoom(&u) > fit, "Shift+2 fits the smaller selection");
    let v = &u.app.session.ds().unwrap().view;
    assert_eq!((v.cx, v.cy), (110.0, 40.0), "centred on it");
    // `+` and `-` step the zoom.
    let z = zoom(&u);
    press(&mut u, Key::Minus, Modifiers::NONE);
    let out = zoom(&u);
    assert!(out < z, "- zooms out");
    press(&mut u, Key::Plus, Modifiers::SHIFT);
    assert!(zoom(&u) > out, "+ zooms in");
}

#[test]
fn alignment_keys() {
    let (mut u, a, b) = setup();
    let x = |u: &Ui, id: NodeId| u.app.session.doc().unwrap().world_bounds(id).unwrap();
    press(&mut u, Key::D, Modifiers::ALT);
    assert_eq!((x(&u, a).x1, x(&u, b).x1), (120.0, 120.0), "Alt+D aligns right edges");
    press(&mut u, Key::W, Modifiers::ALT);
    assert_eq!((x(&u, a).y0, x(&u, b).y0), (0.0, 0.0), "Alt+W aligns tops");
    press(&mut u, Key::V, Modifiers::ALT);
    assert_eq!((x(&u, a).center().y, x(&u, b).center().y), (20.0, 20.0), "Alt+V centres vertically");
}

#[test]
fn panel_keys_and_cmd_k() {
    let (mut u, _, _) = setup();
    press(&mut u, Key::E, Modifiers::SHIFT);
    assert_eq!(u.app.ui.right_tab, RightTab::Prototype, "Shift+E: Prototype");
    press(&mut u, Key::E, Modifiers::SHIFT);
    assert_eq!(u.app.ui.right_tab, RightTab::Design, "and back to Design");
    press(&mut u, Key::D, Modifiers::SHIFT);
    assert_eq!(u.app.ui.right_tab, RightTab::Inspect, "Shift+D: Dev Mode");
    press(&mut u, Key::D, Modifiers::SHIFT);
    assert_eq!(u.app.ui.right_tab, RightTab::Design);
    let collapsed = u.app.ui.left_collapsed;
    let minimized = |u: &Ui| u.app.session.ds().unwrap().view.ui_minimized;
    press(&mut u, Key::Backslash, Modifiers::SHIFT | Modifiers::COMMAND);
    assert!(minimized(&u), "Shift+Cmd+\\ minimizes the interface");
    assert_eq!(u.app.ui.left_collapsed, collapsed, "without collapsing the left sidebar for good");
    press(&mut u, Key::Backslash, Modifiers::SHIFT | Modifiers::COMMAND);
    assert!(!minimized(&u), "and back");
    press(&mut u, Key::K, Modifiers::COMMAND);
    assert!(u.app.ui.palette.is_some(), "Cmd+K opens Actions on the canvas");
}

#[test]
fn text_keys_while_editing() {
    let mut s = Session::new();
    let t = id(s.execute("text.create", &json!({"text": "Hello", "x": 0, "y": 0})).unwrap());
    s.execute("select.set", &json!({"ids": [t]})).unwrap();
    let mut u = Ui::new(s);
    press(&mut u, Key::Enter, Modifiers::NONE);
    assert!(u.app.session.ds().unwrap().text_edit.is_some(), "Enter edits the text");
    press(&mut u, Key::A, Modifiers::COMMAND);
    press(&mut u, Key::U, Modifiers::COMMAND);
    let style = |u: &Ui| u.app.session.doc().unwrap().node(t).unwrap().kind.text().unwrap().style_at(1).clone();
    assert_eq!(style(&u).decoration, TextDecoration::Underline, "Cmd+U underlines the selection");
    let size = style(&u).font_size;
    press(&mut u, Key::Period, Modifiers::SHIFT | Modifiers::COMMAND);
    assert!(style(&u).font_size > size, "Cmd+Shift+> grows the text");
    // Plain letters type; Cmd+K doesn't open Actions while editing.
    press(&mut u, Key::K, Modifiers::COMMAND);
    assert!(u.app.ui.palette.is_none());
    assert!(u.app.session.ds().unwrap().text_edit.is_some(), "still editing");
}

#[test]
fn hold_z_zoom_tool() {
    let (mut u, _, _) = setup();
    u.app.run("view.set", json!({"zoom": 1.0}));
    let p = pos2(700.0, 450.0);
    let held = Modifiers::NONE;
    u.frame(vec![Event::PointerMoved(p), Event::Key { key: Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: held }]);
    let before = u.app.to_page(p);
    u.button(p, true);
    u.button(p, false);
    assert_eq!(zoom(&u), 2.0, "a click with Z held zooms in a step");
    let after = u.app.to_page(p);
    assert!((before.x - after.x).abs() < 0.01 && (before.y - after.y).abs() < 0.01, "about the pointer");
    assert_eq!(u.app.session.selection().len(), 2, "the click didn't reach the move tool");
    u.frame(vec![Event::Key { key: Key::Z, physical_key: None, pressed: false, repeat: false, modifiers: held }]);
    u.button(p, true);
    u.button(p, false);
    assert_eq!(zoom(&u), 2.0, "released: clicks select again");
}

#[test]
fn shortcuts_panel_opens_switches_tabs_and_searches() {
    let (mut u, _, _) = setup();
    // Ctrl+Shift+? (Control on macOS too).
    press(&mut u, Key::Questionmark, Modifiers { ctrl: true, command: !cfg!(target_os = "macos"), shift: true, ..Modifiers::NONE });
    assert!(u.app.ui.shortcuts_panel, "Ctrl+Shift+? opens the panel");
    for g in figcraft_engine::keymap::Group::ALL {
        u.app.ui.shortcuts_tab = g;
        u.frame(vec![]);
    }
    u.app.ui.shortcuts_query = "zoom".into();
    u.frame(vec![]);
    u.app.ui.shortcuts_query = "no such shortcut".into();
    u.frame(vec![]);
    u.app.run("help.shortcuts", json!({}));
    assert!(!u.app.ui.shortcuts_panel, "the command toggles it");
}

#[test]
fn shortcuts_panel_does_not_show_on_the_home_screen() {
    let mut u = Ui::new(Session::empty());
    u.app.ui.home = true;
    u.app.ui.shortcuts_panel = true;
    u.frame(vec![]);
    assert!(!u.app.ui.shortcuts_panel, "the panel closes on the home screen");
}
