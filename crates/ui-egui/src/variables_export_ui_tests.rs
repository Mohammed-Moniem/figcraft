//! Variables ▸ Export… where the platform saves by downloads (the browser, which has no save
//! dialogs): the export arrives as a download instead of doing nothing.

use std::sync::{Arc, Mutex};

use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::variables_panel::export_dialog;
use crate::{FigApp, NoServices, Services};

pub(crate) type Downloads = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// Services that save by downloads and keep them (`deliver` false: the browser refused).
pub(crate) struct Browser {
    pub(crate) got: Downloads,
    pub(crate) deliver: bool,
}

impl Services for Browser {
    fn downloads(&self) -> bool {
        true
    }
    fn download(&mut self, name: &str, bytes: &[u8]) -> bool {
        self.got.lock().unwrap().push((name.to_string(), bytes.to_vec()));
        self.deliver
    }
}

fn session() -> Session {
    let mut s = Session::new();
    for (cmd, p) in [
        ("variable.createCollection", json!({"name": "Theme", "modes": ["Light", "Dark"]})),
        (
            "variable.create",
            json!({"collection": "Theme", "name": "color/primary", "type": "color", "values": {"Light": "#0000ff", "Dark": "#ffffff"}}),
        ),
        ("variable.create", json!({"collection": "Theme", "name": "radius", "type": "number", "value": 4})),
    ] {
        s.execute(cmd, &p).unwrap();
    }
    s
}

fn browser(deliver: bool) -> (FigApp, Downloads) {
    let got = Downloads::default();
    (FigApp::new(session(), Box::new(Browser { got: got.clone(), deliver })), got)
}

fn toast(app: &FigApp) -> String {
    app.ui.toast.as_ref().map(|t| t.0.clone()).unwrap_or_default()
}

#[test]
fn json_export_downloads_what_variable_export_writes() {
    let (mut app, got) = browser(true);
    let expected = app.session.execute("variable.export", &json!({"format": "json"})).unwrap()["text"].as_str().unwrap().to_string();
    export_dialog(&mut app, "json");
    let got = got.lock().unwrap();
    let [(name, bytes)] = got.as_slice() else { panic!("one download, got {}", got.len()) };
    assert!(name.ends_with(".variables.json"), "{name}");
    assert_eq!(String::from_utf8(bytes.clone()).unwrap(), expected);
    let parsed: Value = serde_json::from_slice(bytes).unwrap();
    assert!(parsed.is_object());
    assert!(toast(&app).contains(name.as_str()), "{}", toast(&app));
}

#[test]
fn css_export_downloads_custom_properties() {
    let (mut app, got) = browser(true);
    export_dialog(&mut app, "css");
    let got = got.lock().unwrap();
    let [(name, bytes)] = got.as_slice() else { panic!("one download, got {}", got.len()) };
    assert!(name.ends_with(".variables.css"), "{name}");
    let css = String::from_utf8(bytes.clone()).unwrap();
    assert!(css.contains(":root") && css.contains("--"), "{css}");
}

#[test]
fn dart_export_remains_available_in_the_rewritten_menu() {
    use crate::layers_drag_tests::Ui;
    use crate::live_tests::{flat, text_center};

    let got = Downloads::default();
    let mut u = Ui::new(session());
    let expected = u.app.session.execute("variable.export", &json!({"format": "dart"})).unwrap()["text"].as_str().unwrap().to_string();
    u.app.services = Box::new(Browser { got: got.clone(), deliver: true });
    u.app.ui.variables = Some(Default::default());
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let opener = text_center(&shapes, "Export…");
    u.button(opener, true);
    u.button(opener, false);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let item = text_center(&shapes, "Dart constants for Flutter…");
    u.button(item, true);
    u.button(item, false);
    let got = got.lock().unwrap();
    let [(name, bytes)] = got.as_slice() else { panic!("one download from the menu, got {}", got.len()) };
    assert!(name.ends_with(".variables.dart"), "{name}");
    assert_eq!(std::str::from_utf8(bytes).unwrap(), expected);
    assert!(expected.contains("class Theme") && expected.contains("Color("), "{expected}");
}

#[test]
fn dtcg_export_menu_downloads_tokens_that_import_with_both_modes() {
    use crate::layers_drag_tests::Ui;
    use crate::live_tests::{flat, text_center};
    let got = Downloads::default();
    let mut u = Ui::new(session());
    u.app.services = Box::new(Browser { got: got.clone(), deliver: true });
    u.app.ui.variables = Some(Default::default());
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let opener = text_center(&shapes, "Export…");
    u.button(opener, true);
    u.button(opener, false);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let item = text_center(&shapes, "DTCG design tokens…");
    u.button(item, true);
    u.button(item, false);
    let got = got.lock().unwrap();
    let [(name, bytes)] = got.as_slice() else { panic!("one download from the menu") };
    assert!(name.ends_with(".tokens.json"), "{name}");
    let tokens: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(tokens["Theme"]["radius"]["$type"], "dimension");
    let mut target = Session::new();
    target.execute("variable.import", &json!({"variables": tokens})).unwrap();
    let vars = &target.doc().unwrap().variables;
    assert_eq!(vars.variables.len(), 2);
    assert_eq!(vars.collections[0].modes.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Light", "Dark"]);
}

#[test]
fn a_refused_download_says_so() {
    let (mut app, got) = browser(false);
    export_dialog(&mut app, "json");
    assert_eq!(got.lock().unwrap().len(), 1);
    assert_eq!(toast(&app), "Couldn't download the variables");
}

#[test]
fn platforms_with_save_dialogs_keep_them() {
    // No dialog answer (cancelled) and no downloads: nothing happens, as before.
    let mut app = FigApp::new(session(), Box::new(NoServices));
    export_dialog(&mut app, "json");
    assert!(app.ui.toast.is_none());
}

struct ImportFile {
    path: Option<std::path::PathBuf>,
    bytes: Vec<u8>,
}

impl Services for ImportFile {
    fn pick_open(&mut self, title: &str, exts: &[&str]) -> crate::DialogFuture {
        assert_eq!(title, "Import variables");
        assert_eq!(exts, ["json"]);
        Box::pin(std::future::ready(Some(self.path.clone().expect("browser must pick contents"))))
    }
    fn pick_open_contents(&mut self, title: &str, exts: &[&str]) -> Option<crate::ContentsFuture> {
        assert_eq!(title, "Import variables");
        assert_eq!(exts, ["json"]);
        self.path.is_none().then(|| Box::pin(std::future::ready(Some(Ok(("theme.tokens.json".into(), self.bytes.clone()))))) as crate::ContentsFuture)
    }
}

fn click_import(u: &mut crate::layers_drag_tests::Ui) {
    use crate::live_tests::{flat, text_center};
    u.app.ui.variables = Some(Default::default());
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let button = text_center(&shapes, "Import…");
    u.button(button, true);
    u.button(button, false);
    u.app.poll_dialog(&u.ctx);
    u.frame(vec![]);
}

#[test]
fn dtcg_import_button_reads_native_picker_and_browser_file_input() {
    let exported = session().execute("variable.export", &json!({"format":"dtcg"})).unwrap()["text"].as_str().unwrap().as_bytes().to_vec();
    let dir = std::env::temp_dir().join(format!("figcraft-dtcg-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("theme.tokens.json");
    std::fs::write(&path, &exported).unwrap();
    for browser in [false, true] {
        let mut u = crate::layers_drag_tests::Ui::new(Session::new());
        u.app.services = Box::new(ImportFile { path: (!browser).then(|| path.clone()), bytes: exported.clone() });
        click_import(&mut u);
        assert!(u.app.ui.last_error.is_none(), "{:?}", u.app.ui.last_error);
        let vars = &u.app.session.doc().unwrap().variables;
        assert_eq!(vars.variables.len(), 2);
        assert_eq!(vars.collections[0].modes.len(), 2);
        assert!(toast(&u.app).contains("Imported 2 new variables"), "{}", toast(&u.app));
        u.app.session.execute("edit.undo", &json!({})).unwrap();
        assert!(u.app.session.doc().unwrap().variables.variables.is_empty());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn dtcg_import_button_reports_malformed_browser_input_without_changes() {
    let mut u = crate::layers_drag_tests::Ui::new(Session::new());
    u.app.services = Box::new(ImportFile { path: None, bytes: b"{".to_vec() });
    let before = u.app.session.doc().unwrap().clone();
    click_import(&mut u);
    assert!(u.app.ui.last_error.as_deref().is_some_and(|e| e.contains("variables")));
    assert_eq!(u.app.session.doc().unwrap(), &before);
}

#[test]
fn dtcg_typed_variables_keep_their_type_through_editor_copy_paste_and_undo() {
    use crate::live_tests::{flat, text_center};
    use figcraft_doc::DesignTokenType;
    for (ty, value) in [("number", json!(42)), ("fontFamily", json!("Sample Sans"))] {
        let mut source = Session::new();
        source.execute("variable.import", &json!({"variables":{"Theme":{"token":{"$type":ty,"$value":value}}}})).unwrap();
        let mut u = crate::layers_drag_tests::Ui::new(source);
        u.app.ui.variables = Some(Default::default());
        for _ in 0..3 {
            u.frame(vec![]);
        }
        let shapes = flat(&u.frame_output(vec![]).shapes);
        let row = text_center(&shapes, "token");
        u.button(row, true);
        u.button(row, false);
        u.frame(vec![egui::Event::Copy]);
        assert!(u.app.ui.variables.as_ref().unwrap().copied.is_some());
        let depth = u.app.session.undo_depth().0;
        u.frame(vec![egui::Event::Paste(String::new())]);
        assert!(u.app.ui.last_error.is_none(), "{:?}", u.app.ui.last_error);
        let copy = u.app.session.doc().unwrap().variables.get("token copy").unwrap();
        assert_eq!(copy.design_token_type, DesignTokenType::parse(ty), "clipboard keeps {ty} provenance");
        assert_eq!(copy.scopes, None);
        let exported = u.app.session.execute("variable.export", &json!({"format":"dtcg"})).unwrap();
        let tokens: Value = serde_json::from_str(exported["text"].as_str().unwrap()).unwrap();
        assert_eq!(tokens["Theme"]["token copy"]["$type"], ty);
        assert_eq!(u.app.session.undo_depth().0, depth + 1, "paste is one undo step");
        u.app.session.execute("edit.undo", &json!({})).unwrap();
        assert!(u.app.session.doc().unwrap().variables.get("token copy").is_none());
        u.app.session.execute("edit.redo", &json!({})).unwrap();
        assert_eq!(u.app.session.doc().unwrap().variables.get("token copy").unwrap().design_token_type, DesignTokenType::parse(ty));
    }
}

struct PendingImport {
    bytes: Arc<Mutex<Option<Vec<u8>>>>,
}

impl Services for PendingImport {
    fn pick_open_contents(&mut self, _: &str, _: &[&str]) -> Option<crate::ContentsFuture> {
        let bytes = self.bytes.clone();
        Some(Box::pin(std::future::poll_fn(move |_| match bytes.lock().unwrap().take() {
            Some(bytes) => std::task::Poll::Ready(Some(Ok(("theme.tokens.json".into(), bytes)))),
            None => std::task::Poll::Pending,
        })))
    }
}

#[test]
fn dtcg_delayed_import_stays_with_its_owner_and_drops_if_that_file_closes() {
    for close_owner in [false, true] {
        let bytes = Arc::new(Mutex::new(None));
        let mut u = crate::layers_drag_tests::Ui::new(Session::new());
        u.app.services = Box::new(PendingImport { bytes: bytes.clone() });
        let owner = u.app.session.ds().unwrap().uid;
        click_import(&mut u);
        assert!(u.app.pending_dialog.is_some());
        u.app.run("file.new", json!({})).unwrap();
        let other = u.app.session.ds().unwrap().uid;
        assert_ne!(owner, other);
        if close_owner {
            u.app.run("file.close", json!({"index":0,"force":true})).unwrap();
        }
        *bytes.lock().unwrap() = Some(serde_json::to_vec(&json!({"Theme":{"count":{"$type":"number","$value":42}}})).unwrap());
        u.app.poll_dialog(&u.ctx);
        assert!(u.app.pending_dialog.is_none());
        assert_eq!(u.app.session.ds().unwrap().uid, other, "the user's front tab stays in front");
        assert!(u.app.session.doc().unwrap().variables.variables.is_empty(), "the other tab never receives the import");
        if !close_owner {
            let original = u.app.session.docs.iter().find(|d| d.uid == owner).unwrap();
            assert_eq!(original.doc.variables.get("count").unwrap().design_token_type, Some(figcraft_doc::DesignTokenType::Number));
        }
    }
}
