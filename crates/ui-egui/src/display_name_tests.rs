//! Your name, one for comments and live sessions (#1380): "Your name" in Preferences, shown and
//! editable in the Join dialog, and kept as soon as it changes.

use std::sync::{Arc, Mutex};

use egui::{Event, Key, Modifiers};
use figcraft_engine::Prefs;

use crate::Services;
use crate::live_tests::{Harness, has_text, text_center};

/// Keeps the name of each preferences save.
struct Kept(Arc<Mutex<Vec<String>>>);

impl Services for Kept {
    fn save_prefs(&mut self, prefs: &Prefs) {
        self.0.lock().unwrap().push(prefs.user_name.clone());
    }
}

fn harness() -> (Harness, Arc<Mutex<Vec<String>>>) {
    let (mut h, _) = Harness::new();
    let kept = Arc::new(Mutex::new(Vec::new()));
    h.app.services = Box::new(Kept(kept.clone()));
    (h, kept)
}

/// Replace the text of the field showing `old` with `new` and press Enter.
fn retype(h: &mut Harness, old: &str, new: &str) {
    if h.app.ui.preferences {
        let ctx = h.ctx.clone();
        crate::preferences_test_support::show(&ctx, |events| h.frame_output(events), "Your name", egui::accesskit::Role::TextInput);
    }
    let shapes = h.frames(1);
    h.click(text_center(&shapes, old));
    let key = |key, modifiers| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
    h.frame(vec![key(Key::A, Modifiers::COMMAND)]);
    h.frame(vec![Event::Text(new.into())]);
    h.frame(vec![key(Key::Enter, Modifiers::NONE)]);
    h.frames(1);
}

#[test]
fn the_join_dialog_shows_your_name_and_changes_it() {
    let (mut h, kept) = harness();
    h.app.session.execute("pref.userName", &serde_json::json!({"name": "Ada"})).unwrap();
    crate::menus::invoke(&mut h.app, "collab.join");
    let shapes = h.frames(3);
    assert!(has_text(&shapes, "Your name") && has_text(&shapes, "Ada"), "the name others will see");
    retype(&mut h, "Ada", "Bob");
    assert_eq!(h.app.session.prefs.user_name, "Bob");
    assert_eq!(kept.lock().unwrap().last().map(String::as_str), Some("Bob"), "kept at once");
    assert!(h.app.ui.live.join.is_some(), "the dialog stays to join");
}

#[test]
fn preferences_call_it_your_name_and_the_default_shows_empty() {
    let (mut h, kept) = harness();
    h.app.ui.preferences = true;
    let shapes = h.frames(3);
    assert!(has_text(&shapes, "Your name") && !has_text(&shapes, "Your name in comments"));
    assert!(!has_text(&shapes, "You"), "the default isn't a name to edit");
    h.app.session.execute("pref.userName", &serde_json::json!({"name": "Ada"})).unwrap();
    retype(&mut h, "Ada", "Lin");
    assert_eq!(h.app.session.prefs.user_name, "Lin");
    let names = kept.lock().unwrap();
    assert_eq!(names.last().map(String::as_str), Some("Lin"));
    assert_eq!(
        names.iter().filter(|name| name.as_str() == "Lin").count(),
        1,
        "editing the name saves it once; session restore can independently save initial window state"
    );
}

struct SavedSnapshots(Arc<Mutex<Vec<Prefs>>>);

impl Services for SavedSnapshots {
    fn save_prefs(&mut self, prefs: &Prefs) {
        self.0.lock().unwrap().push(prefs.clone());
    }
}

fn snapshot_harness() -> (Harness, Arc<Mutex<Vec<Prefs>>>) {
    let (mut h, _) = Harness::new();
    let kept = Arc::new(Mutex::new(Vec::new()));
    h.app.services = Box::new(SavedSnapshots(kept.clone()));
    (h, kept)
}

#[test]
fn name_save_standalone_command_still_persists_at_the_next_logic_tick() {
    let (mut h, kept) = snapshot_harness();
    let doc = h.app.session.doc().unwrap().clone();
    let history = h.app.session.undo_depth();
    assert!(h.app.run("pref.userName", serde_json::json!({"name":"Standalone original name"})).is_some());
    assert!(h.app.session.prefs_unsaved, "the direct command still requests durable preferences");
    assert!(kept.lock().unwrap().is_empty(), "a direct command does not imitate a field's immediate service save");
    h.app.keep_preferences(&h.ctx);
    assert!(!h.app.session.prefs_unsaved);
    assert_eq!(kept.lock().unwrap().len(), 1);
    assert_eq!(kept.lock().unwrap()[0].user_name, "Standalone original name");
    h.app.keep_preferences(&h.ctx);
    assert_eq!(kept.lock().unwrap().len(), 1, "a fulfilled direct request does not repeat");
    assert_eq!(h.app.session.doc().unwrap(), &doc);
    assert_eq!(h.app.session.undo_depth(), history);
}

#[test]
fn name_save_failed_command_neither_writes_nor_discards_queued_valid_work() {
    let (mut h, kept) = snapshot_harness();
    let original = h.app.session.prefs.user_name.clone();
    assert!(h.app.run("pref.userName", serde_json::json!({"name":null})).is_none());
    assert_eq!(h.app.session.prefs.user_name, original);
    assert!(!h.app.session.prefs_unsaved);
    h.app.keep_preferences(&h.ctx);
    assert!(kept.lock().unwrap().is_empty(), "a failed command creates no service write");
    assert!(h.app.run("pref.userName", serde_json::json!({"name":"Queued original name"})).is_some());
    assert!(h.app.run("pref.userName", serde_json::json!({"name":[]})).is_none());
    assert!(h.app.session.prefs_unsaved, "failure cannot consume a prior valid request");
    assert!(kept.lock().unwrap().is_empty());
    h.app.keep_preferences(&h.ctx);
    assert_eq!(kept.lock().unwrap().len(), 1);
    assert_eq!(kept.lock().unwrap()[0].user_name, "Queued original name");
}

fn direct_name_field_frame(h: &mut Harness, time: &mut f64, mut events: Vec<Event>, modifiers: Modifiers) -> egui::FullOutput {
    *time += 1.0 / 60.0;
    events.insert(0, Event::ModifiersChanged(modifiers));
    let ctx = h.ctx.clone();
    let app = &mut h.app;
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 300.0))),
            time: Some(*time),
            events,
            ..Default::default()
        },
        |ui| crate::live::name_field(app, ui, egui::Id::unique("name-save-direct-field"), 200.0),
    );
    out.textures_delta.clear();
    out
}

#[test]
fn name_save_actual_field_commit_fulfils_only_its_preference_flag_and_keeps_other_work() {
    let (mut h, kept) = snapshot_harness();
    h.app.session.prefs.user_name = "Original field name".into();
    h.app.session.prefs.nudge_small = 7.0;
    let doc = h.app.session.doc().unwrap().clone();
    let history = h.app.session.undo_depth();
    let mut time = 0.0;
    let out = direct_name_field_frame(&mut h, &mut time, vec![], Modifiers::NONE);
    let at = text_center(&crate::live_tests::flat(&out.shapes), "Original field name");
    for pressed in [true, false] {
        direct_name_field_frame(
            &mut h,
            &mut time,
            vec![
                Event::PointerMoved(at),
                Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE },
            ],
            Modifiers::NONE,
        );
    }
    let key = |key, pressed, modifiers| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
    direct_name_field_frame(
        &mut h,
        &mut time,
        vec![key(Key::A, true, Modifiers::COMMAND), key(Key::A, false, Modifiers::COMMAND)],
        Modifiers::COMMAND,
    );
    direct_name_field_frame(&mut h, &mut time, vec![Event::Text("Immediate field name".into())], Modifiers::NONE);
    assert_eq!(h.app.session.prefs.user_name, "Original field name", "editing remains a draft until the actual field commits");
    h.app.session.plugins.unsaved = true;
    h.app.session.live.unsaved = true;
    direct_name_field_frame(
        &mut h,
        &mut time,
        vec![key(Key::Enter, true, Modifiers::NONE), key(Key::Enter, false, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(h.app.session.prefs.user_name, "Immediate field name");
    assert!(!h.app.session.prefs_unsaved, "the immediate full snapshot fulfils this command's request");
    assert!(h.app.session.plugins.unsaved && h.app.session.live.unsaved, "other pending owners retain their flags");
    assert_eq!(kept.lock().unwrap().len(), 1);
    assert_eq!(kept.lock().unwrap()[0].user_name, "Immediate field name");
    assert_eq!(kept.lock().unwrap()[0].nudge_small, 7.0, "the immediate save includes the full preferences snapshot");
    h.app.keep_preferences(&h.ctx);
    assert_eq!(kept.lock().unwrap().len(), 2, "the distinct plugin/live pending work is still saved");
    assert!(!h.app.session.plugins.unsaved && !h.app.session.live.unsaved && !h.app.session.prefs_unsaved);
    h.app.keep_preferences(&h.ctx);
    assert_eq!(kept.lock().unwrap().len(), 2);
    assert_eq!(h.app.session.doc().unwrap(), &doc);
    assert_eq!(h.app.session.undo_depth(), history);
}
