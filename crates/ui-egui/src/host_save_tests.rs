//! In a page host the browser hands files to the host: a Save of the document the host opened goes
//! out under the host's file name, a Save As under the document's own name.

use std::sync::{Arc, Mutex};

use figcraft_engine::Session;
use serde_json::json;

use crate::{DocumentSave, FigApp, SaveDelivery, Services};

type Saved = Arc<Mutex<Vec<String>>>;

/// Services that save by downloads, with a document file the host opened.
struct Host {
    got: Saved,
    file: Option<&'static str>,
}

impl Services for Host {
    fn downloads(&self) -> bool {
        true
    }
    fn download(&mut self, name: &str, _bytes: &[u8]) -> bool {
        self.got.lock().unwrap().push(name.to_string());
        true
    }
    fn host_file(&self) -> Option<String> {
        self.file.map(str::to_string)
    }
}

fn app(file: Option<&'static str>) -> (FigApp, Saved) {
    let got = Saved::default();
    let mut app = FigApp::new(Session::empty(), Box::new(Host { got: got.clone(), file }));
    app.session.execute("file.new", &json!({"name": "Original"})).unwrap();
    (app, got)
}

#[test]
fn save_keeps_the_name_the_host_opened() {
    let (mut app, got) = app(Some("client.figcraft"));
    app.save();
    assert_eq!(got.lock().unwrap().as_slice(), ["client.figcraft"]);
}

#[test]
fn save_as_uses_the_document_s_own_name() {
    let (mut app, got) = app(Some("client.figcraft"));
    app.save_as_dialog();
    assert_eq!(got.lock().unwrap().as_slice(), ["Original.figcraft"]);
}

#[test]
fn without_a_host_file_or_for_a_copy_format_save_is_a_save_as() {
    for file in [None, Some("design.json"), Some("export.fig")] {
        let (mut app, got) = app(file);
        app.save();
        assert_eq!(got.lock().unwrap().as_slice(), ["Original.figcraft"], "{file:?}");
    }
}

#[derive(Default)]
struct AckState {
    posted: Vec<DocumentSave>,
    acknowledged: Vec<DocumentSave>,
    fail: bool,
}

struct AcknowledgedHost(Arc<Mutex<AckState>>);

impl Services for AcknowledgedHost {
    fn downloads(&self) -> bool {
        true
    }
    fn save_document(&mut self, snapshot: DocumentSave, _: &str, _: &[u8]) -> SaveDelivery {
        let mut state = self.0.lock().unwrap();
        if state.fail {
            return SaveDelivery::Failed;
        }
        state.posted.push(snapshot);
        SaveDelivery::Pending
    }
    fn saved_documents(&mut self) -> Vec<DocumentSave> {
        std::mem::take(&mut self.0.lock().unwrap().acknowledged)
    }
}

#[test]
fn host_ack_ui_waits_for_durable_ack_and_keeps_explicit_save_as_distinct() {
    let state = Arc::new(Mutex::new(AckState::default()));
    let mut app = FigApp::new(Session::new(), Box::new(AcknowledgedHost(state.clone())));
    app.session.execute("file.rename", &json!({"name": "Changed"})).unwrap();
    app.save();
    assert!(app.session.docs[0].is_dirty());
    let save = state.lock().unwrap().posted[0];
    assert!(!save.save_as, "ordinary Save lets the host resolve the UID mapping");
    app.save_as_dialog();
    assert!(state.lock().unwrap().posted[1].save_as);
    app.session.execute("file.new", &json!({"name": "Other"})).unwrap();
    state.lock().unwrap().acknowledged.push(save);
    app.acknowledged_saves();
    assert!(!app.session.docs[0].is_dirty());
}

#[test]
fn host_ack_ui_failure_or_ack_after_edit_does_not_clear_dirty() {
    let state = Arc::new(Mutex::new(AckState { fail: true, ..Default::default() }));
    let mut app = FigApp::new(Session::new(), Box::new(AcknowledgedHost(state.clone())));
    app.session.execute("file.rename", &json!({"name": "Changed"})).unwrap();
    app.save();
    assert!(app.session.ds().unwrap().is_dirty());
    state.lock().unwrap().fail = false;
    app.save();
    let save = state.lock().unwrap().posted[0];
    app.session.execute("file.rename", &json!({"name": "Edited while pending"})).unwrap();
    state.lock().unwrap().acknowledged.push(save);
    app.acknowledged_saves();
    assert!(app.session.ds().unwrap().is_dirty());
}
