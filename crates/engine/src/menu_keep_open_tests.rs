use crate::{Prefs, Session};
use serde_json::json;

#[test]
fn menu_keep_open_preference_is_opt_in_saved_and_independent_of_the_document() {
    let mut s = Session::new();
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    assert_eq!(serde_json::to_value(Prefs::default()).unwrap()["keep_menu_open"], false);
    assert_eq!(serde_json::to_value(serde_json::from_value::<Prefs>(json!({})).unwrap()).unwrap()["keep_menu_open"], false);
    s.prefs_unsaved = false;
    assert_eq!(s.execute("pref.keepMenuOpen", &json!({"on":true})).unwrap(), json!({"on":true}));
    assert!(s.prefs_unsaved);
    let saved = serde_json::to_value(&s.prefs).unwrap();
    assert_eq!(saved["keep_menu_open"], true);
    assert_eq!(serde_json::to_value(serde_json::from_value::<Prefs>(saved).unwrap()).unwrap()["keep_menu_open"], true);
    s.prefs_unsaved = false;
    s.execute("pref.keepMenuOpen", &json!({"on":true})).unwrap();
    assert!(!s.prefs_unsaved);
    assert_eq!(s.execute("pref.keepMenuOpen", &json!({})).unwrap(), json!({"on":false}));
    assert!(s.prefs_unsaved);
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), history);
    assert!(!s.ds().unwrap().is_dirty());
}

#[test]
fn menu_keep_open_invalid_input_leaves_preference_and_document_unchanged() {
    let mut s = Session::new();
    s.execute("pref.keepMenuOpen", &json!({"on":true})).unwrap();
    for input in [json!({"on":null}), json!({"on":[]}), json!({"on":{}})] {
        let before = s.prefs.clone();
        s.prefs_unsaved = false;
        assert!(s.execute("pref.keepMenuOpen", &input).is_err());
        assert_eq!(s.prefs, before);
        assert!(!s.prefs_unsaved);
    }
}
