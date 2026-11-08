//! Browser preference storage orchestration, shared by the actual host and native model tests.

use figcraft_engine::{Prefs, Session};
use std::cell::OnceCell;
use std::rc::Rc;

/// Captures the actual assigned host value without duplicating its normalization or timing.
#[derive(Clone, Default)]
pub struct VisitNameCapture(Rc<OnceCell<(String, String)>>);

impl VisitNameCapture {
    pub fn assigned(&self, local_name: String, injected_name: String) {
        let _ = self.0.set((local_name, injected_name));
    }
}

#[derive(Clone)]
pub struct Storage {
    kept_readable: bool,
    appearance_writable: bool,
    visit_name: VisitNameCapture,
    explicit_name: bool,
}

impl Storage {
    pub fn readable(&self) -> bool {
        self.kept_readable && self.appearance_writable
    }

    pub fn visit_name_capture(&self) -> VisitNameCapture {
        self.visit_name.clone()
    }

    pub fn store(&mut self, prefs: &Prefs, mut write: impl FnMut(&str, &str) -> bool) -> bool {
        // Name controls save every committed change. The first observed different name is an
        // explicit edit, and later edits back to the injected host value stay explicit. An
        // unchanged value cannot reveal intent, so appearance-only saves never infer it.
        self.explicit_name |= self.visit_name.0.get().is_some_and(|(_, injected)| prefs.user_name.as_str() != injected.as_str());
        let name = self.visit_name.0.get().filter(|_| !self.explicit_name).map(|(local, _)| local.as_str());
        let kept = self.kept_readable
            && match name {
                Some(name) => crate::kept::store_named(name, &prefs.joined_files, |value| write(crate::kept::STATE_KEY, value)),
                None => crate::kept::store(prefs, |value| write(crate::kept::STATE_KEY, value)),
            };
        let appearance = self.appearance_writable && crate::appearance::store(prefs, |value| write(crate::appearance::STATE_KEY, value));
        kept && appearance
    }
}

pub fn load(session: &mut Session, mut read: impl FnMut(&str) -> Result<Option<String>, ()>) -> (bool, Storage) {
    let (restored, kept_readable) = crate::kept::load(session, &mut read);
    let (appearance_restored, appearance_writable) = crate::appearance::load(session, read);
    (restored && appearance_restored, Storage { kept_readable, appearance_writable, visit_name: VisitNameCapture::default(), explicit_name: false })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use figcraft_engine::{Prefs, Session};
    use serde_json::json;

    const APPEARANCE_KEY: &str = "figcraft.preferences.v1";

    fn configured() -> Session {
        let mut s = Session::new();
        for (id, params) in [
            ("pref.selectionOutlineOpacity", json!({"opacity":35.5})),
            ("pref.pixelGridSize", json!({"size":0.125})),
            ("pref.accentColor", json!({"color":"#0D99FF"})),
            ("pref.numberWheel", json!({"on":true})),
            ("pref.shortcut", json!({"id":"shape.rectangle","platform":"mac","keys":["Alt+F8"]})),
            ("pref.shortcut", json!({"id":"shape.rectangle","platform":"other","keys":["Alt+F8"]})),
        ] {
            s.execute(id, &params).unwrap();
        }
        assert!(!s.ds().unwrap().is_dirty());
        assert_eq!(s.undo_depth(), (0, 0));
        s
    }

    fn load(s: &mut Session, items: &BTreeMap<String, String>) -> super::Storage {
        let (restored, storage) = super::load(s, |key| Ok(items.get(key).cloned()));
        assert!(restored && storage.readable());
        storage
    }

    #[test]
    fn browser_five_selected_preferences_survive_the_same_storage_reload_path() {
        let mut before = configured();
        before.prefs.user_name = "Synthetic visitor".into();
        let mut items = BTreeMap::new();
        let mut storage = load(&mut Session::new(), &items);
        assert!(storage.store(&before.prefs, |key, value| {
            items.insert(key.into(), value.into());
            true
        }));
        let mut after = Session::new();
        load(&mut after, &items);
        assert_eq!(after.prefs.selection_outline_opacity(), 35.5);
        assert_eq!(after.prefs.pixel_grid_size, 0.125);
        assert_eq!(after.prefs.accent, "#0D99FF");
        assert!(after.prefs.number_wheel);
        assert_eq!(after.prefs.shortcuts, before.prefs.shortcuts);
        assert_eq!(after.prefs.user_name, "Synthetic visitor");
        assert!(items.contains_key(crate::kept::STATE_KEY) && items.contains_key(APPEARANCE_KEY));
        assert!(!after.ds().unwrap().is_dirty());
        assert_eq!(after.undo_depth(), (0, 0));
        assert!(!after.live.available() && after.live.iter().next().is_none());
    }

    #[test]
    fn unreadable_appearance_record_is_reported_and_never_overwritten_by_joined_save() {
        let mut s = Session::new();
        let (restored, mut storage) = super::load(&mut s, |key| if key == APPEARANCE_KEY { Err(()) } else { Ok(None) });
        assert!(!restored || !storage.readable(), "the real storage read failure must reach host feedback");
        let mut writes = Vec::new();
        assert!(!storage.store(&s.prefs, |key, _| {
            writes.push(key.to_owned());
            true
        }));
        assert!(!writes.iter().any(|key| key == APPEARANCE_KEY));
        assert!(writes.iter().any(|key| key == crate::kept::STATE_KEY), "the independent joined record remains writable");
    }

    #[test]
    fn appearance_write_failure_keeps_memory_usable_and_preserves_the_previous_record() {
        let before = configured();
        let mut items: BTreeMap<String, String> = BTreeMap::from([(APPEARANCE_KEY.into(), "previous durable synthetic record".into())]);
        let mut storage = load(&mut Session::new(), &BTreeMap::new());
        let old = items.get(APPEARANCE_KEY).cloned();
        assert!(
            !storage.store(&before.prefs, |key, value| {
                if key == APPEARANCE_KEY {
                    return false;
                }
                items.insert(key.into(), value.into());
                true
            }),
            "the host must report the failed appearance write"
        );
        assert_eq!(items.get(APPEARANCE_KEY), old.as_ref());
        assert!(before.prefs.number_wheel);
        assert_eq!(before.prefs.pixel_grid_size, 0.125);
        assert_ne!(before.prefs.shortcuts, Prefs::default().shortcuts);
    }

    #[test]
    fn joined_read_failure_blocks_only_the_joined_record_while_appearance_can_be_kept() {
        let mut s = configured();
        let (restored, mut storage) = super::load(&mut s, |key| if key == crate::kept::STATE_KEY { Err(()) } else { Ok(None) });
        assert!(!restored || !storage.readable());
        let mut writes = Vec::new();
        assert!(!storage.store(&s.prefs, |key, _| {
            writes.push(key.to_owned());
            true
        }));
        assert_eq!(writes, [APPEARANCE_KEY]);
    }

    #[test]
    fn malformed_or_future_appearance_records_remain_raw_while_name_saves_still_work() {
        for old in ["malformed synthetic-private-secret".into(), json!({"version":2,"unrecognized":"keep me"}).to_string()] {
            let mut items = named_items();
            items.insert(APPEARANCE_KEY.into(), old.clone());
            let mut s = Session::new();
            let (restored, mut storage) = super::load(&mut s, |key| Ok(items.get(key).cloned()));
            assert!(!restored && !storage.readable());
            s.execute("pref.userName", &json!({"name":"New local name"})).unwrap();
            assert!(!storage.store(&s.prefs, |key, value| {
                items.insert(key.into(), value.into());
                true
            }));
            assert_eq!(items.get(APPEARANCE_KEY), Some(&old));
            assert_eq!(serde_json::from_str::<serde_json::Value>(&items[crate::kept::STATE_KEY]).unwrap()["name"], "New local name");
        }
    }

    #[test]
    fn a_failed_joined_write_does_not_short_circuit_the_appearance_attempt() {
        let s = configured();
        let mut storage = load(&mut Session::new(), &BTreeMap::new());
        let mut writes = Vec::new();
        assert!(!storage.store(&s.prefs, |key, _| {
            writes.push(key.to_owned());
            key != crate::kept::STATE_KEY
        }));
        assert_eq!(writes, [crate::kept::STATE_KEY, APPEARANCE_KEY]);
    }

    #[test]
    fn appearance_storage_is_separate_from_the_unchanged_private_joined_record() {
        let mut s = configured();
        let joined: Prefs = serde_json::from_value(json!({
            "joined_files":[{"address":"wss://example.invalid/","file":"f","name":"Original synthetic file","link":"figcraft://join?server=wss://example.invalid/&file=f&token=synthetic-private-capability","access":"view","opened":123}]
        })).unwrap();
        s.prefs.joined_files = joined.joined_files;
        assert_eq!(s.prefs.joined_files.len(), 1);
        let mut items = BTreeMap::new();
        let mut storage = load(&mut Session::new(), &items);
        save(&mut items, &s, &mut storage);
        assert!(items[crate::kept::STATE_KEY].contains("synthetic-private-capability"));
        assert!(!items[APPEARANCE_KEY].contains("synthetic-private-capability"));
        let mut after = Session::new();
        load(&mut after, &items);
        assert_eq!(after.prefs.joined_files, s.prefs.joined_files);
        assert_eq!(after.prefs.shortcuts, s.prefs.shortcuts);
        assert_eq!(after.prefs.pixel_grid_size, 0.125);
    }

    fn named_items() -> BTreeMap<String, String> {
        BTreeMap::from([(crate::kept::STATE_KEY.into(), json!({"version":1,"name":"Original local name","joined_files":[]}).to_string())])
    }

    fn attributed(items: &BTreeMap<String, String>, author: Option<&str>) -> (Session, super::Storage) {
        let mut s = Session::new();
        let storage = load(&mut s, items);
        let capture = storage.visit_name_capture();
        // Model existing startup with bounded original ASCII names: restore, assign a nonblank
        // author exactly as the host setter does, then capture its actual resulting value.
        if let Some(author) = author.filter(|v| !v.trim().is_empty()) {
            let local = s.prefs.user_name.clone();
            s.prefs.user_name = author.trim().into();
            capture.assigned(local, s.prefs.user_name.clone());
        }
        (s, storage)
    }

    fn save(items: &mut BTreeMap<String, String>, s: &Session, storage: &mut super::Storage) {
        assert!(storage.store(&s.prefs, |key, value| {
            items.insert(key.into(), value.into());
            true
        }));
    }

    fn saved_name(items: &BTreeMap<String, String>) -> String {
        let mut s = Session::new();
        load(&mut s, items);
        s.prefs.user_name
    }

    #[test]
    fn attribution_appearance_only_saves_preserve_the_pre_author_local_name() {
        let mut items = named_items();
        let (mut s, mut storage) = attributed(&items, Some("Synthetic host author"));
        s.execute("pref.accentColor", &json!({"color":"#0D99FF"})).unwrap();
        save(&mut items, &s, &mut storage);
        assert_eq!(s.prefs.user_name, "Synthetic host author", "the active host author is unchanged");
        assert_eq!(saved_name(&items), "Original local name", "appearance saves cannot retain visit-only attribution");
    }

    #[test]
    fn attribution_normal_and_blank_author_visits_keep_explicit_name_edits() {
        for author in [None, Some("   ")] {
            let mut items = named_items();
            let (mut s, mut storage) = attributed(&items, author);
            s.execute("pref.userName", &json!({"name":"Explicit local name"})).unwrap();
            save(&mut items, &s, &mut storage);
            assert_eq!(saved_name(&items), "Explicit local name");
            assert_eq!(s.prefs.user_name, "Explicit local name");
        }
    }

    #[test]
    fn attribution_a_different_explicit_name_and_a_later_edit_back_are_kept() {
        let mut items = named_items();
        let (mut s, mut storage) = attributed(&items, Some("Synthetic host author"));
        s.execute("pref.userName", &json!({"name":"Explicit local name"})).unwrap();
        save(&mut items, &s, &mut storage);
        assert_eq!(saved_name(&items), "Explicit local name");
        s.execute("pref.userName", &json!({"name":"Synthetic host author"})).unwrap();
        save(&mut items, &s, &mut storage);
        assert_eq!(saved_name(&items), "Synthetic host author", "returning to the host text after an observed edit is explicit too");
        s.execute("pref.pixelGridSize", &json!({"size":0.125})).unwrap();
        save(&mut items, &s, &mut storage);
        assert_eq!(saved_name(&items), "Synthetic host author");
    }

    #[test]
    fn attribution_an_unchanged_name_is_not_inferred_to_be_an_explicit_edit() {
        let mut items = named_items();
        let (mut s, mut storage) = attributed(&items, Some("Synthetic host author"));
        s.execute("pref.userName", &json!({"name":"Synthetic host author"})).unwrap();
        save(&mut items, &s, &mut storage);
        assert_eq!(saved_name(&items), "Original local name", "the save boundary cannot distinguish an exact same-name commit from other prefs");
    }
}
