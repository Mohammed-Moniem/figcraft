use egui::{Event, Key, Modifiers, pos2};
use serde_json::json;

use crate::present_tests::Ui;

fn setup(enabled: bool) -> Ui {
    let mut session = figcraft_engine::Session::new();
    let mut prefs = serde_json::to_value(&session.prefs).unwrap();
    prefs["keep_menu_open"] = json!(enabled);
    session.prefs = serde_json::from_value(prefs).unwrap();
    let mut ui = Ui::new(session);
    ui.ctx.enable_accesskit();
    ui.app.ui.main_menu = true;
    crate::menus::open_main_menu(&ui.ctx, "View");
    ui.frames(4, Vec::new);
    ui
}

fn click(ui: &mut Ui, label: &str) {
    let nodes = ui.frame_output(Vec::new()).platform_output.accesskit_update.unwrap().nodes;
    let node = nodes
        .into_iter()
        .map(|(_, node)| node)
        .find(|node| node.label() == Some(label))
        .unwrap_or_else(|| panic!("missing real menu control {label}"));
    let b = node.bounds().unwrap();
    let point = pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
    assert!(ui.ctx.content_rect().contains(point));
    ui.button(point, true);
    ui.button(point, false);
    ui.frames(2, Vec::new);
}

#[test]
fn menu_keep_open_default_closes_after_a_real_checked_option_click() {
    let mut ui = setup(false);
    let before = ui.app.session.ds().unwrap().view.rulers;
    click(&mut ui, "Rulers");
    assert_eq!(ui.app.session.ds().unwrap().view.rulers, !before);
    assert!(!ui.app.ui.main_menu);
}

#[test]
fn menu_keep_open_enabled_allows_repeated_real_toggles_and_escape_closes() {
    let mut ui = setup(true);
    let doc = ui.app.session.doc().unwrap().clone();
    let history = ui.app.session.undo_depth();
    let before = ui.app.session.ds().unwrap().view.rulers;
    click(&mut ui, "Rulers");
    assert_eq!(ui.app.session.ds().unwrap().view.rulers, !before);
    assert!(ui.app.ui.main_menu, "the opted-in menu remains available after a checked option");
    click(&mut ui, "Rulers");
    assert_eq!(ui.app.session.ds().unwrap().view.rulers, before);
    assert!(ui.app.ui.main_menu);
    ui.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    ui.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert!(!ui.app.ui.main_menu);
    assert_eq!(ui.app.session.doc().unwrap(), &doc);
    assert_eq!(ui.app.session.undo_depth(), history);
}

#[test]
fn menu_keep_open_enabled_still_closes_on_an_actual_outside_click() {
    let mut ui = setup(true);
    let point = pos2(1100.0, 750.0);
    ui.button(point, true);
    ui.button(point, false);
    assert!(!ui.app.ui.main_menu);
}

#[derive(Clone)]
struct PreferenceSaves(std::rc::Rc<std::cell::RefCell<Vec<figcraft_engine::Prefs>>>);

impl crate::Services for PreferenceSaves {
    fn save_prefs(&mut self, prefs: &figcraft_engine::Prefs) {
        self.0.borrow_mut().push(prefs.clone());
    }
}

#[test]
fn menu_keep_open_ordinary_actions_still_close_and_change_the_view() {
    let mut ui = setup(true);
    let zoom = ui.app.session.ds().unwrap().view.zoom;
    click(&mut ui, "Zoom in");
    assert!(ui.app.session.ds().unwrap().view.zoom > zoom);
    assert!(!ui.app.ui.main_menu);
}

#[test]
fn menu_keep_open_nested_options_remain_available_for_repeated_clicks() {
    let mut ui = setup(true);
    let nodes = ui.frame_output(Vec::new()).platform_output.accesskit_update.unwrap().nodes;
    let node = nodes.into_iter().map(|(_, node)| node).find(|node| node.label() == Some("Outlines")).unwrap();
    let b = node.bounds().unwrap();
    let point = pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
    ui.frames(20, || vec![Event::PointerMoved(point)]);
    let before = ui.app.session.ds().unwrap().view.outlines;
    click(&mut ui, "Show outlines");
    assert_eq!(ui.app.session.ds().unwrap().view.outlines, !before);
    assert!(ui.app.ui.main_menu);
    click(&mut ui, "Show outlines");
    assert_eq!(ui.app.session.ds().unwrap().view.outlines, before);
    assert!(ui.app.ui.main_menu);
}

#[test]
fn menu_keep_open_preference_options_repeat_and_ordinary_dialog_action_closes() {
    let mut ui = setup(true);
    crate::menus::open_main_menu(&ui.ctx, "Preferences");
    ui.frames(4, Vec::new);
    let before = ui.app.session.prefs.keep_tool;
    click(&mut ui, "Keep tool selected after use");
    assert_eq!(ui.app.session.prefs.keep_tool, !before);
    assert!(ui.app.ui.main_menu);
    click(&mut ui, "Keep tool selected after use");
    assert_eq!(ui.app.session.prefs.keep_tool, before);
    assert!(ui.app.ui.main_menu);
    click(&mut ui, "Preferences…");
    assert!(!ui.app.ui.main_menu);
    assert!(ui.app.ui.preferences);
}

#[test]
fn menu_keep_open_escape_also_closes_while_hovering_the_root_menu() {
    let mut ui = setup(false);
    let nodes = ui.frame_output(Vec::new()).platform_output.accesskit_update.unwrap().nodes;
    let node = nodes.into_iter().map(|(_, node)| node).find(|node| node.label() == Some("View")).unwrap();
    let b = node.bounds().unwrap();
    let point = pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
    ui.frame(vec![Event::PointerMoved(point)]);
    ui.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    assert!(!ui.app.ui.main_menu, "Escape closes even while the root menu owns the pointer");
}

#[test]
fn menu_keep_open_real_preferences_checkbox_is_available_and_saves_both_directions() {
    let mut ui = setup(false);
    let saved = PreferenceSaves(Default::default());
    ui.app.services = Box::new(saved.clone());
    ui.app.ui.main_menu = false;
    ui.run("app.preferences", json!({}));
    ui.frames(4, Vec::new);
    let doc = ui.app.session.doc().unwrap().clone();
    let history = ui.app.session.undo_depth();
    for value in [true, false] {
        ui.app.session.prefs_unsaved = false;
        saved.0.borrow_mut().clear();
        click(&mut ui, "Keep menus open when toggling options");
        assert_eq!(serde_json::to_value(&ui.app.session.prefs).unwrap()["keep_menu_open"], value);
        let writes = saved.0.borrow();
        assert!(!writes.is_empty(), "the actual preferences-save service received the changed opt-in");
        assert_eq!(serde_json::to_value(writes.last().unwrap()).unwrap()["keep_menu_open"], value);
    }
    assert_eq!(ui.app.session.doc().unwrap(), &doc);
    assert_eq!(ui.app.session.undo_depth(), history);
    assert!(!ui.app.session.ds().unwrap().is_dirty());
}
