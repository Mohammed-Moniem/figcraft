//! Spell check while editing text, against a fake checker (tests never touch the system's).

use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::Session;
use crate::params::id_of;
use crate::spelling::SpellChecker;

/// Knows a few words; counts how often it's asked.
struct Fake(Arc<Mutex<usize>>);

impl SpellChecker for Fake {
    fn misspelled(&mut self, word: &str) -> bool {
        *self.0.lock().unwrap() += 1;
        !["Hello", "world", "don't", "the"].contains(&word)
    }
    fn suggest(&mut self, word: &str) -> Vec<String> {
        vec![format!("{word}!"), "world".into()]
    }
}

/// Editing a text layer "Hello wrold, don't Helo 42" with the fake checker installed.
fn editing() -> (Session, Arc<Mutex<usize>>) {
    let asked = Arc::new(Mutex::new(0));
    let mut s = Session::new();
    s.spelling.checker = Some(Box::new(Fake(asked.clone())));
    let t = id_of(&s.execute("text.create", &json!({"text": "Hello wrold, don't Helo 42", "x": 0, "y": 0})).unwrap()["id"]).unwrap();
    s.execute("text.edit.begin", &json!({"id": t, "caret": 0})).unwrap();
    s.sync_spelling();
    (s, asked)
}

#[test]
fn misspelled_words_are_flagged_and_cached() {
    let (mut s, asked) = editing();
    // "wrold" and "Helo"; the apostrophe stays inside "don't" and numbers aren't words to check.
    assert_eq!(s.misspelled(), &[(6, 11), (19, 23)]);
    let n = *asked.lock().unwrap();
    s.sync_spelling();
    assert_eq!(*asked.lock().unwrap(), n, "nothing changed: nothing checked again");
    s.execute("text.edit.key", &json!({"key": "End"})).unwrap();
    s.execute("text.edit.insert", &json!({"text": " Hello"})).unwrap();
    s.sync_spelling();
    assert_eq!(*asked.lock().unwrap(), n, "known words come from the cache");
    s.execute("text.spelling", &json!({"on": false})).unwrap();
    s.sync_spelling();
    assert!(s.misspelled().is_empty(), "switched off");
    assert!(!s.prefs.spell_check);
}

#[test]
fn scripts_without_spaces_are_not_checked() {
    let (mut s, _) = editing();
    s.execute("text.edit.key", &json!({"key": "End"})).unwrap();
    s.execute("text.edit.insert", &json!({"text": " 这是中文 สวัสดี Привет"})).unwrap();
    s.sync_spelling();
    // CJK and Thai runs aren't single words; Cyrillic is checked like any other spaced script.
    assert_eq!(s.misspelled(), &[(6, 11), (19, 23), (39, 45)]);
}

#[test]
fn suggestions_replace_the_word_in_one_undo_step() {
    let (mut s, _) = editing();
    let r = s.execute("text.spellingSuggestions", &json!({"index": 8})).unwrap();
    assert_eq!(r, json!({"word": "wrold", "start": 6, "end": 11, "misspelled": true, "suggestions": ["wrold!", "world"]}));
    let ok = s.execute("text.spellingSuggestions", &json!({"index": 0})).unwrap();
    assert_eq!(ok["misspelled"], json!(false));
    assert_eq!(ok["suggestions"], json!([]));
    s.execute("text.replaceWord", &json!({"index": 11, "with": "world"})).unwrap();
    s.execute("text.edit.end", &json!({})).unwrap();
    let text = |s: &Session| s.doc().unwrap().node(s.selection()[0]).unwrap().kind.text().unwrap().characters.clone();
    assert_eq!(text(&s), "Hello world, don't Helo 42");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(text(&s), "Hello wrold, don't Helo 42");
    // Not editing: nothing to replace.
    assert!(s.execute("text.replaceWord", &json!({"index": 0, "with": "x"})).is_err());
}

#[test]
fn ignore_and_add_to_dictionary() {
    let (mut s, _) = editing();
    s.execute("spelling.ignore", &json!({"word": "wrold"})).unwrap();
    s.sync_spelling();
    assert_eq!(s.misspelled(), &[(19, 23)]);
    s.execute("spelling.addWord", &json!({"word": "Helo"})).unwrap();
    s.sync_spelling();
    assert!(s.misspelled().is_empty());
    assert_eq!(s.prefs.dictionary, vec!["helo".to_string()], "saved lowercased in preferences");
    assert!(s.execute("spelling.addWord", &json!({"word": "two words"})).is_err());
}

#[test]
fn checker_language_and_availability_changes_invalidate_same_document_marks() {
    struct Changing(Arc<Mutex<(&'static str, bool)>>);
    impl SpellChecker for Changing {
        fn language(&self) -> String {
            self.0.lock().unwrap().0.into()
        }
        fn available(&self) -> bool {
            self.0.lock().unwrap().1
        }
        fn misspelled(&mut self, _word: &str) -> bool {
            self.0.lock().unwrap().0 != "fr"
        }
        fn suggest(&mut self, _word: &str) -> Vec<String> {
            vec![]
        }
    }
    let state = Arc::new(Mutex::new(("en", true)));
    let mut s = Session::new();
    s.spelling.set_checker(Box::new(Changing(state.clone())));
    let t = s.execute("text.create", &json!({"text":"bonjour", "x":0, "y":0})).unwrap();
    s.execute("text.edit.begin", &json!({"id":t["id"]})).unwrap();
    s.sync_spelling();
    assert_eq!(s.misspelled(), &[(0, 7)]);
    let revision = s.ds().unwrap().revision;
    *state.lock().unwrap() = ("fr", true);
    s.sync_spelling();
    assert!(s.misspelled().is_empty(), "the old English answer cannot survive a system-language change");
    assert_eq!(s.ds().unwrap().revision, revision, "a system-language change does not alter saved text");
    let info = s.execute("text.spelling", &json!({"on":true})).unwrap();
    assert_eq!(info["backend"], "custom");
    assert_eq!(info["language"], "fr");
    let revision = s.ds().unwrap().revision;
    *state.lock().unwrap() = ("en", false);
    s.sync_spelling();
    assert!(s.misspelled().is_empty());
    assert_eq!(s.ds().unwrap().revision, revision, "service availability does not alter saved text");
    assert_eq!(s.execute("text.spelling", &json!({"on":true})).unwrap()["available"], false);
    s.spelling.checker = None;
    let absent = s.execute("text.spelling", &json!({"on":true})).unwrap();
    assert_eq!(absent["backend"], "unavailable");
    assert!(absent["language"].is_null());
}

/// A deterministic persistence/service fixture. Only owned temporary bytes are read or written;
/// the fake checker has no OS learning API, and the production add-word command writes Prefs.
#[test]
fn isolated_prefs_reload_keeps_user_dictionary_but_not_run_only_ignores() {
    let (mut first, _) = editing();
    first.execute("spelling.ignore", &json!({"word":"wrold"})).unwrap();
    first.execute("spelling.addWord", &json!({"word":"Helo"})).unwrap();
    first.sync_spelling();
    assert!(first.misspelled().is_empty());
    assert_eq!(first.prefs.dictionary, vec!["helo".to_string()]);
    let owned = tempfile::tempdir().unwrap();
    let preferences = owned.path().join("isolated-preferences.json");
    let document = owned.path().join("synthetic-text.figcraft");
    let prefs_bytes = serde_json::to_vec(&first.prefs).unwrap();
    std::fs::write(&preferences, &prefs_bytes).unwrap();
    std::fs::write(&document, figcraft_format::save_bytes(first.doc().unwrap(), None).unwrap()).unwrap();
    drop(first);
    let reloaded: crate::Prefs = serde_json::from_slice(&std::fs::read(&preferences).unwrap()).unwrap();
    assert_eq!(reloaded.dictionary, vec!["helo".to_string()]);
    assert!(!std::str::from_utf8(&prefs_bytes).unwrap().contains("wrold"), "ignore is deliberately scoped to a run, not persisted as a learned word");
    let (doc, issues) = figcraft_format::load_bytes(&std::fs::read(&document).unwrap()).unwrap();
    assert!(issues.is_empty());
    let mut restarted = Session::with_document(doc);
    restarted.prefs = reloaded;
    let asked = Arc::new(Mutex::new(0));
    restarted.spelling.set_checker(Box::new(Fake(asked.clone())));
    let text = restarted.doc().unwrap().nodes().find(|node| node.kind.text().is_some()).unwrap().id;
    restarted.execute("text.edit.begin", &json!({"id":text,"caret":0})).unwrap();
    let before = restarted.ds().unwrap().clone();
    restarted.sync_spelling();
    assert_eq!(restarted.misspelled(), &[(6, 11)], "the fresh service flags the run-only ignored word; the persisted user word remains accepted");
    assert!(*asked.lock().unwrap() > 0, "a newly installed checker actually handles this restart");
    assert!(Arc::ptr_eq(&before.doc, &restarted.ds().unwrap().doc));
    assert_eq!(before.revision, restarted.ds().unwrap().revision);
    assert_eq!((before.history.undo.len(), before.history.redo.len()), restarted.undo_depth());
    restarted.execute("text.spelling", &json!({"on":false})).unwrap();
    restarted.sync_spelling();
    assert!(restarted.misspelled().is_empty());
    restarted.execute("text.spelling", &json!({"on":true})).unwrap();
    restarted.sync_spelling();
    assert_eq!(restarted.misspelled(), &[(6, 11)]);
    assert_eq!(
        std::fs::read(&preferences).unwrap(),
        prefs_bytes,
        "checking, ignoring and service replacement never rewrite an unrelated/global preference file"
    );
    assert_eq!(std::fs::read_dir(owned.path()).unwrap().count(), 2, "only the two explicitly owned fixture files exist");
}
