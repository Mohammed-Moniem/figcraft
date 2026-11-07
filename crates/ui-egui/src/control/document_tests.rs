//! A control request naming its `document` acts on that file, and the tab in front stays in front.

use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::{FigApp, NoServices};

/// Two files open: "Back" behind "Front", which is in front. Returns the app and Back's id.
fn two_files() -> (FigApp, u64) {
    let mut s = Session::new();
    s.execute("file.new", &json!({"name": "Front"})).unwrap();
    s.docs[0].doc_mut().name = "Back".into();
    let back = s.docs[0].uid;
    (FigApp::new(s, Box::new(NoServices)), back)
}

fn handle(app: &mut FigApp, method: &str, p: Value) -> Result<Value, String> {
    super::handle(app, &egui::Context::default(), method, &p)
}

fn layers(app: &FigApp, i: usize) -> usize {
    let ds = &app.session.docs[i];
    ds.doc.node(ds.page).map_or(0, |p| p.children.len())
}

#[test]
fn a_call_naming_the_back_file_edits_it_and_leaves_the_front_one_alone() {
    let (mut app, back) = two_files();
    let rect = json!({"command": "shape.rectangle", "params": {"x": 0, "y": 0, "width": 20, "height": 10}, "document": back});
    handle(&mut app, "engine.execute", rect).unwrap();
    assert_eq!((layers(&app, 0), layers(&app, 1)), (1, 0), "the rectangle landed in Back only");
    assert_eq!(app.session.active, 1, "Front stays in front");
    let doc = handle(&mut app, "document.inspect", json!({"depth": 1, "document": back})).unwrap();
    assert_eq!((doc["name"].as_str(), doc["id"].as_u64()), (Some("Back"), Some(back)));
    let listed = handle(&mut app, "engine.execute", json!({"command": "doc.list"})).unwrap();
    assert_eq!(listed[0]["id"], json!(back));
    assert!(handle(&mut app, "ui.render", json!({"document": back})).is_ok());
    // An open file's path names it too.
    app.session.docs[0].path = Some("/work/back.figcraft".into());
    let doc = handle(&mut app, "document.inspect", json!({"depth": 0, "document": "/work/back.figcraft"})).unwrap();
    assert_eq!(doc["id"], json!(back));
    assert_eq!(app.session.active, 1);
}

#[test]
fn a_file_that_is_not_open_or_a_ui_method_on_the_back_file_is_an_error() {
    let (mut app, back) = two_files();
    let e = handle(&mut app, "document.inspect", json!({"document": 999_999})).unwrap_err();
    assert!(e.contains("no open document 999999"), "{e}");
    let e = handle(&mut app, "ui.type", json!({"text": "x", "document": back})).unwrap_err();
    assert!(e.contains("acts on the tab in front"), "{e}");
    let front = app.session.docs[1].uid;
    assert!(handle(&mut app, "ui.inspect", json!({"document": front})).is_ok(), "the file in front may be named");
    assert_eq!(app.session.active, 1);
}

#[test]
fn a_command_that_opens_a_file_still_brings_it_to_the_front() {
    let (mut app, back) = two_files();
    handle(&mut app, "engine.execute", json!({"command": "file.new", "document": back})).unwrap();
    assert_eq!(app.session.active, 2, "a new file opens in front");
}

#[test]
fn conditional_matching_revision_applies_once_and_stale_retry_has_no_effect() {
    let (mut app, back) = two_files();
    let revision = app.session.docs[0].revision;
    let request =
        json!({"document":back,"command":"shape.rectangle","params":{"width":20,"height":10},"expected":{"document":back,"revision":revision}});
    handle(&mut app, "engine.execute", request.clone()).unwrap();
    let before = app.session.docs[0].doc.clone();
    let before_revision = app.session.docs[0].revision;
    let before_history = app.session.docs[0].history.undo.len();
    let before_selection = app.session.docs[0].selection.clone();
    let before_events = app.session.events.latest();
    let error = handle(&mut app, "engine.execute", request).unwrap_err();
    assert!(error.starts_with("CONTROL_REVISION_CONFLICT:"), "{error}");
    assert_eq!(app.session.docs[0].doc, before);
    assert_eq!(app.session.docs[0].revision, before_revision);
    assert_eq!(app.session.docs[0].history.undo.len(), before_history);
    assert_eq!(app.session.docs[0].selection, before_selection);
    assert_eq!(app.session.events.latest(), before_events);
    assert_eq!((layers(&app, 0), layers(&app, 1), app.session.active), (1, 0, 1));
}

#[test]
fn conditional_wrong_document_never_edits_either_open_file() {
    let (mut app, back) = two_files();
    let front = app.session.docs[1].uid;
    let before_back = app.session.docs[0].doc.clone();
    let before_front = app.session.docs[1].doc.clone();
    let before_revision = app.session.docs[0].revision;
    let error = handle(&mut app, "engine.execute", json!({"document":back,"command":"shape.rectangle","params":{"width":20,"height":10},"expected":{"document":front,"revision":before_revision}})).unwrap_err();
    assert!(error.starts_with("CONTROL_DOCUMENT_CONFLICT:"), "{error}");
    assert_eq!(app.session.docs[0].doc, before_back);
    assert_eq!(app.session.docs[1].doc, before_front);
    assert_eq!(app.session.docs[0].revision, before_revision);
    assert_eq!((layers(&app, 0), layers(&app, 1), app.session.active), (0, 0, 1));
}

#[test]
fn conditional_malformed_conditions_fail_closed_before_mutation() {
    for expected in [
        json!(null),
        json!([]),
        json!({}),
        json!({"document":1}),
        json!({"document":1,"revision":-1}),
        json!({"document":1.5,"revision":0}),
        json!({"document":1,"revision":0,"extra":true}),
    ] {
        let (mut app, back) = two_files();
        let before = app.session.docs[0].doc.clone();
        let revision = app.session.docs[0].revision;
        let error = handle(
            &mut app,
            "engine.execute",
            json!({"document":back,"command":"shape.rectangle","params":{"width":20,"height":10},"expected":expected}),
        )
        .unwrap_err();
        assert!(error.starts_with("CONTROL_INVALID_EXPECTED:"), "{error}");
        assert_eq!(app.session.docs[0].doc, before);
        assert_eq!(app.session.docs[0].revision, revision);
        assert_eq!((layers(&app, 0), layers(&app, 1), app.session.active), (0, 0, 1));
    }
}

#[test]
fn conditional_inspection_advertises_the_live_process_token() {
    let (mut app, back) = two_files();
    let revision = app.session.docs[0].revision;
    let inspected = handle(&mut app, "document.inspect", json!({"document":back,"depth":0})).unwrap();
    assert_eq!(inspected["revision"], revision);
    assert_eq!(
        inspected["capabilities"]["conditionalExecute"],
        json!({"version":1,"expected":["document","revision"],"revisionScope":"live-process"})
    );
    assert_eq!(app.session.active, 1);
}

#[test]
fn conditional_inspected_revision_survives_host_document_list_heartbeats() {
    let (mut app, back) = two_files();
    let inspected = handle(&mut app, "document.inspect", json!({"document":back,"depth":0})).unwrap();
    for _ in 0..4 {
        handle(&mut app, "engine.execute", json!({"command":"doc.list","params":{}})).unwrap();
    }
    handle(&mut app, "engine.execute", json!({"document":back,"command":"shape.rectangle","params":{"width":20,"height":10},"expected":{"document":back,"revision":inspected["revision"]}})).unwrap();
    assert_eq!((layers(&app, 0), layers(&app, 1), app.session.active), (1, 0, 1));
}

#[test]
fn conditional_closed_target_is_a_known_refusal_and_bad_guards_resolve_nothing() {
    let (mut app, back) = two_files();
    let revision = app.session.docs[0].revision;
    app.session.execute("file.close", &json!({"index":0,"force":true})).unwrap();
    let before = app.session.docs[0].doc.clone();
    let before_revision = app.session.docs[0].revision;
    let before_events = app.session.events.latest();
    let request =
        json!({"document":back,"command":"shape.rectangle","params":{"width":20,"height":10},"expected":{"document":back,"revision":revision}});
    let error = handle(&mut app, "engine.execute", request).unwrap_err();
    assert!(error.starts_with("CONTROL_DOCUMENT_CONFLICT:"), "{error}");
    let malformed =
        handle(&mut app, "engine.execute", json!({"document":back,"command":"shape.rectangle","params":{"width":20,"height":10},"expected":null}))
            .unwrap_err();
    assert!(malformed.starts_with("CONTROL_INVALID_EXPECTED:"), "malformed guard must fail before closed-target lookup: {malformed}");
    assert_eq!(app.session.docs[0].doc, before);
    assert_eq!(app.session.docs[0].revision, before_revision);
    assert_eq!(app.session.events.latest(), before_events);
    assert_eq!(app.session.active, 0);
}

#[test]
fn page_deadline_gate_refuses_late_admission_and_commit_without_a_timer() {
    let (mut app, back) = two_files();
    let gate = super::RequestGate::with_timeout(std::time::Duration::ZERO);
    let (reply, receiver) = std::sync::mpsc::channel();
    let request = super::ControlRequest {
        id: json!(100),
        method: "engine.execute".into(),
        params: json!({"document":back,"command":"shape.rectangle","params":{"width":20,"height":10}}),
        reply,
        state: gate.clone(),
    };
    let context = egui::Context::default();
    super::route_queued(&mut app, &context, context.viewport_id(), 0.0, request);
    assert_eq!((layers(&app, 0), layers(&app, 1)), (0, 0));
    assert_eq!(gate.state(), super::RequestState::Cancelled);
    assert!(!gate.start());
    assert!(gate.commit(|| 42).is_none());
    assert!(receiver.try_recv().is_err());
}

#[test]
fn page_admitted_input_is_running_until_its_later_frame_acknowledgement() {
    let (mut app, _) = two_files();
    let gate = super::RequestGate::with_timeout(std::time::Duration::from_secs(30));
    let (reply, receiver) = std::sync::mpsc::channel();
    let request =
        super::ControlRequest { id: json!(101), method: "ui.input".into(), params: json!({"events":[{"key":"Delete"}]}), reply, state: gate.clone() };
    let context = egui::Context::default();
    super::route_queued(&mut app, &context, context.viewport_id(), 0.0, request);
    assert!(!app.control_input.is_empty());
    assert_eq!(app.input_acks.len(), 1);
    assert!(receiver.try_recv().is_err(), "input acknowledgement still waits for its presented frame");
    assert_eq!(gate.cancel_if_open(), super::RequestState::Running, "queued UI effects cannot be retracted by ending a transport wait");
}
