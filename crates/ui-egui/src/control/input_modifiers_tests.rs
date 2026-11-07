//! Raw control gestures reach the same platform-specific bindings as physical keyboard input.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use egui::os::OperatingSystem;
use egui::{Event, Modifiers};
use figcraft_engine::Session;
use figcraft_engine::keymap::{self, Context, Platform};
use serde_json::{Value, json};

use crate::{FigApp, NoServices};

fn queued(os: OperatingSystem, event: Value) -> Vec<Event> {
    let ctx = egui::Context::default();
    ctx.set_os(os);
    let mut app = FigApp::new(Session::new(), Box::new(NoServices));
    super::handle(&mut app, &ctx, "ui.input", &json!({"events": [event]})).unwrap();
    crate::windows::take_input(&mut app, egui::ViewportId::ROOT)
}

fn pressed_modifiers(events: &[Event]) -> Modifiers {
    events
        .iter()
        .find_map(|e| match e {
            Event::Key { pressed: true, modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .expect("a queued key press")
}

fn binding(platform: Platform, key: &str, modifiers: Modifiers) -> Option<&'static str> {
    keymap::lookup(platform, Context::Canvas, key, crate::shortcuts::mods(modifiers)).map(|b| b.id)
}

#[test]
fn mac_raw_command_selects_the_copy_as_png_binding() {
    let events = queued(OperatingSystem::Mac, json!({"key": "C", "shift": true, "cmd": true}));
    let modifiers = pressed_modifiers(&events);
    assert!(modifiers.command && modifiers.mac_cmd && !modifiers.ctrl);
    assert_eq!(binding(Platform::Mac, "C", modifiers), Some("edit.copyAsPng"));
}

#[test]
fn mac_raw_control_does_not_alias_command() {
    let events = queued(OperatingSystem::Mac, json!({"key": "C", "shift": true, "ctrl": true}));
    let modifiers = pressed_modifiers(&events);
    assert!(modifiers.ctrl && !modifiers.command && !modifiers.mac_cmd);
    assert!(keymap::matches("Shift+Ctrl+C", Platform::Mac, "C", crate::shortcuts::mods(modifiers)));
    assert_ne!(binding(Platform::Mac, "C", modifiers), Some("edit.copyAsPng"));
}

#[test]
fn mac_raw_control_and_command_can_be_combined() {
    let events = queued(OperatingSystem::Mac, json!({"key": "M", "cmd": true, "ctrl": true}));
    let modifiers = pressed_modifiers(&events);
    assert!(modifiers.ctrl && modifiers.command && modifiers.mac_cmd);
    assert_eq!(binding(Platform::Mac, "M", modifiers), Some("object.mask"));
}

#[test]
fn other_platform_raw_command_and_control_select_the_primary_copy_binding() {
    for os in [OperatingSystem::Windows, OperatingSystem::Nix, OperatingSystem::Unknown] {
        for (cmd, ctrl) in [(true, false), (false, true), (true, true)] {
            let events = queued(os, json!({"key": "C", "shift": true, "cmd": cmd, "ctrl": ctrl}));
            let modifiers = pressed_modifiers(&events);
            assert!(modifiers.ctrl && modifiers.command && !modifiers.mac_cmd, "{os:?}: {cmd}/{ctrl}");
            assert_eq!(binding(Platform::Other, "C", modifiers), Some("edit.copyAsPng"), "{os:?}: {cmd}/{ctrl}");
        }
    }
}

#[test]
fn raw_holds_pointer_buttons_and_wheels_use_the_same_platform_modifiers() {
    for os in [OperatingSystem::Mac, OperatingSystem::Windows] {
        let expected = pressed_modifiers(&queued(os, json!({"key": "C", "shift": true, "cmd": true})));
        for event in [
            json!({"hold": {"shift": true, "cmd": true}}),
            json!({"down": [10, 20], "shift": true, "cmd": true}),
            json!({"scroll": [0, -10], "shift": true, "cmd": true}),
        ] {
            let events = queued(os, event.clone());
            let modifiers = events
                .iter()
                .find_map(|e| match e {
                    Event::ModifiersChanged(modifiers) | Event::PointerButton { modifiers, .. } | Event::MouseWheel { modifiers, .. } => {
                        Some(*modifiers)
                    }
                    _ => None,
                })
                .expect("a queued modifier event");
            assert_eq!(modifiers, expected, "{os:?}: {event}");
        }
    }
}
