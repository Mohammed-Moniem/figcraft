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

struct ResourceDropSpy(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Drop for ResourceDropSpy {
    fn drop(&mut self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

fn resource_gate(timeout: std::time::Duration) -> (super::RequestGate, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let drops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let gate = super::RequestGate::with_timeout_and_resource(timeout, std::sync::Arc::new(ResourceDropSpy(drops.clone())));
    (gate, drops)
}

fn resource_drops(drops: &std::sync::atomic::AtomicUsize) -> usize {
    drops.load(std::sync::atomic::Ordering::SeqCst)
}

#[test]
fn cancellation_registry_handle_does_not_pin_a_cancelled_payload_resource() {
    let (gate, drops) = resource_gate(std::time::Duration::from_secs(30));
    let registry = gate.cancellation_handle();
    let payload = gate.clone();
    assert_eq!(registry.cancel_if_open(), super::RequestState::Cancelled);
    drop(gate);
    assert_eq!(resource_drops(&drops), 0, "the queued payload still owns its allocations");
    drop(payload);
    assert_eq!(resource_drops(&drops), 1, "an idle registry must not require another registration to refund");
    assert_eq!(registry.state(), super::RequestState::Cancelled, "cancellation state remains usable without payload ownership");
}

#[test]
fn expired_request_keeps_resource_until_its_rejected_payload_is_dropped() {
    let (mut app, back) = two_files();
    let (gate, drops) = resource_gate(std::time::Duration::ZERO);
    let registry = gate.cancellation_handle();
    let (reply, receiver) = std::sync::mpsc::channel();
    let request = super::ControlRequest {
        id: json!(102),
        method: "engine.execute".into(),
        params: json!({"document":back,"command":"shape.rectangle","params":{"width":20,"height":10}}),
        reply,
        state: gate,
    };
    assert_eq!(registry.state(), super::RequestState::Cancelled);
    assert_eq!(resource_drops(&drops), 0, "deadline expiry does not drop a still-owned request");
    let context = egui::Context::default();
    super::route_queued(&mut app, &context, context.viewport_id(), 0.0, request);
    assert_eq!(resource_drops(&drops), 1);
    assert_eq!((layers(&app, 0), layers(&app, 1)), (0, 0));
    assert!(receiver.try_recv().is_err());
    assert!(!registry.start());
}

#[test]
fn timed_out_running_input_retains_resource_in_pending_ack_after_receiver_drop() {
    let (mut app, _) = two_files();
    let (gate, drops) = resource_gate(std::time::Duration::from_secs(30));
    let registry = gate.cancellation_handle();
    let (reply, receiver) = std::sync::mpsc::channel();
    let request = super::ControlRequest {
        id: json!(103),
        method: "ui.input".into(),
        params: json!({"events":[{"text":"pending resource text"}]}),
        reply,
        state: gate,
    };
    let context = egui::Context::default();
    super::route_queued(&mut app, &context, context.viewport_id(), 0.0, request);
    assert_eq!(app.input_acks.len(), 1);
    assert_eq!(registry.cancel_if_open(), super::RequestState::Running);
    drop(receiver);
    assert_eq!(resource_drops(&drops), 0, "transport departure cannot refund admitted UI input");
    drop(app);
    assert_eq!(resource_drops(&drops), 1, "dropping the last pending owner releases once");
}

#[test]
fn cancelled_screenshot_resource_is_released_when_capture_owner_is_removed() {
    let (mut app, _) = two_files();
    let (gate, drops) = resource_gate(std::time::Duration::from_secs(30));
    let registry = gate.cancellation_handle();
    let (reply, receiver) = std::sync::mpsc::channel();
    let context = egui::Context::default();
    let request = super::ControlRequest { id: json!(104), method: "ui.screenshot".into(), params: json!({}), reply, state: gate };
    super::route_queued(&mut app, &context, context.viewport_id(), 0.0, request);
    assert_eq!(app.capture.shots.len(), 1);
    assert_eq!(registry.cancel_if_open(), super::RequestState::Cancelled);
    assert_eq!(resource_drops(&drops), 0, "cancellation state does not remove the capture's ID/path owners");
    super::expire(&mut app, &context, 0.0);
    assert!(app.capture.shots.is_empty());
    assert_eq!(resource_drops(&drops), 1);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn input_resource_covers_font_replay_and_discard_until_presentation() {
    let context = egui::Context::default();
    let mut app = FigApp::new(Session::new(), Box::new(NoServices));
    let (gate, drops) = resource_gate(std::time::Duration::from_secs(30));
    let registry = gate.cancellation_handle();
    let (reply, receiver) = std::sync::mpsc::channel();
    let request = super::ControlRequest {
        id: json!(105),
        method: "ui.input".into(),
        params: json!({"events":[{"text":"resource replay text"}]}),
        reply,
        state: gate,
    };
    super::route_queued(&mut app, &context, context.viewport_id(), 0.0, request);
    assert_eq!(registry.cancel_if_open(), super::RequestState::Running);
    let events = crate::windows::take_input(&mut app, egui::ViewportId::ROOT);
    let screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0)));
    let mut first = context.run_ui(egui::RawInput { screen_rect, events, ..Default::default() }, |ui| app.ui(ui));
    first.textures_delta.clear();
    assert!(!app.control_input.is_empty(), "font installation requeues input for the first drawn frame");
    assert_eq!(app.input_acks.len(), 1);
    assert!(receiver.try_recv().is_err());
    assert_eq!(resource_drops(&drops), 0);

    let events = crate::windows::take_input(&mut app, egui::ViewportId::ROOT);
    let mut passes = 0;
    let mut output = context.run_ui(egui::RawInput { screen_rect, events, ..Default::default() }, |ui| {
        passes += 1;
        if passes == 1 {
            ui.ctx().request_discard("resource lifetime sizing pass");
        }
        app.ui(ui);
        if passes == 1 {
            assert_eq!(resource_drops(&drops), 0, "raw-input and replay storage stays charged through the discarded pass");
            assert!(receiver.try_recv().is_err(), "discarded pass cannot release its pending acknowledgement");
        }
    });
    output.textures_delta.clear();
    assert_eq!(passes, 2);
    assert_eq!(receiver.try_recv().unwrap().id, json!(105));
    assert!(app.input_acks.is_empty() && app.applied_acks.is_empty());
    assert!(context.input(|input| input.raw.events.is_empty() && input.events.is_empty()), "the rerun replaced the first pass's volatile events");
    assert_eq!(resource_drops(&drops), 1, "empty rerun and final acknowledgement release without an extra future frame");
    assert!(app.control_input_resources.is_empty(), "empty viewport holders do not accumulate after replacement");
    assert_eq!(registry.state(), super::RequestState::Running);
}

#[test]
fn input_resource_outlives_ack_until_context_replaces_non_discarded_input() {
    let mut ui = crate::layers_drag_tests::Ui::new(Session::new());
    let (gate, drops) = resource_gate(std::time::Duration::from_secs(30));
    let (reply, receiver) = std::sync::mpsc::channel();
    let request = super::ControlRequest {
        id: json!(106),
        method: "ui.input".into(),
        params: json!({"events":[{"text":"context retained text"}]}),
        reply,
        state: gate,
    };
    super::route_queued(&mut ui.app, &ui.ctx, egui::ViewportId::ROOT, 0.0, request);
    let events = crate::windows::take_input(&mut ui.app, egui::ViewportId::ROOT);
    ui.frame(events);
    assert_eq!(receiver.try_recv().unwrap().id, json!(106));
    assert!(ui.app.input_acks.is_empty() && ui.app.applied_acks.is_empty());
    assert!(ui.ctx.input(|input| input.raw.events.iter().any(|event| matches!(event, egui::Event::Text(_)))));
    assert!(ui.ctx.input(|input| input.events.iter().any(|event| matches!(event, egui::Event::Text(_)))));
    assert_eq!(resource_drops(&drops), 0, "egui retains both event vectors after the reply is sent");
    assert!(crate::windows::take_input(&mut ui.app, egui::ViewportId::ROOT).is_empty());
    assert_eq!(resource_drops(&drops), 0, "the raw hook alone has not replaced Context input");
    ui.frame(vec![]);
    assert_eq!(resource_drops(&drops), 1);
    assert!(ui.app.control_input_resources.is_empty());
}

#[test]
fn closed_viewport_drops_context_events_and_pending_input_resources() {
    let mut session = Session::new();
    session.execute("file.new", &json!({"name":"Front"})).unwrap();
    let mut ui = crate::layers_drag_tests::Ui::new(session);
    ui.ctx.set_embed_viewports(false);
    let window = super::handle(&mut ui.app, &ui.ctx, "window.new", &json!({"index":0})).unwrap()["window"].as_u64().unwrap();
    let viewport = crate::windows::viewport(window);
    let document = ui.app.windows.list.iter().find(|candidate| candidate.id == window).unwrap().docs[0];
    let (gate, drops) = resource_gate(std::time::Duration::from_secs(30));
    let (reply, receiver) = std::sync::mpsc::channel();
    let request = super::ControlRequest {
        id: json!(107),
        method: "ui.input".into(),
        params: json!({"window":window,"events":[{"text":"closed viewport text"}]}),
        reply,
        state: gate,
    };
    crate::windows::as_window(&mut ui.app, window, |app| super::route_queued(app, &ui.ctx, viewport, 0.0, request)).unwrap();
    let events = crate::windows::take_input(&mut ui.app, viewport);
    assert!(!events.is_empty());
    ui.ctx.input_mut_for(viewport, |input| {
        input.raw.events = events.clone();
        input.events = events;
    });
    assert_eq!(resource_drops(&drops), 0);
    let index = ui.app.session.docs.iter().position(|candidate| candidate.uid == document).unwrap();
    ui.app.session.execute("file.close", &json!({"index":index,"force":true})).unwrap();
    crate::windows::begin(&mut ui.app, &ui.ctx);
    assert!(!ui.app.windows.exists(window));
    assert!(ui.ctx.input_for(viewport, |input| input.raw.events.is_empty() && input.events.is_empty()));
    assert_eq!(receiver.try_recv().unwrap().id, json!(107));
    assert_eq!(resource_drops(&drops), 1);
    assert!(ui.app.control_input_resources.is_empty());
    assert!(ui.app.applied_acks.is_empty());
}

#[test]
fn teardown_clears_retained_context_events_before_refunding_resources() {
    struct StopServices(std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl crate::Services for StopServices {
        fn mcp_server_apply(&mut self, enabled: bool, port: u16) {
            assert!(!enabled);
            assert_eq!(port, 0);
            self.0.store(true, std::sync::atomic::Ordering::Release);
        }
    }

    struct ContextDropSpy {
        context: egui::Context,
        drops: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        stopped: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl Drop for ContextDropSpy {
        fn drop(&mut self) {
            assert!(self.stopped.load(std::sync::atomic::Ordering::Acquire), "the transport must stop before its payload resource is refunded");
            assert!(
                self.context.input(|input| input.raw.events.is_empty() && input.events.is_empty()),
                "Context input must go before its resource refund"
            );
            self.drops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    for explicit_shutdown in [false, true] {
        let mut ui = crate::layers_drag_tests::Ui::new(Session::new());
        let context = ui.ctx.clone();
        let drops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        ui.app.services = Box::new(StopServices(stopped.clone()));
        let gate = super::RequestGate::with_timeout_and_resource(
            std::time::Duration::from_secs(30),
            std::sync::Arc::new(ContextDropSpy { context: context.clone(), drops: drops.clone(), stopped: stopped.clone() }),
        );
        let (reply, receiver) = std::sync::mpsc::channel();
        ui.app.control_queue.push(super::ControlRequest {
            id: json!(108),
            method: "ui.input".into(),
            params: json!({"events":[{"text":"teardown retained text"}]}),
            reply,
            state: gate,
        });
        ui.frame(vec![]);
        assert_eq!(ui.app.input_acks.len(), 1, "the actual UI queue admitted the input");
        let events = crate::windows::take_input(&mut ui.app, egui::ViewportId::ROOT);
        ui.frame(events);
        assert_eq!(receiver.try_recv().unwrap().id, json!(108));
        assert!(context.input(|input| input.raw.events.iter().any(|event| matches!(event, egui::Event::Text(_)))));
        assert!(context.input(|input| input.events.iter().any(|event| matches!(event, egui::Event::Text(_)))));
        assert_eq!(resource_drops(&drops), 0);
        assert!(!stopped.load(std::sync::atomic::Ordering::Acquire));
        if explicit_shutdown {
            ui.app.release_control_input_resources();
            assert_eq!(resource_drops(&drops), 1, "native exit can release without running Drop");
        }
        drop(ui.app);
        assert!(
            context.input(|input| input.raw.events.is_empty() && input.events.is_empty()),
            "the Context survives the app without retaining control text"
        );
        assert_eq!(resource_drops(&drops), 1, "explicit cleanup and Drop are idempotent");
        drop(context);
        assert_eq!(resource_drops(&drops), 1);
    }
}

#[test]
fn release_control_input_stops_service_before_cancelling_and_refunding_a_queued_request() {
    struct StopOrderServices {
        gate: super::RequestGate,
        observed: std::sync::Arc<std::sync::Mutex<Vec<super::RequestState>>>,
        stopped: std::sync::Arc<std::sync::atomic::AtomicBool>,
        saves: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl crate::Services for StopOrderServices {
        fn mcp_server_apply(&mut self, enabled: bool, port: u16) {
            assert!(!enabled);
            assert_eq!(port, 0);
            self.observed.lock().unwrap().push(self.gate.state());
            self.stopped.store(true, std::sync::atomic::Ordering::Release);
        }

        fn save_prefs(&mut self, _prefs: &figcraft_engine::Prefs) {
            self.saves.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    struct StoppedDropSpy {
        stopped: std::sync::Arc<std::sync::atomic::AtomicBool>,
        drops: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl Drop for StoppedDropSpy {
        fn drop(&mut self) {
            assert!(self.stopped.load(std::sync::atomic::Ordering::Acquire), "stop must precede the final queued payload drop");
            self.drops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    let observed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let drops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let saves = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let gate = super::RequestGate::with_timeout_and_resource(
        std::time::Duration::from_secs(30),
        std::sync::Arc::new(StoppedDropSpy { stopped: stopped.clone(), drops: drops.clone() }),
    );
    let watch = gate.cancellation_handle();
    let services = StopOrderServices { gate: watch.clone(), observed: observed.clone(), stopped, saves: saves.clone() };
    let mut app = FigApp::new(Session::new(), Box::new(services));
    app.session.prefs.mcp_enabled = true;
    app.session.prefs.mcp_port = 47800;
    let prefs = app.session.prefs.clone();
    let (reply, receiver) = std::sync::mpsc::channel();
    app.control_queue.push(super::ControlRequest {
        id: json!(109),
        method: "engine.execute".into(),
        params: json!({"command":"file.new","params":{"name":"never dispatched"}}),
        reply,
        state: gate,
    });
    app.release_control_input_resources();
    assert_eq!(*observed.lock().unwrap(), vec![super::RequestState::Open], "service stop must run before UI cancellation/payload cleanup");
    assert_eq!(watch.state(), super::RequestState::Cancelled);
    assert_eq!(resource_drops(&drops), 1);
    assert!(app.control_queue.is_empty());
    assert!(matches!(receiver.try_recv(), Err(std::sync::mpsc::TryRecvError::Disconnected)));
    assert!(app.session.prefs == prefs, "stopping transport for teardown must not change saved preferences");
    assert_eq!(saves.load(std::sync::atomic::Ordering::SeqCst), 0);
    drop(app);
    assert_eq!(*observed.lock().unwrap(), vec![super::RequestState::Open, super::RequestState::Cancelled]);
    assert_eq!(resource_drops(&drops), 1, "the later Drop cannot refund twice");
    assert_eq!(saves.load(std::sync::atomic::Ordering::SeqCst), 0);
}
