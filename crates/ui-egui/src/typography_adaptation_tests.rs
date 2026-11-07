//! Local adaptations beyond the older upstream spell checker and newer-base RTL code.
use figcraft_engine::Session;
use serde_json::json;
#[test]
fn spelling_context_is_discarded_after_text_edits_tab_changes_and_edit_end() {
    let mut s = Session::new();
    let id = figcraft_doc::NodeId(s.execute("node.create", &json!({"type":"text","text":"sentense"})).unwrap()["id"].as_u64().unwrap());
    figcraft_engine::textedit::begin(&mut s, id, None, false).unwrap();
    let mut ui = crate::layers_drag_tests::Ui::new(s);
    let pos = egui::pos2(400.0, 200.0);
    let ds = ui.app.session.ds().unwrap();
    ui.app.ui.context_spelling = Some(crate::SpellingContext {
        position: pos,
        document_uid: ds.uid,
        layer: id,
        snapshot: ds.doc.clone(),
        index: 0,
        word: "sentense".into(),
        suggestions: vec!["sentence".into()],
    });
    assert!(crate::menus::spelling_context_current(&ui.app, pos));
    assert!(!crate::menus::spelling_context_current(&ui.app, pos + egui::vec2(1.0, 0.0)));
    figcraft_engine::textedit::insert(&mut ui.app.session, "x").unwrap();
    assert!(!crate::menus::spelling_context_current(&ui.app, pos), "a text edit invalidates captured suggestions");
    let ds = ui.app.session.ds().unwrap();
    ui.app.ui.context_spelling = Some(crate::SpellingContext {
        position: pos,
        document_uid: ds.uid,
        layer: id,
        snapshot: ds.doc.clone(),
        index: 0,
        word: "sentense".into(),
        suggestions: vec![],
    });
    ui.app.session.execute("file.new", &json!({})).unwrap();
    assert!(!crate::menus::spelling_context_current(&ui.app, pos), "another tab cannot consume old suggestions");
}
