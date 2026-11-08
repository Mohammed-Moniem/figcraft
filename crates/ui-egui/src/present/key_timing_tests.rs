//! Original #555 real-frame controls. The runtime RED keeps typed plumbing but old input handling.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect, vec2};
use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::Services;
use crate::present_tests::{Ui as Harness, id};

fn ex(s: &mut Session, command: &str, params: Value) -> Value {
    s.execute(command, &params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn frame(s: &mut Session, name: &str, x: f64) -> NodeId {
    id(&ex(s, "frame.create", json!({"name":name,"x":x,"width":300,"height":220})))
}

fn on(s: &mut Session, node: NodeId, kind: &str, key: &str, ms: u32, action: Value) {
    ex(s, "proto.interaction.add", json!({"id":node,"trigger":{"type":kind,"key":key,"ms":ms},"action":action,"transition":{"type":"instant"}}));
}

fn pair(key: &str, ms: u32) -> (Harness, NodeId, NodeId) {
    let mut s = Session::new();
    let base = frame(&mut s, "Original timing base", 0.0);
    let menu = frame(&mut s, "Original timing menu", 400.0);
    on(&mut s, base, "keyDown", key, ms, json!({"type":"openOverlay","dest":menu}));
    on(&mut s, base, "keyUp", key, ms, json!({"type":"closeOverlay"}));
    let mut ui = Harness::new(s);
    ui.run("proto.present", json!({"start":base}));
    (ui, base, menu)
}

fn key(key: Key, pressed: bool, repeat: bool, modifiers: Modifiers) -> Event {
    Event::Key { key, physical_key: None, pressed, repeat, modifiers }
}

fn overlays(ui: &Harness) -> Vec<Value> {
    ui.app.inspect()["presenter"]["overlays"].as_array().unwrap().clone()
}

fn focus(ui: &mut Harness, focused: bool) {
    ui.time += 1.0 / 60.0;
    let raw =
        RawInput { time: Some(ui.time), focused, screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))), ..Default::default() };
    let app = &mut ui.app;
    let _ = ui.ctx.run_ui(raw, |screen| app.ui(screen));
}

#[test]
fn prototype_key_timing_immediate_pair_closes_the_same_base_owners_overlay_on_release() {
    let (mut ui, _, menu) = pair("A", 0);
    let document = ui.app.session.doc().unwrap().clone();
    let history = ui.app.session.undo_depth();
    ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
    assert_eq!(overlays(&ui), [json!(menu)], "down must open before release");
    ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
    assert!(overlays(&ui).is_empty(), "up belongs to the base even after its down opened an overlay");
    assert_eq!(ui.app.session.doc().unwrap(), &document);
    assert_eq!(ui.app.session.undo_depth(), history);
}

#[test]
fn prototype_key_timing_threshold_fires_once_and_early_release_cancels() {
    let (mut ui, _, menu) = pair("A", 500);
    ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
    ui.frames(20, Vec::new);
    assert!(overlays(&ui).is_empty(), "still below the continuous hold threshold");
    ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
    ui.frames(40, Vec::new);
    assert!(overlays(&ui).is_empty(), "a released early hold cannot fire later");
    ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
    ui.frames(20, Vec::new);
    assert!(overlays(&ui).is_empty());
    ui.frames(20, Vec::new);
    assert_eq!(overlays(&ui), [json!(menu)], "a fresh sustained hold must fire");
    ui.frames(5, || vec![key(Key::A, true, true, Modifiers::NONE)]);
    ui.frames(40, Vec::new);
    assert_eq!(overlays(&ui), [json!(menu)], "OS repeats and idle frames cannot refire");
    ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
    assert!(overlays(&ui).is_empty());
}

#[test]
fn prototype_key_timing_up_only_requires_an_initial_matching_down_and_enough_hold() {
    let mut s = Session::new();
    let base = frame(&mut s, "Original release base", 0.0);
    let menu = frame(&mut s, "Original release menu", 400.0);
    on(&mut s, base, "keyUp", "A", 500, json!({"type":"openOverlay","dest":menu}));
    let mut ui = Harness::new(s);
    ui.run("proto.present", json!({"start":base}));
    ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
    assert!(overlays(&ui).is_empty(), "unmatched release is not a completed hold");
    ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
    ui.frames(10, Vec::new);
    ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
    assert!(overlays(&ui).is_empty());
    ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
    ui.frames(40, Vec::new);
    assert!(overlays(&ui).is_empty(), "up action never fires while the key remains down");
    ui.frame(vec![key(Key::B, false, false, Modifiers::NONE)]);
    assert!(overlays(&ui).is_empty(), "another key cannot complete the hold");
    ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
    assert_eq!(overlays(&ui), [json!(menu)]);
}

#[test]
fn prototype_key_timing_combination_uses_initial_modifiers_and_same_frame_edges_do_not_stick() {
    let (mut ui, _, menu) = pair("Shift+A", 0);
    ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
    assert!(overlays(&ui).is_empty(), "the wrong initial combination cannot bind");
    ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
    ui.frame(vec![key(Key::A, true, false, Modifiers::SHIFT)]);
    assert_eq!(overlays(&ui), [json!(menu)]);
    ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
    assert!(overlays(&ui).is_empty(), "releasing Shift first must not strand its paired A release");
    ui.frame(vec![key(Key::A, true, false, Modifiers::SHIFT), key(Key::A, false, false, Modifiers::NONE)]);
    ui.frames(3, Vec::new);
    assert!(overlays(&ui).is_empty(), "both actual edges in one frame complete the pair");
    ui.frame(vec![key(Key::A, true, false, Modifiers::SHIFT)]);
    assert_eq!(overlays(&ui), [json!(menu)], "the previous same-frame pair did not retain a stale hold");
}

#[test]
fn prototype_key_timing_source_restart_focus_and_navigation_cancel_pending_holds() {
    for cancel in ["source", "restart", "focus", "navigation"] {
        let (mut ui, base, menu) = pair("A", 500);
        ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
        ui.frames(15, Vec::new);
        match cancel {
            "source" => {
                ui.run("node.set", json!({"id":base,"props":{"name":"Edited timing base"}}));
            }
            "restart" => ui.frame(vec![key(Key::R, true, false, Modifiers::NONE)]),
            "focus" => {
                focus(&mut ui, false);
                focus(&mut ui, true);
            }
            _ => ui.frame(vec![key(Key::ArrowRight, true, false, Modifiers::NONE)]),
        }
        ui.frames(40, Vec::new);
        assert!(overlays(&ui).is_empty(), "{cancel} must retire the original owner/hold");
        ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
        if cancel == "navigation" {
            // Present reuses the existing tab without restarting when its start is unchanged.
            assert_ne!(ui.app.inspect()["presenter"]["current"], json!(base));
            ui.frame(vec![key(Key::R, true, false, Modifiers::NONE), key(Key::R, false, false, Modifiers::NONE)]);
            assert_eq!(ui.app.inspect()["presenter"]["current"], json!(base));
        }
        ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
        ui.frames(40, Vec::new);
        assert_eq!(overlays(&ui), [json!(menu)], "{cancel}: only the fresh hold may acquire the owner");
    }
}

struct Pads {
    held: Arc<AtomicU32>,
    polls: Arc<AtomicUsize>,
}

impl Services for Pads {
    fn gamepad_held(&mut self) -> u32 {
        self.polls.fetch_add(1, Ordering::Relaxed);
        self.held.load(Ordering::Relaxed)
    }
}

#[test]
fn prototype_key_timing_controller_uses_real_poll_hold_and_release_edges() {
    let (mut ui, base, menu) = pair("Gamepad A", 500);
    let held = Arc::new(AtomicU32::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    ui.app.services = Box::new(Pads { held: held.clone(), polls: polls.clone() });
    ui.frames(2, Vec::new);
    assert_eq!(ui.app.inspect()["presenter"]["gamepad"], true);
    held.store(1, Ordering::Relaxed);
    ui.frames(20, Vec::new);
    assert!(overlays(&ui).is_empty());
    held.store(0, Ordering::Relaxed);
    ui.frames(40, Vec::new);
    assert!(overlays(&ui).is_empty());
    held.store(1, Ordering::Relaxed);
    ui.frames(40, Vec::new);
    assert_eq!(overlays(&ui), [json!(menu)]);
    ui.frames(40, Vec::new);
    assert_eq!(overlays(&ui), [json!(menu)]);
    held.store(0, Ordering::Relaxed);
    ui.frames(2, Vec::new);
    assert!(overlays(&ui).is_empty());
    assert_eq!(ui.app.inspect()["presenter"]["current"], json!(base));
    assert!(polls.load(Ordering::Relaxed) >= 100, "the production Services polling path actually ran");
}

#[test]
fn prototype_key_timing_legacy_immediate_keypress_remains_an_actual_input_control() {
    let mut s = Session::new();
    let base = frame(&mut s, "Original legacy base", 0.0);
    let other = frame(&mut s, "Original legacy next", 400.0);
    ex(
        &mut s,
        "proto.interaction.add",
        json!({"id":base,"trigger":{"type":"keyPress","key":"A"},"action":{"type":"navigate","dest":other},"transition":{"type":"instant"}}),
    );
    let mut ui = Harness::new(s);
    ui.run("proto.present", json!({"start":base}));
    ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
    assert_eq!(ui.app.inspect()["presenter"]["current"], json!(other));
}

#[test]
fn prototype_key_timing_a_closed_and_reopened_same_id_overlay_does_not_inherit_a_hold() {
    let mut s = Session::new();
    let base = frame(&mut s, "Original owner base", 0.0);
    let menu = frame(&mut s, "Original owner menu", 400.0);
    let nested = frame(&mut s, "Original new owner result", 800.0);
    on(&mut s, base, "keyPress", "Q", 0, json!({"type":"openOverlay","dest":menu}));
    on(&mut s, menu, "keyDown", "A", 500, json!({"type":"openOverlay","dest":nested}));
    ex(
        &mut s,
        "proto.interaction.add",
        json!({"id":menu,"trigger":{"type":"keyPress","key":"C"},"actions":[
        {"type":"closeOverlay"},{"type":"openOverlay","dest":menu}
    ],"transition":{"type":"instant"}}),
    );
    let mut ui = Harness::new(s);
    ui.run("proto.present", json!({"start":base}));
    ui.frame(vec![key(Key::Q, true, false, Modifiers::NONE), key(Key::Q, false, false, Modifiers::NONE)]);
    assert_eq!(overlays(&ui), [json!(menu)]);
    ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
    ui.frames(15, Vec::new);
    ui.frame(vec![key(Key::C, true, false, Modifiers::NONE), key(Key::C, false, false, Modifiers::NONE)]);
    assert_eq!(overlays(&ui), [json!(menu)], "one frame closes the owner and opens a new instance with the same node id");
    ui.frames(40, Vec::new);
    assert_eq!(overlays(&ui), [json!(menu)], "the replaced overlay must not inherit its predecessor's held input");
    ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
    ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
    ui.frames(40, Vec::new);
    assert_eq!(overlays(&ui), [json!(menu), json!(nested)], "a fresh hold acquires the new overlay instance");
}

#[test]
fn prototype_key_timing_controller_focus_regain_requires_a_new_physical_edge() {
    let (mut ui, _, menu) = pair("Gamepad A", 500);
    let held = Arc::new(AtomicU32::new(0));
    ui.app.services = Box::new(Pads { held: held.clone(), polls: Arc::new(AtomicUsize::new(0)) });
    ui.frames(2, Vec::new);
    held.store(1, Ordering::Relaxed);
    ui.frames(15, Vec::new);
    focus(&mut ui, false);
    focus(&mut ui, true);
    ui.frames(40, Vec::new);
    assert!(overlays(&ui).is_empty(), "focus regain adopts the already-held mask without inventing a down edge");
    held.store(0, Ordering::Relaxed);
    ui.frames(2, Vec::new);
    held.store(1, Ordering::Relaxed);
    ui.frames(40, Vec::new);
    assert_eq!(overlays(&ui), [json!(menu)]);
}

#[test]
fn prototype_key_timing_controller_in_the_preview_keeps_its_existing_pointer_independent_scope() {
    let (mut ui, base, menu) = pair("Gamepad A", 500);
    let tab = ui.app.ui.present_active.unwrap();
    crate::tabs::close_present(&mut ui.app, tab);
    ui.run("proto.preview", json!({"start":base}));
    let pointer = egui::pos2(20.0, 500.0);
    assert!(!ui.pane().contains(pointer));
    let held = Arc::new(AtomicU32::new(0));
    ui.app.services = Box::new(Pads { held: held.clone(), polls: Arc::new(AtomicUsize::new(0)) });
    ui.frames(2, || vec![Event::PointerMoved(pointer)]);
    held.store(1, Ordering::Relaxed);
    ui.frames(40, || vec![Event::PointerMoved(pointer)]);
    assert_eq!(ui.preview()["overlays"], json!([menu]), "a controller keeps working while the editor owns the pointer");
    held.store(0, Ordering::Relaxed);
    ui.frames(2, Vec::new);
    assert_eq!(ui.preview()["overlays"], json!([]));
}

#[derive(Default)]
struct Opened(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

impl Services for Opened {
    fn open_url(&mut self, url: &str) {
        self.0.lock().unwrap().push(url.to_owned());
    }
}

#[test]
fn prototype_key_timing_a_positive_hold_never_bypasses_the_link_notice_as_a_fresh_edge() {
    for phase in ["keyDown", "keyUp"] {
        for ms in [0, 500] {
            let mut s = Session::new();
            s.prefs.link_notice_skipped = true;
            let base = frame(&mut s, "Original link timing fixture", 0.0);
            on(&mut s, base, phase, "A", ms, json!({"type":"openUrl","url":"https://example.com/hold"}));
            let opened = Opened::default();
            let seen = opened.0.clone();
            let mut ui = Harness::new(s);
            ui.app.services = Box::new(opened);
            ui.run("proto.present", json!({"start":base}));
            ui.frame(vec![key(Key::A, true, false, Modifiers::NONE)]);
            ui.frames(40, Vec::new);
            ui.frame(vec![key(Key::A, false, false, Modifiers::NONE)]);
            if ms == 0 {
                assert_eq!(*seen.lock().unwrap(), ["https://example.com/hold"], "{phase}: zero hold is a fresh explicit edge");
            } else {
                assert!(seen.lock().unwrap().is_empty(), "{phase}: a held input is not a new request to leave the prototype");
                assert_eq!(ui.app.inspect()["presenter"]["linkNotice"], "https://example.com/hold");
            }
        }
    }
}
