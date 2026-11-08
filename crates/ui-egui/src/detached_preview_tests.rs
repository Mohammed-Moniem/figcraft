//! Original synthetic preview UI: the player is painted before its header is used.
use crate::present_tests::{Ui, setup};
use egui::{Event, PointerButton, Pos2, Rect, pos2};
use serde_json::json;

fn button(out: &egui::FullOutput, label: &str) -> Option<Rect> {
    out.platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find(|(_, n)| n.role() == egui::accesskit::Role::Button && n.label() == Some(label))
        .and_then(|(_, n)| n.bounds())
        .map(|r| Rect::from_min_max(pos2(r.x0 as f32, r.y0 as f32), pos2(r.x1 as f32, r.y1 as f32)))
}

fn click(u: &mut Ui, at: Pos2) -> egui::FullOutput {
    let event = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
    u.frame(vec![Event::PointerMoved(at), event(true)]);
    u.frame_output(vec![Event::PointerMoved(at), event(false)])
}

#[test]
fn detached_preview_header_opens_a_native_viewport_without_moving_the_editor() {
    let (mut u, a, _, _) = setup();
    u.ctx.set_embed_viewports(false);
    u.run("proto.preview", json!({"start": a}));
    let out = u.frame_output(vec![]);
    let pane = u.pane();
    assert_eq!(u.preview()["current"], json!(a));
    assert!(out.shapes.len() > 20, "the editor and prototype are painted, not just a request");
    let document = u.app.session.doc().unwrap().clone();
    let uid = u.app.session.ds().unwrap().uid;
    let files = u.app.session.docs.len();
    // AccessKit requests activate this project's screen-reader guard, so this native-capable
    // pointer case draws without it. The separate inline control checks accessible header names.
    // Click the fourth compact header slot, absent in the original player.
    let at = pos2(pane.right() - 96.0, pane.top() + 18.0);
    click(&mut u, at);
    let after = u.frame_output(vec![]);
    let children: Vec<_> = after.viewport_output.iter().filter(|(id, _)| **id != egui::ViewportId::ROOT).collect();
    assert_eq!(children.len(), 1, "native preview missing after actual rendered header click");
    let child = children[0].1;
    assert_eq!(child.builder.decorations, Some(true));
    assert_eq!(child.builder.resizable, Some(true));
    assert!(child.viewport_ui_cb.is_some(), "a real independently drawn deferred viewport");
    assert!(u.app.ui.preview.is_none(), "the existing player transferred out of the inline pane");
    assert_eq!(u.app.session.docs.len(), files);
    assert_eq!(u.app.session.ds().unwrap().uid, uid);
    assert_eq!(u.app.session.doc().unwrap().nodes().collect::<Vec<_>>(), document.nodes().collect::<Vec<_>>());
}

#[test]
fn detached_preview_control_inline_close_and_present_keep_working() {
    let (mut u, a, _, _) = setup();
    u.ctx.set_embed_viewports(false);
    u.ctx.enable_accesskit();
    u.run("proto.preview", json!({"start": a}));
    let out = u.frame_output(vec![]);
    let close = button(&out, "Close preview").unwrap().center();
    click(&mut u, close);
    assert!(u.app.ui.preview.is_none());
    u.run("proto.preview", json!({"start": a}));
    let out = u.frame_output(vec![]);
    let present = button(&out, "Present").unwrap().center();
    click(&mut u, present);
    assert!(u.app.ui.present.is_some());
    assert_eq!(u.app.inspect()["presenter"]["current"], json!(a));
}

use crate::{FigApp, NoServices};
use egui::{RawInput, ViewportId, ViewportInfo, vec2};
use std::collections::HashMap;
use std::sync::Arc;

struct NativeUi {
    ctx: egui::Context,
    time: f64,
    callbacks: HashMap<ViewportId, Arc<egui::DeferredViewportUiCallback>>,
}

impl NativeUi {
    fn new(session: figcraft_engine::Session) -> Self {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        crate::windows::install(FigApp::new(session, Box::new(NoServices)));
        let mut u = Self { ctx, time: 0.0, callbacks: HashMap::new() };
        for _ in 0..4 {
            u.root();
        }
        u
    }
    fn with<R>(&self, f: impl FnOnce(&mut FigApp) -> R) -> R {
        crate::windows::with_app(f).unwrap()
    }
    fn root(&mut self) -> egui::FullOutput {
        self.frame(ViewportId::ROOT, vec![], true, None)
    }
    fn frame(&mut self, viewport: ViewportId, mut events: Vec<Event>, focused: bool, close: Option<ViewportId>) -> egui::FullOutput {
        self.time += 1.0 / 60.0;
        // Model the native raw-input hook before egui derives this viewport's pointer/key state.
        events.extend(self.with(|app| crate::windows::take_input(app, viewport)));
        let size = if viewport == ViewportId::ROOT { vec2(1440.0, 900.0) } else { vec2(440.0, 720.0) };
        let mut input = RawInput {
            viewport_id: viewport,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(self.time),
            events,
            focused,
            ..Default::default()
        };
        input.viewports.insert(
            ViewportId::ROOT,
            ViewportInfo {
                focused: Some(viewport == ViewportId::ROOT && focused),
                inner_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                ..Default::default()
            },
        );
        for child in self.callbacks.keys().copied().chain((viewport != ViewportId::ROOT).then_some(viewport)) {
            input.viewports.insert(
                child,
                ViewportInfo {
                    parent: Some(ViewportId::ROOT),
                    focused: Some(child == viewport && focused),
                    native_pixels_per_point: Some(2.0),
                    inner_rect: Some(Rect::from_min_size(pos2(1600.0, 50.0), size)),
                    outer_rect: Some(Rect::from_min_size(pos2(1600.0, 20.0), size + vec2(0.0, 30.0))),
                    events: if close == Some(child) { vec![egui::ViewportEvent::Close] } else { vec![] },
                    ..Default::default()
                },
            );
        }
        let callback = self.callbacks.get(&viewport).cloned();
        let mut out = self.ctx.run_ui(input, |ui| {
            if viewport == ViewportId::ROOT {
                crate::windows::with_app(|app| app.ui(ui));
            } else if let Some(callback) = callback.as_ref() {
                callback(ui);
            }
        });
        for (id, output) in &out.viewport_output {
            if let Some(callback) = &output.viewport_ui_cb {
                self.callbacks.insert(*id, callback.clone());
            }
        }
        out.textures_delta.clear();
        if viewport != ViewportId::ROOT {
            let fault = self.with(|app| {
                let root = app.ui.toast.as_ref().map(|(s, _)| s.clone());
                let child = crate::windows::as_window(
                    app,
                    crate::windows::list(app)
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|w| crate::windows::viewport(w["id"].as_u64().unwrap()) == viewport)
                        .and_then(|w| w["id"].as_u64())
                        .unwrap_or(u64::MAX),
                    |app| app.ui.toast.as_ref().map(|(s, _)| s.clone()),
                )
                .flatten();
                root.or(child).filter(|s| s.starts_with("Internal error"))
            });
            assert!(fault.is_none(), "a caught frame panic is still a product failure: {fault:?}");
        }
        out
    }
    fn run(&mut self, cmd: &str, params: serde_json::Value) -> serde_json::Value {
        let result = self.with(|app| {
            let r = app.session.execute(cmd, &params).unwrap();
            app.handle_ui_requests();
            r
        });
        self.root();
        result
    }
    fn preview_id(&self, uid: u64) -> u64 {
        self.with(|app| {
            crate::windows::list(app).as_array().unwrap().iter().find(|w| w["kind"] == "preview" && w["sourceDocument"] == uid).unwrap()["id"]
                .as_u64()
                .unwrap()
        })
    }
    fn inspect(&self, id: u64) -> serde_json::Value {
        self.with(|app| crate::windows::as_window(app, id, |app| app.inspect()).unwrap())
    }
    fn key(&mut self, id: u64, key: egui::Key, pressed: bool) {
        self.frame(
            crate::windows::viewport(id),
            vec![Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Default::default() }],
            true,
            None,
        );
    }
}

fn native_session() -> (figcraft_engine::Session, figcraft_doc::NodeId, figcraft_doc::NodeId) {
    let mut s = figcraft_engine::Session::new();
    let a = crate::present_tests::id(&s.execute("node.create", &json!({"type":"frame","name":"Preview A","width":240,"height":400})).unwrap());
    let b =
        crate::present_tests::id(&s.execute("node.create", &json!({"type":"frame","name":"Preview B","x":300,"width":240,"height":400})).unwrap());
    (s, a, b)
}

fn inject_native_control_key(u: &mut NativeUi, id: u64, key: &str) {
    let ctx = u.ctx.clone();
    u.with(|app| {
        crate::windows::as_window(app, id, |app| {
            crate::control::handle(app, &ctx, "ui.input", &json!({"events":[{"key":key,"cmd":true}]})).unwrap();
        })
        .unwrap();
    });
    u.frame(crate::windows::viewport(id), vec![], true, None);
}

fn close_native_source(u: &mut NativeUi, uid: u64) {
    let ctx = u.ctx.clone();
    u.with(|app| {
        crate::control::handle(app, &ctx, "engine.execute", &json!({"window":0,"document":uid,"command":"file.close","params":{"force":true}}))
            .unwrap();
    });
}

#[test]
fn detached_preview_source_close_retires_the_injected_input_fence_before_backend_forgets_its_viewport() {
    let (session, a, _) = native_session();
    let mut u = NativeUi::new(session);
    let source = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let closed = u.preview_id(source);
    let closed_viewport = crate::windows::viewport(closed);
    u.run("file.new", json!({"name":"Unrelated survivor"}));
    let survivor_uid = u.with(|app| app.session.ds().unwrap().uid);
    let frame = crate::present_tests::id(&u.run("node.create", json!({"type":"frame","width":200,"height":300})));
    u.run("proto.detachPreview", json!({"start":frame}));
    let survivor = u.preview_id(survivor_uid);
    u.frame(crate::windows::viewport(survivor), vec![], true, None);
    let before = u.with(|app| (app.session.ds().unwrap().doc.clone(), app.session.ds().unwrap().history.undo.len()));

    // Match the native failure: the last acknowledged raw input belongs to the source
    // companion, then an editor request closes that source while another companion survives.
    inject_native_control_key(&mut u, closed, "D");
    assert_eq!(u.with(|app| app.control_presented_input_frame.map(|(viewport, _)| viewport)), Some(closed_viewport));
    close_native_source(&mut u, source);
    u.callbacks.remove(&closed_viewport);
    u.root();
    let retired = u.root();
    assert!(
        !retired.viewport_output.contains_key(&closed_viewport),
        "egui must really forget the closed viewport, not merely hide its registry entry"
    );

    let (tx, rx) = std::sync::mpsc::channel();
    u.with(|app| {
        app.control_queue.push(crate::ControlRequest {
            id: json!(9201),
            method: "window.list".into(),
            params: json!({"window":0}),
            reply: tx,
            state: Default::default(),
        });
    });
    // This surviving deferred draw hit input_in_flight's dead frame counter in the native run.
    u.frame(crate::windows::viewport(survivor), vec![], true, None);
    u.root(); // The queued state request explicitly targets the visible root window.
    let ctx = u.ctx.clone();
    u.with(|app| crate::control::background(app, &ctx));
    let reply = rx.try_recv().expect("the survivor must answer state requests after source closure").result.unwrap();
    assert!(reply.as_array().unwrap().iter().any(|window| window["id"] == survivor));
    u.with(|app| {
        assert!(!app.windows.exists(closed));
        assert!(app.windows.exists(survivor));
        assert_eq!(app.session.ds().unwrap().uid, survivor_uid);
        assert!(Arc::ptr_eq(&before.0, &app.session.ds().unwrap().doc));
        assert_eq!(app.session.ds().unwrap().history.undo.len(), before.1);
    });
    inject_native_control_key(&mut u, survivor, "D");
    assert_eq!(u.with(|app| app.control_presented_input_frame.map(|(viewport, _)| viewport)), Some(crate::windows::viewport(survivor)));
}

#[test]
fn detached_preview_closing_another_source_preserves_the_live_companions_input_fence() {
    let (session, a, _) = native_session();
    let mut u = NativeUi::new(session);
    let source = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let closed = u.preview_id(source);
    u.run("file.new", json!({"name":"Live input owner"}));
    let survivor_uid = u.with(|app| app.session.ds().unwrap().uid);
    let frame = crate::present_tests::id(&u.run("node.create", json!({"type":"frame","width":200,"height":300})));
    u.run("proto.detachPreview", json!({"start":frame}));
    let survivor = u.preview_id(survivor_uid);
    inject_native_control_key(&mut u, survivor, "D");
    let live = u.with(|app| app.control_presented_input_frame);
    assert_eq!(live.map(|(viewport, _)| viewport), Some(crate::windows::viewport(survivor)));
    close_native_source(&mut u, source);
    u.callbacks.remove(&crate::windows::viewport(closed));
    u.root();
    u.root();
    assert_eq!(
        u.with(|app| app.control_presented_input_frame),
        live,
        "closing another UID must not erase the live viewport's presented-input boundary"
    );
    u.frame(crate::windows::viewport(survivor), vec![], true, None);
    assert_eq!(u.with(|app| app.session.ds().unwrap().uid), survivor_uid);
}

#[test]
fn detached_preview_lifecycle_pins_live_uid_across_tabs_and_keeps_private_navigation() {
    let (session, a, b) = native_session();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.preview", json!({"start":a}));
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let viewport = crate::windows::viewport(id);
    let out = u.frame(viewport, vec![], true, None);
    assert!(out.shapes.len() > 5, "the deferred callback paints the child player");
    u.key(id, egui::Key::ArrowRight, true);
    u.key(id, egui::Key::ArrowRight, false);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(b));
    u.run("file.new", json!({"name":"Other editor"}));
    let other = u.with(|app| app.session.ds().unwrap().uid);
    u.with(|app| {
        let active = app.session.active;
        app.session.active = app.session.docs.iter().position(|d| d.uid == uid).unwrap();
        app.session.execute("node.set", &json!({"id":b,"props":{"name":"Live edit","width":260}})).unwrap();
        app.session.active = active;
    });
    u.frame(viewport, vec![], true, None);
    assert_eq!(u.inspect(id)["preview"]["name"], "Live edit");
    assert_eq!(u.inspect(id)["preview"]["frameSize"], json!([260.0, 400.0]));
    u.with(|app| {
        assert_eq!(app.session.ds().unwrap().uid, other);
        assert_eq!(app.session.docs.iter().find(|d| d.uid == uid).unwrap().doc.node(a).unwrap().name, "Preview A");
    });
    // Repeated detach focuses the same companion without restarting its navigation.
    u.with(|app| {
        let active = app.session.active;
        app.session.active = app.session.docs.iter().position(|d| d.uid == uid).unwrap();
        app.session.execute("proto.detachPreview", &json!({"start":a})).unwrap();
        app.session.active = active;
        app.handle_ui_requests();
    });
    u.root();
    assert_eq!(u.preview_id(uid), id);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(b));
    assert_eq!(u.with(|app| app.session.ds().unwrap().uid), other);
}

#[test]
fn detached_preview_close_resets_without_undo_or_tab_changes_and_old_callback_cannot_rebind() {
    let (session, a, b) = native_session();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    let undo = u.with(|app| app.session.ds().unwrap().history.undo.len());
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let viewport = crate::windows::viewport(id);
    u.key(id, egui::Key::ArrowRight, true);
    u.key(id, egui::Key::ArrowRight, false);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(b));
    u.frame(viewport, vec![], true, Some(viewport));
    assert!(!u.with(|app| app.windows.exists(id)));
    u.with(|app| {
        assert_eq!(app.session.ds().unwrap().uid, uid);
        assert_eq!(app.session.ds().unwrap().history.undo.len(), undo);
    });
    u.run("proto.preview", json!({"start":a}));
    assert_eq!(u.with(|app| app.inspect())["preview"]["current"], json!(a));
    u.run("proto.detachPreview", json!({"start":a}));
    let second = u.preview_id(uid);
    assert_ne!(second, id);
    u.run("file.close", json!({"force":true}));
    u.run("file.new", json!({"name":"Replacement"}));
    let replacement = u.with(|app| app.session.ds().unwrap().uid);
    assert_ne!(replacement, uid);
    u.frame(crate::windows::viewport(second), vec![], true, None);
    assert!(!u.with(|app| app.windows.exists(second)));
    assert_eq!(u.with(|app| app.session.ds().unwrap().uid), replacement);
}

#[test]
fn detached_preview_capture_and_input_are_owned_by_the_companion_and_design_methods_refuse() {
    let (session, a, b) = native_session();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let before = u.with(|app| app.session.doc().unwrap().clone());
    let ctx = u.ctx.clone();
    u.with(|app| {
        crate::windows::as_window(app, id, |app| {
            for method in ["engine.execute", "ui.pointer", "ui.key", "ui.type", "ui.set"] {
                assert!(
                    crate::control::handle(
                        app,
                        &ctx,
                        method,
                        &json!({"command":"shape.rectangle","params":{},"key":"ArrowRight","text":"no","x":0,"y":0})
                    )
                    .unwrap_err()
                    .contains("preview accepts"),
                    "{method}"
                );
            }
            crate::control::handle(app, &ctx, "ui.input", &json!({"events":[{"key":"ArrowRight"}]})).unwrap();
        })
        .unwrap()
    });
    let vp = crate::windows::viewport(id);
    u.frame(vp, vec![], true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(b));
    u.with(|app| assert_eq!(app.session.doc().unwrap().nodes().collect::<Vec<_>>(), before.nodes().collect::<Vec<_>>()));
    let (tx, rx) = std::sync::mpsc::channel();
    u.with(|app| {
        app.control_queue.push(crate::ControlRequest {
            id: json!(81),
            method: "ui.screenshot".into(),
            params: json!({"window":id}),
            reply: tx,
            state: Default::default(),
        })
    });
    u.frame(vp, vec![], true, None);
    u.with(|app| {
        assert_eq!(app.capture.shots[0].document, uid);
        assert_eq!(app.capture.shots[0].spec.unwrap().size_px, [880, 1440], "capture uses child size and 2x DPI");
    });
    u.frame(vp, vec![], true, Some(vp));
    u.root();
    assert!(rx.try_recv().unwrap().result.unwrap_err().contains("closed"));
    u.with(|app| assert!(app.capture.shots.is_empty()));
}

#[test]
fn detached_preview_embedded_and_screen_reader_hosts_keep_an_accessible_inline_player() {
    for screen_reader in [false, true] {
        let (mut u, a, _, _) = setup();
        u.ctx.set_embed_viewports(!screen_reader);
        u.ctx.enable_accesskit();
        u.run("proto.preview", json!({"start":a}));
        let out = u.frame_output(vec![]);
        let node = out.platform_output.accesskit_update.as_ref().unwrap().nodes.iter().find(|(_, n)| n.label() == Some("Pop out preview")).unwrap();
        assert!(node.1.is_disabled(), "host capability refusal is discoverable");
        let before = u.app.session.ds().unwrap().history.undo.len();
        u.run("proto.detachPreview", json!({"start":a}));
        assert!(u.app.ui.preview.is_some());
        assert_eq!(u.app.session.ds().unwrap().history.undo.len(), before);
        assert_eq!(crate::windows::list(&u.app).as_array().unwrap().len(), 1);
    }
}

#[test]
fn detached_preview_root_metadata_stays_on_its_editor_during_companion_inspection() {
    let (session, a, _) = native_session();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    u.run("file.new", json!({"name":"Other editor"}));
    let child = u.inspect(id);
    assert_eq!(child["windows"][0]["active"], "Other editor", "temporary source inspection must preserve the root editor presentation");
    assert_eq!(u.with(|app| app.session.doc().unwrap().name.clone()), "Other editor");
}

fn key_event(key: egui::Key, pressed: bool) -> Event {
    Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Default::default() }
}
fn hotspot_center(u: &NativeUi, id: u64, node: figcraft_doc::NodeId) -> Pos2 {
    let state = u.inspect(id);
    let r = &state["preview"]["hotspots"].as_array().unwrap().iter().find(|h| h["id"] == json!(node)).unwrap()["rect"];
    pos2(((r[0].as_f64().unwrap() + r[2].as_f64().unwrap()) / 2.0) as f32, ((r[1].as_f64().unwrap() + r[3].as_f64().unwrap()) / 2.0) as f32)
}

#[test]
fn detached_preview_transfer_drops_held_key_release_but_fresh_focused_keys_play_without_hover() {
    let (mut session, a, b) = native_session();
    session.execute("proto.interaction.add", &json!({"id":a,"trigger":{"type":"keyUp","key":"K"},"action":{"type":"navigate","dest":b}})).unwrap();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.preview", json!({"start":a}));
    let pane = u.ctx.memory(|m| m.area_rect(crate::present::pane_id())).unwrap();
    u.frame(ViewportId::ROOT, vec![Event::PointerMoved(pane.center()), key_event(egui::Key::K, true)], true, None);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    u.key(id, egui::Key::K, false);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(a), "old release has no transferred authority");
    u.key(id, egui::Key::K, true);
    u.key(id, egui::Key::K, false);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(b), "the focused companion takes a fresh key even without a hovered pointer");
}

#[test]
fn detached_preview_transfer_drops_a_pointer_press_before_accepting_a_new_click() {
    let (mut session, a, b) = native_session();
    session.execute("proto.interaction.add", &json!({"id":a,"trigger":{"type":"onClick"},"action":{"type":"navigate","dest":b}})).unwrap();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.preview", json!({"start":a}));
    let at = u.ctx.memory(|m| m.area_rect(crate::present::pane_id())).unwrap().center();
    u.frame(
        ViewportId::ROOT,
        vec![Event::PointerMoved(at), Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Default::default() }],
        true,
        None,
    );
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let vp = crate::windows::viewport(id);
    u.frame(vp, vec![], true, None);
    let at = hotspot_center(&u, id, a);
    let pointer = |pressed| {
        vec![Event::PointerMoved(at), Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() }]
    };
    u.frame(vp, pointer(false), true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(a));
    u.frame(vp, pointer(true), true, None);
    u.frame(vp, pointer(false), true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(b));
}

#[test]
fn detached_preview_controller_focus_regain_adopts_held_buttons_without_replay() {
    use std::sync::atomic::{AtomicU32, Ordering};
    struct Pads(Arc<AtomicU32>);
    impl crate::Services for Pads {
        fn gamepad_held(&mut self) -> u32 {
            self.0.load(Ordering::SeqCst)
        }
    }
    let (mut session, a, b) = native_session();
    session
        .execute("proto.interaction.add", &json!({"id":a,"trigger":{"type":"keyPress","key":"Gamepad A"},"action":{"type":"navigate","dest":b}}))
        .unwrap();
    let mut u = NativeUi::new(session);
    let held = Arc::new(AtomicU32::new(1));
    u.with(|app| app.services = Box::new(Pads(held.clone())));
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let vp = crate::windows::viewport(id);
    u.frame(vp, vec![], true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(a), "held before transfer is adopted");
    held.store(0, Ordering::SeqCst);
    u.frame(vp, vec![], true, None);
    held.store(1, Ordering::SeqCst);
    u.frame(vp, vec![], false, None);
    u.frame(vp, vec![], true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(a), "held during focus loss is adopted");
    held.store(0, Ordering::SeqCst);
    u.frame(vp, vec![], true, None);
    held.store(1, Ordering::SeqCst);
    u.frame(vp, vec![], true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(b));
}

#[test]
fn detached_preview_stays_with_its_uid_when_the_source_tab_moves_to_an_editor_window() {
    let (session, a, _) = native_session();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    u.run("file.new", json!({"name":"Other editor"}));
    let other = u.with(|app| app.session.ds().unwrap().uid);
    let ctx = u.ctx.clone();
    let owner = u.with(|app| crate::windows::open(app, &ctx, Some(app.session.docs.iter().position(|d| d.uid == uid).unwrap()), None, None).unwrap());
    u.root();
    u.frame(crate::windows::viewport(id), vec![], true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(a));
    u.with(|app| {
        assert_eq!(app.windows.of(uid), owner);
        assert_eq!(app.session.ds().unwrap().uid, other);
        let list = crate::windows::list(app);
        assert_eq!(list.as_array().unwrap().iter().find(|w| w["id"] == id).unwrap()["files"], json!([]));
    });
}

#[test]
fn detached_preview_requests_capture_uid_before_tab_changes_and_limit_companions_to_eight() {
    let (session, a, _) = native_session();
    let mut u = NativeUi::new(session);
    let first = u.with(|app| app.session.ds().unwrap().uid);
    u.with(|app| {
        app.session.execute("proto.detachPreview", &json!({"start":a})).unwrap();
        app.session.execute("file.new", &json!({"name":"Second"})).unwrap();
        app.handle_ui_requests();
    });
    u.root();
    let first_id = u.preview_id(first);
    assert_ne!(u.with(|app| app.session.ds().unwrap().uid), first);
    for index in 1..=8 {
        if index > 1 {
            u.run("file.new", json!({"name":format!("Source {index}")}));
        }
        let frame = u.run("node.create", json!({"type":"frame","width":100,"height":100}))["id"].clone();
        u.run("proto.detachPreview", json!({"start":frame}));
    }
    let list = u.with(|app| crate::windows::list(app));
    assert_eq!(list.as_array().unwrap().iter().filter(|w| w["kind"] == "preview").count(), 8);
    assert!(u.with(|app| app.ui.preview.is_some()), "the ninth request keeps a working inline fallback");
    assert_eq!(u.preview_id(first), first_id);
}

#[test]
fn detached_preview_runtime_variable_changes_keep_the_editor_and_global_variable_unchanged() {
    let (mut session, a, b) = native_session();
    session.execute("variable.createCollection", &json!({"name":"Tokens"})).unwrap();
    session.execute("variable.create", &json!({"collection":"Tokens","name":"Size","type":"number","value":20})).unwrap();
    let r = crate::present_tests::id(
        &session.execute("node.create", &json!({"type":"rectangle","parent":a,"x":20,"y":20,"width":20,"height":20})).unwrap(),
    );
    session.execute("variable.bind", &json!({"ids":[r],"field":"width","variable":"Size"})).unwrap();
    session.execute("proto.interaction.add", &json!({"id":r,"trigger":{"type":"onClick"},"action":{"type":"navigate","dest":b}})).unwrap();
    session
        .execute(
            "proto.interaction.add",
            &json!({"id":a,"trigger":{"type":"keyPress","key":"K"},"action":{"type":"setVariable","variable":"Size","value":60}}),
        )
        .unwrap();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    let undo = u.with(|app| app.session.ds().unwrap().history.undo.len());
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let vp = crate::windows::viewport(id);
    u.frame(vp, vec![], true, None);
    let width = |u: &NativeUi| {
        let state = u.inspect(id);
        let r = &state["preview"]["hotspots"].as_array().unwrap().iter().find(|h| h["id"] == json!(r)).unwrap()["rect"];
        r[2].as_f64().unwrap() - r[0].as_f64().unwrap()
    };
    let before = width(&u);
    u.key(id, egui::Key::K, true);
    u.key(id, egui::Key::K, false);
    assert!((width(&u) - 3.0 * before).abs() < 0.01);
    u.with(|app| {
        assert_eq!(app.session.doc().unwrap().node(r).unwrap().width, 20.0);
        assert_eq!(app.session.ds().unwrap().history.undo.len(), undo);
    });
}

#[test]
fn detached_preview_unavailable_background_request_does_not_replace_another_files_inline_player() {
    let (mut u, a, _, _) = setup();
    let first = u.app.session.ds().unwrap().uid;
    u.run("file.new", json!({"name":"Active editor"}));
    let b = crate::present_tests::id(&u.run("node.create", json!({"type":"frame","name":"Keep this player","width":200,"height":300})));
    u.run("proto.preview", json!({"start":b}));
    let active = u.app.session.ds().unwrap().uid;
    let forward = u.app.session.bring_forward(&json!(first)).unwrap();
    u.app.session.execute("proto.detachPreview", &json!({"start":a})).unwrap();
    u.app.session.put_back(forward);
    u.app.handle_ui_requests();
    u.frame(vec![]);
    assert_eq!(u.app.session.ds().unwrap().uid, active);
    assert_eq!(u.preview()["name"], "Keep this player", "an unavailable delayed request must not overwrite the active file's inline player");
    assert_eq!(crate::windows::list(&u.app).as_array().unwrap().len(), 1);
}

#[test]
fn detached_preview_stale_request_cannot_attach_to_a_replacement_with_the_same_node_id() {
    let (session, a, _) = native_session();
    let mut u = NativeUi::new(session);
    let old = u.with(|app| app.session.ds().unwrap().uid);
    u.with(|app| {
        app.session.execute("proto.detachPreview", &json!({"start":a})).unwrap();
        app.session.execute("file.close", &json!({"force":true})).unwrap();
        app.session.execute("file.new", &json!({"name":"Replacement"})).unwrap();
        let next = app.session.execute("node.create", &json!({"type":"frame","width":200,"height":300})).unwrap();
        assert_eq!(next["id"], json!(a));
        app.handle_ui_requests();
    });
    u.root();
    u.with(|app| {
        assert_ne!(app.session.ds().unwrap().uid, old);
        assert_eq!(crate::windows::list(app).as_array().unwrap().len(), 1);
        assert!(app.ui.preview.is_none());
    });
}

#[test]
fn detached_preview_native_menu_never_edits_the_source_or_other_editor() {
    let (mut session, a, _) = native_session();
    let child = crate::present_tests::id(&session.execute("node.create", &json!({"type":"rectangle","parent":a,"width":30,"height":30})).unwrap());
    session.execute("select.set", &json!({"ids":[child]})).unwrap();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    u.run("file.new", json!({"name":"Other editor"}));
    let before = u.with(|app| app.session.docs.iter().find(|d| d.uid == uid).unwrap().clone());
    u.with(|app| {
        app.windows.focused = id;
        crate::native_menu::run_in_front(app, "edit.delete");
    });
    u.with(|app| {
        let source = app.session.docs.iter().find(|d| d.uid == uid).unwrap();
        assert!(source.doc.contains(child), "native-menu design actions must not edit a preview's source");
        assert_eq!(source.history.undo.len(), before.history.undo.len());
        assert_eq!(app.session.doc().unwrap().name, "Other editor");
        let bar = crate::native_menu::menu_bar_in_front(app, true);
        assert!(!bar.find(&"edit.delete".into()).unwrap().enabled, "preview menus expose the same refusal");
        assert!(bar.find(&"edit.delete".into()).unwrap().shortcut.is_none());
        let native = craft_menu::mac_layout(&bar, &craft_menu::LayoutOptions::new("FigCraft")).bar;
        assert!(
            native
                .menus
                .iter()
                .flat_map(|m| m.children.iter())
                .any(|n| matches!(n, craft_menu::Node::Standard { kind: craft_menu::Standard::CloseWindow, .. })),
            "native Close Window remains available"
        );
        assert!(native.find(&craft_menu::Builtin::Quit.command()).unwrap().enabled, "native Quit keeps the existing unsaved-work check");
    });
}

#[test]
fn detached_preview_transient_focus_loss_drops_old_key_release_then_accepts_a_fresh_gesture() {
    let (mut session, a, b) = native_session();
    session.execute("proto.interaction.add", &json!({"id":a,"trigger":{"type":"keyUp","key":"K"},"action":{"type":"navigate","dest":b}})).unwrap();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let vp = crate::windows::viewport(id);
    u.frame(vp, vec![], true, None);
    let before = u.with(|app| app.session.doc().unwrap().clone());
    let undo = u.with(|app| app.session.undo_depth());
    u.key(id, egui::Key::K, true);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(a), "keyUp fixture is held before the focus boundary");
    u.frame(vp, vec![Event::WindowFocused(false), Event::WindowFocused(true), key_event(egui::Key::K, false)], true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(a), "a same-batch focus regain cannot authorize the pre-loss release");
    u.key(id, egui::Key::K, true);
    u.key(id, egui::Key::K, false);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(b), "a fresh focused gesture still navigates");
    u.with(|app| {
        assert_eq!(app.session.doc().unwrap(), &before);
        assert_eq!(app.session.undo_depth(), undo);
    });
}

#[test]
fn detached_preview_transient_focus_loss_drops_old_pointer_release_then_accepts_a_fresh_click() {
    let (mut session, a, b) = native_session();
    session.execute("proto.interaction.add", &json!({"id":a,"trigger":{"type":"onClick"},"action":{"type":"navigate","dest":b}})).unwrap();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let vp = crate::windows::viewport(id);
    u.frame(vp, vec![], true, None);
    let at = hotspot_center(&u, id, a);
    let pointer = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
    let before = u.with(|app| app.session.doc().unwrap().clone());
    let undo = u.with(|app| app.session.undo_depth());
    u.frame(vp, vec![Event::PointerMoved(at), pointer(true)], true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(a), "the click has not released before the focus boundary");
    u.frame(vp, vec![Event::WindowFocused(false), Event::WindowFocused(true), Event::PointerMoved(at), pointer(false)], true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(a), "a same-batch focus regain cannot authorize the pre-loss pointer release");
    u.frame(vp, vec![Event::PointerMoved(at), pointer(true)], true, None);
    u.frame(vp, vec![Event::PointerMoved(at), pointer(false)], true, None);
    assert_eq!(u.inspect(id)["preview"]["current"], json!(b), "a fresh focused click still navigates");
    u.with(|app| {
        assert_eq!(app.session.doc().unwrap(), &before);
        assert_eq!(app.session.undo_depth(), undo);
    });
}

#[test]
fn detached_preview_native_menu_does_not_fall_back_to_an_editor_when_its_source_closed_before_retirement() {
    let (session, a, _) = native_session();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    u.run("file.new", json!({"name":"Keep this editor"}));
    let r = crate::present_tests::id(&u.run("node.create", json!({"type":"rectangle","width":20,"height":20})));
    u.run("select.set", json!({"ids":[r]}));
    u.with(|app| {
        let index = app.session.docs.iter().position(|d| d.uid == uid).unwrap();
        app.session.execute("file.close", &json!({"index":index,"force":true})).unwrap();
        let before = app.session.ds().unwrap().clone();
        app.windows.focused = id;
        // The source has gone but root's next frame has not retired the companion yet.
        crate::native_menu::run_in_front(app, "edit.delete");
        assert!(app.session.doc().unwrap().contains(r), "a failed preview UID swap cannot fall through to editing another window");
        assert_eq!(app.session.ds().unwrap().history.undo.len(), before.history.undo.len());
        assert_eq!(app.session.doc().unwrap().name, "Keep this editor");
    });
}

#[test]
fn detached_preview_native_paste_queued_for_a_closed_source_is_retired_without_a_child_raw_hook() {
    let (session, a, _) = native_session();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let vp = crate::windows::viewport(id);
    let ctx = u.ctx.clone();
    u.with(|app| {
        app.windows.focused = id;
        crate::native_menu::send_keys(app, &ctx, vec![Event::Paste("Original synthetic pending input".into())]);
        assert!(app.native_menu_inputs.contains_key(&vp));
    });
    // Source closure is processed by root; this child never takes another raw-input hook.
    u.run("file.close", json!({"force":true}));
    u.with(|app| {
        assert!(!app.windows.exists(id));
        assert!(!app.native_menu_inputs.contains_key(&vp), "retired preview cannot retain a native paste batch");
    });
}

fn painted_control_center(out: &egui::FullOutput, label: &str, screen: Rect) -> Pos2 {
    fn find(shape: &egui::Shape, clip: Rect, label: &str, screen: Rect) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                let rect = Rect::from_min_size(t.pos, t.galley.size());
                (clip.contains_rect(rect) && screen.contains_rect(rect)).then_some(rect.center())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, clip, label, screen)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|s| find(&s.shape, s.clip_rect, label, screen)).unwrap_or_else(|| panic!("visible painted notice control {label:?}"))
}

fn notice_focus_boundary(label: &str) {
    use std::sync::Mutex;
    struct Opened(Arc<Mutex<Vec<String>>>);
    impl crate::Services for Opened {
        fn open_url(&mut self, url: &str) {
            self.0.lock().unwrap().push(url.into());
        }
    }
    const URL: &str = "https://example.invalid/original-preview-test";
    let (mut session, a, _) = native_session();
    session.execute("proto.interaction.add", &json!({"id":a,"trigger":{"type":"keyPress","key":"K"},"action":{"type":"openUrl","url":URL}})).unwrap();
    let mut u = NativeUi::new(session);
    let opened = Arc::new(Mutex::new(Vec::new()));
    u.with(|app| app.services = Box::new(Opened(opened.clone())));
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    let authored = u.with(|app| app.session.doc().unwrap().clone());
    let history = u.with(|app| app.session.undo_depth());
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let vp = crate::windows::viewport(id);
    u.frame(vp, vec![], true, None);
    u.key(id, egui::Key::K, true);
    u.key(id, egui::Key::K, false);
    let mut out = u.frame(vp, vec![], true, None);
    for _ in 0..2 {
        out = u.frame(vp, vec![], true, None);
    }
    assert_eq!(u.inspect(id)["preview"]["linkNotice"], URL);
    assert!(opened.lock().unwrap().is_empty());
    let at = painted_control_center(&out, label, Rect::from_min_size(Pos2::ZERO, vec2(440.0, 720.0)));
    let pointer = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
    u.frame(vp, vec![Event::PointerMoved(at), pointer(true)], true, None);
    u.frame(vp, vec![Event::WindowFocused(false), Event::WindowFocused(true), Event::PointerMoved(at), pointer(false)], true, None);
    assert!(opened.lock().unwrap().is_empty(), "a pre-loss notice press cannot request any platform opener");
    assert_eq!(u.inspect(id)["preview"]["linkNotice"], URL, "a pre-loss notice press cannot cancel its new focus owner");
    // For the checkbox, a fresh Open reveals whether the old click improperly changed skip.
    let out = u.frame(vp, vec![], true, None);
    let fresh_label = if label == "Don't show this again" { "Open link" } else { label };
    let at = painted_control_center(&out, fresh_label, Rect::from_min_size(Pos2::ZERO, vec2(440.0, 720.0)));
    let pointer = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
    u.frame(vp, vec![Event::PointerMoved(at), pointer(true)], true, None);
    u.frame(vp, vec![Event::PointerMoved(at), pointer(false)], true, None);
    assert!(u.inspect(id)["preview"]["linkNotice"].is_null(), "a fresh focused notice choice still works");
    assert_eq!(opened.lock().unwrap().len(), usize::from(fresh_label == "Open link"));
    u.with(|app| {
        assert!(!app.session.prefs.link_notice_skipped, "the retired checkbox press did not change skip policy");
        assert_eq!(app.session.doc().unwrap(), &authored);
        assert_eq!(app.session.undo_depth(), history);
    });
    if label == "Don't show this again" {
        // A genuinely new checkbox click still chooses the saved policy.
        u.key(id, egui::Key::K, true);
        u.key(id, egui::Key::K, false);
        let mut out = u.frame(vp, vec![], true, None);
        for _ in 0..2 {
            out = u.frame(vp, vec![], true, None);
        }
        let at = painted_control_center(&out, "Don't show this again", Rect::from_min_size(Pos2::ZERO, vec2(440.0, 720.0)));
        let pointer = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        u.frame(vp, vec![Event::PointerMoved(at), pointer(true)], true, None);
        u.frame(vp, vec![Event::PointerMoved(at), pointer(false)], true, None);
        let out = u.frame(vp, vec![], true, None);
        let at = painted_control_center(&out, "Open link", Rect::from_min_size(Pos2::ZERO, vec2(440.0, 720.0)));
        let pointer = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        u.frame(vp, vec![Event::PointerMoved(at), pointer(true)], true, None);
        u.frame(vp, vec![Event::PointerMoved(at), pointer(false)], true, None);
        assert_eq!(opened.lock().unwrap().len(), 2);
        u.with(|app| {
            assert!(app.session.prefs.link_notice_skipped, "a fresh checkbox click still toggles policy");
            assert_eq!(app.session.doc().unwrap(), &authored);
            assert_eq!(app.session.undo_depth(), history);
        });
    }
}

#[test]
fn detached_preview_notice_open_requires_a_fresh_press_after_transient_focus_loss() {
    notice_focus_boundary("Open link");
}
#[test]
fn detached_preview_notice_cancel_requires_a_fresh_press_after_transient_focus_loss() {
    notice_focus_boundary("Cancel");
}
#[test]
fn detached_preview_notice_skip_checkbox_requires_a_fresh_press_after_transient_focus_loss() {
    notice_focus_boundary("Don't show this again");
}

#[test]
fn detached_preview_live_source_edit_wakes_only_its_idle_child() {
    let (session, a, _) = native_session();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let viewport = crate::windows::viewport(id);
    u.run("file.new", json!({"name":"Unrelated source"}));
    let other_uid = u.with(|app| app.session.ds().unwrap().uid);
    let other_frame = u.run("node.create", json!({"type":"frame","name":"Unrelated frame","width":200,"height":300}));
    let other_frame = crate::present_tests::id(&other_frame);
    u.run("proto.detachPreview", json!({"start":other_frame}));
    let other = crate::windows::viewport(u.preview_id(other_uid));
    for _ in 0..8 {
        u.frame(viewport, vec![], true, None);
        u.frame(other, vec![], true, None);
        u.root();
    }
    let wakes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let requests = wakes.clone();
    u.ctx.set_request_repaint_callback(move |info| requests.lock().unwrap().push(info.viewport_id));
    // A source edit to an inactive editor tab has no pointer, timer, capture, or child frame.
    u.with(|app| {
        let active = app.session.active;
        app.session.active = app.session.docs.iter().position(|d| d.uid == uid).unwrap();
        app.session.execute("node.set", &json!({"id":a,"props":{"name":"Woken edit","width":280}})).unwrap();
        app.session.active = active;
    });
    u.root();
    let requested = wakes.lock().unwrap().clone();
    assert!(requested.contains(&viewport), "source edit must wake its idle companion: {requested:?}");
    assert!(!requested.contains(&other), "unrelated companion must remain idle: {requested:?}");
    u.frame(viewport, vec![], true, None);
    assert_eq!(u.inspect(id)["preview"]["name"], "Woken edit");
    assert_eq!(u.inspect(id)["preview"]["frameSize"], json!([280.0, 400.0]));
    for _ in 0..4 {
        u.frame(viewport, vec![], true, None);
    }
    wakes.lock().unwrap().clear();
    u.root();
    assert!(!wakes.lock().unwrap().contains(&viewport), "unchanged source must not keep waking an idle child");
}

#[test]
fn detached_preview_live_source_edit_wakes_from_logic_without_an_editor_paint() {
    let (session, a, _) = native_session();
    let mut u = NativeUi::new(session);
    let uid = u.with(|app| app.session.ds().unwrap().uid);
    u.run("proto.detachPreview", json!({"start":a}));
    let id = u.preview_id(uid);
    let viewport = crate::windows::viewport(id);
    for _ in 0..8 {
        u.frame(viewport, vec![], true, None);
        u.root();
    }
    let wakes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let requests = wakes.clone();
    u.ctx.set_request_repaint_callback(move |info| requests.lock().unwrap().push(info.viewport_id));
    let ctx = u.ctx.clone();
    let undo = u.with(|app| {
        app.session.execute("node.set", &json!({"id":a,"props":{"name":"Background edit","width":300}})).unwrap();
        let undo = app.session.ds().unwrap().history.undo.len();
        // The real logic-only phase, after control/live polling, must request the child itself.
        app.background_tick(&ctx);
        undo
    });
    assert!(wakes.lock().unwrap().contains(&viewport), "background edits must wake a visible preview without an editor frame");
    u.frame(viewport, vec![], true, None);
    assert_eq!(u.inspect(id)["preview"]["name"], "Background edit");
    assert_eq!(u.inspect(id)["preview"]["frameSize"], json!([300.0, 400.0]));
    assert_eq!(u.with(|app| app.session.ds().unwrap().history.undo.len()), undo);
    for _ in 0..4 {
        u.frame(viewport, vec![], true, None);
    }
    wakes.lock().unwrap().clear();
    u.with(|app| app.background_tick(&ctx));
    assert!(!wakes.lock().unwrap().contains(&viewport), "unchanged source must not cause a logic-only repaint loop");
}
