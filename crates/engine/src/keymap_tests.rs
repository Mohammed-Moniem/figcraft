//! The keymap: every default shortcut parses, is unique where it's live, and names a command;
//! platform differences, key-name normalisation, display, and the commands that only keys reach
//! (opacity digits, the zoom tool, panel toggles).

use serde_json::{Value, json};

use crate::cmd::edit::opacity_from_digits;
use crate::keymap::{Context, GESTURES, Group, KEYS, Mods, Platform, display, lookup, matches, normalize, parse, primary};
use crate::params::id_of;
use crate::tools::ToolId;
use crate::{EngineError, Session};
use figcraft_doc::NodeId;

const PLATFORMS: [Platform; 2] = [Platform::Mac, Platform::Other];

#[test]
fn custom_shortcuts_persist_per_platform_without_changing_defaults() {
    let mut s = Session::new();
    let old: crate::Prefs = serde_json::from_value(json!({"nudge_small": 3})).unwrap();
    assert_eq!(old.nudge_small, 3.0);
    assert_eq!(serde_json::to_value(&old).unwrap()["shortcuts"], json!({}));
    ex(&mut s, "pref.shortcut", json!({"id": "file.revert", "platform": "mac", "keys": ["Alt+F8"]}));
    let saved = serde_json::to_value(&s.prefs).unwrap();
    assert_eq!(saved["shortcuts"]["mac"]["file.revert"], json!(["Alt+F8"]));
    let restored: crate::Prefs = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), saved);
    assert!(s.prefs_unsaved);
    assert_eq!(primary("file.save", Platform::Mac), Some("Cmd+S"));
    assert!(primary("file.revert", Platform::Mac).is_none());
}

#[test]
fn custom_shortcut_conflicts_require_explicit_replacement_and_resets_are_safe() {
    let mut s = Session::new();
    let before = s.prefs.clone();
    let conflict = ex(&mut s, "pref.shortcut", json!({"id": "file.revert", "platform": "other", "keys": ["Ctrl+S"]}));
    assert_eq!(conflict["conflicts"], json!(["file.save"]));
    assert_eq!(s.prefs, before, "asking about a conflict cannot overwrite its owner");
    ex(&mut s, "pref.shortcut", json!({"id": "file.revert", "platform": "other", "keys": ["Ctrl+S"], "replace": true}));
    let v = serde_json::to_value(&s.prefs).unwrap();
    assert_eq!(v["shortcuts"]["other"]["file.save"], json!([]));
    assert_eq!(v["shortcuts"]["other"]["file.revert"], json!(["Ctrl+S"]));
    let conflict = ex(&mut s, "pref.resetShortcut", json!({"id": "file.save", "platform": "other"}));
    assert_eq!(conflict["conflicts"], json!(["file.revert"]));
    ex(&mut s, "pref.resetShortcut", json!({"platform": "other"}));
    assert_eq!(serde_json::to_value(&s.prefs).unwrap()["shortcuts"], json!({}));
}

#[test]
fn custom_shortcuts_reject_unknown_or_reserved_input_atomically() {
    let mut s = Session::new();
    for p in [
        json!({"id": "no.such.command", "keys": ["F8"]}),
        json!({"id": "file.revert", "keys": ["Bogus"]}),
        json!({"id": "file.revert", "keys": ["Escape"]}),
        json!({"id": "file.revert", "keys": ["Cmd+C"]}),
        json!({"id": "file.revert", "keys": ["Space"]}),
        json!({"id": "file.revert", "keys": ["ArrowLeft"]}),
    ] {
        assert!(s.execute("pref.shortcut", &p).is_err(), "{p}");
        assert_eq!(s.prefs, crate::Prefs::default());
    }
}

#[test]
fn custom_shortcuts_resolve_context_platform_aliases_and_preserve_alternates() {
    let mut s = Session::new();
    ex(&mut s, "pref.shortcut", json!({"id": "file.revert", "platform": "mac", "keys": ["Cmd+K"], "replace": true}));
    assert_eq!(s.prefs.shortcuts.keys("app.actions", Platform::Mac), ["Cmd+/"]);
    assert_eq!(s.prefs.shortcuts.lookup(Platform::Mac, Context::Canvas, "K", Mods { cmd: true, ..Default::default() }).unwrap().id, "file.revert");
    assert_eq!(
        s.prefs.shortcuts.lookup(Platform::Mac, Context::Text, "K", Mods { cmd: true, ..Default::default() }).unwrap().id,
        "text.link",
        "the separate text-context owner survives a Canvas-only replacement"
    );
    assert_eq!(s.prefs.shortcuts.primary("app.actions", Platform::Other), Some("Cmd+K"));
    ex(&mut s, "pref.shortcut", json!({"id": "file.revert", "platform": "other", "keys": ["Ctrl+Shift+F8"]}));
    assert_eq!(
        s.prefs.shortcuts.lookup(Platform::Other, Context::Canvas, "F8", Mods { cmd: true, shift: true, ..Default::default() }).unwrap().id,
        "file.revert"
    );
    assert!(s.prefs.shortcuts.lookup(Platform::Mac, Context::Canvas, "F8", Mods { ctrl: true, shift: true, ..Default::default() }).is_none());
    ex(&mut s, "pref.resetShortcut", json!({"id": "app.actions", "platform": "mac", "replace": true}));
    assert_eq!(s.prefs.shortcuts.primary("app.actions", Platform::Mac), Some("Cmd+K"));
    assert!(s.prefs.shortcuts.primary("file.revert", Platform::Mac).is_none());
    assert_eq!(s.prefs.shortcuts.primary("file.revert", Platform::Other), Some("Ctrl+Shift+F8"));
}

#[test]
fn custom_shortcuts_bad_saved_data_preserves_other_preferences() {
    let prefs: crate::Prefs = serde_json::from_value(
        json!({"nudge_small": 7, "shortcuts": {"mac": {"file.revert": ["Escape"], "tool.rectangle": ["Alt+F8"], "bad.id": ["F9"]}}}),
    )
    .unwrap();
    assert_eq!(prefs.nudge_small, 7.0);
    assert_eq!(prefs.shortcuts.primary("tool.rectangle", Platform::Mac), Some("Alt+F8"));
    assert!(prefs.shortcuts.primary("file.revert", Platform::Mac).is_none());
    let prefs: crate::Prefs = serde_json::from_value(json!({"theme": "dark", "shortcuts": "broken"})).unwrap();
    assert_eq!(prefs.theme, "dark");
    assert_eq!(prefs.shortcuts, Default::default());
}

#[test]
fn custom_shortcuts_reserved_default_owner_can_reset_and_replacements_respect_budget() {
    let mut s = Session::new();
    ex(&mut s, "pref.shortcut", json!({"id": "proto.preview", "platform": "mac", "keys": ["Alt+F9"]}));
    ex(&mut s, "pref.resetShortcut", json!({"id": "proto.preview", "platform": "mac"}));
    assert_eq!(s.prefs.shortcuts.primary("proto.preview", Platform::Mac), Some("Shift+Space"));
    ex(&mut s, "pref.shortcut", json!({"id": "proto.preview", "platform": "mac", "keys": ["Shift+Space"]}));
    assert_eq!(serde_json::from_value::<crate::Prefs>(serde_json::to_value(&s.prefs).unwrap()).unwrap(), s.prefs);
    let mut map = serde_json::Map::new();
    for command in crate::all_commands().iter().filter(|c| !["file.revert", "file.save", "file.open", "select.none"].contains(&c.id)).take(511) {
        map.insert(command.id.to_string(), json!([]));
    }
    assert_eq!(map.len(), 511, "enough real registered commands exercise the saved override budget");
    s.prefs = serde_json::from_value(json!({"shortcuts": {"mac": map}})).unwrap();
    let before = s.prefs.clone();
    assert!(s.execute("pref.shortcut", &json!({"id": "file.revert", "platform": "mac", "keys": ["Cmd+S", "Cmd+O"], "replace": true})).is_err());
    assert_eq!(s.prefs, before, "displaced owners count toward the budget before any mutation");
}

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn rect(s: &mut Session, x: f64, y: f64) -> NodeId {
    id_of(&ex(s, "shape.rectangle", json!({"x": x, "y": y, "width": 10, "height": 10}))["id"]).unwrap()
}

fn mods(shift: bool, alt: bool, cmd: bool, ctrl: bool) -> Mods {
    Mods { shift, alt, cmd, ctrl }
}

fn id_for(p: Platform, c: Context, key: &str, m: Mods) -> Option<&'static str> {
    lookup(p, c, key, m).map(|b| b.id)
}

#[test]
fn every_binding_parses_and_names_a_command() {
    for b in KEYS {
        assert!(!b.mac.is_empty(), "{} has no keys", b.id);
        for p in PLATFORMS {
            for k in b.keys(p) {
                assert!(parse(k).is_some(), "{}: `{k}` doesn't parse", b.id);
                assert!(!display(k, p).is_empty(), "{}: `{k}`", b.id);
            }
        }
        // An engine command, or a menu item the UI handles (file dialogs, clipboard formats).
        let in_menu = crate::catalog::items().any(|m| m.id == b.id);
        assert!(crate::find_command(b.id).is_some() || in_menu, "{} is neither a command nor a menu item", b.id);
        assert!(!b.title().is_empty());
    }
    assert!(Group::ALL.iter().all(|g| KEYS.iter().any(|b| b.group == *g)), "every panel tab has shortcuts");
    assert!(GESTURES.iter().all(|(_, l, m, o)| !l.is_empty() && !m.is_empty() && !o.is_empty()));
}

/// The chord a binding fires on, as the platform sees it (Ctrl and Cmd are one key off macOS).
fn chord(p: Platform, spec: &str) -> (bool, bool, bool, bool, String) {
    let (m, k) = parse(spec).unwrap();
    let (cmd, ctrl) = match p {
        Platform::Mac => (m.cmd, m.ctrl),
        Platform::Other => (m.cmd || m.ctrl, false),
    };
    (m.shift, m.alt, cmd, ctrl, k.to_ascii_uppercase())
}

fn overlap(a: Context, b: Context) -> bool {
    a == b || a == Context::Both || b == Context::Both
}

#[test]
fn no_two_commands_share_a_chord_where_both_are_live() {
    for p in PLATFORMS {
        for (i, a) in KEYS.iter().enumerate() {
            for b in KEYS.iter().skip(i + 1).filter(|b| b.id != a.id && overlap(a.context, b.context)) {
                for ka in a.keys(p) {
                    for kb in b.keys(p) {
                        assert_ne!(chord(p, ka), chord(p, kb), "{p:?}: `{ka}` ({}) and `{kb}` ({}) collide", a.id, b.id);
                    }
                }
            }
        }
    }
}

#[test]
fn key_names_normalise() {
    assert_eq!(normalize("Plus"), Some(("=", true)));
    assert_eq!(normalize("Questionmark"), Some(("/", true)));
    assert_eq!(normalize("Equals"), Some(("=", false)));
    assert_eq!(normalize("Backspace"), Some(("Delete", false)));
    assert_eq!(normalize("Left"), Some(("ArrowLeft", false)));
    assert_eq!(normalize("Exclamationmark"), Some(("1", true)));
    assert_eq!(normalize("PageDown"), Some(("PageDown", false)));
    assert_eq!(normalize("Bogus"), None);
    assert_eq!(normalize(""), None);
    assert_eq!(parse("Ctrl+Shift+?").map(|(m, k)| (m.shift, m.ctrl, k)), Some((true, true, "/")));
    assert!(parse("Cmd+").is_none());
    assert!(parse("A+B").is_none());
}

#[test]
fn lookups_follow_the_platform_and_context() {
    lookups_on_every_platform();
    lookups_that_differ_by_platform();
}

fn lookups_on_every_platform() {
    let none = Mods::default();
    let shift = mods(true, false, false, false);
    for p in PLATFORMS {
        assert_eq!(id_for(p, Context::Canvas, "0", shift), Some("view.zoom100"));
        assert_eq!(id_for(p, Context::Canvas, "Exclamationmark", shift), Some("view.zoomFit"));
        assert_eq!(id_for(p, Context::Canvas, "2", shift), Some("view.zoomSelection"));
        // `+` comes as its own key, with or without Shift held.
        assert_eq!(id_for(p, Context::Canvas, "Plus", none), Some("view.zoomIn"));
        assert_eq!(id_for(p, Context::Canvas, "Plus", shift), Some("view.zoomIn"));
        assert_eq!(id_for(p, Context::Canvas, "Minus", none), Some("view.zoomOut"));
        assert_eq!(id_for(p, Context::Canvas, "A", mods(false, true, false, false)), Some("align.left"));
        assert_eq!(id_for(p, Context::Canvas, "Slash", mods(false, true, false, false)), Some("object.removeFill"));
        assert_eq!(id_for(p, Context::Canvas, "Questionmark", shift), Some("object.removeStroke"));
        assert_eq!(id_for(p, Context::Canvas, "U", mods(true, true, false, false)), Some("bool.union"));
        assert_eq!(id_for(p, Context::Canvas, "V", none), Some("tool.move"));
        // Cmd+K opens Actions on the canvas and makes a link while editing text.
        assert_eq!(id_for(p, Context::Canvas, "K", mods(false, false, true, false)), Some("app.actions"));
        assert_eq!(id_for(p, Context::Text, "K", mods(false, false, true, false)), Some("text.link"));
        // Formatting works in both; tools and canvas commands don't fire while typing.
        assert_eq!(id_for(p, Context::Text, "B", mods(false, false, true, false)), Some("text.bold"));
        assert_eq!(id_for(p, Context::Canvas, "B", mods(false, false, true, false)), Some("text.bold"));
        assert_eq!(id_for(p, Context::Text, "V", none), None);
        assert_eq!(id_for(p, Context::Text, "A", mods(false, true, false, false)), None);
    }
}

fn lookups_that_differ_by_platform() {
    let mac = Platform::Mac;
    let other = Platform::Other;
    // Use as mask: ⌃⌘M on macOS, Ctrl+Alt+M elsewhere.
    assert_eq!(id_for(mac, Context::Canvas, "M", mods(false, false, true, true)), Some("object.mask"));
    assert_eq!(id_for(other, Context::Canvas, "M", mods(false, true, true, true)), Some("object.mask"));
    // Bring to front: ⌥⌘] on macOS, Ctrl+Shift+] elsewhere (`}` with Shift).
    assert_eq!(id_for(mac, Context::Canvas, "CloseBracket", mods(false, true, true, false)), Some("arrange.front"));
    assert_eq!(id_for(other, Context::Canvas, "CloseCurlyBracket", mods(true, false, true, true)), Some("arrange.front"));
    // Distribute and tidy up differ too.
    assert_eq!(id_for(mac, Context::Canvas, "H", mods(false, true, false, true)), Some("distribute.horizontal"));
    assert_eq!(id_for(other, Context::Canvas, "H", mods(true, true, false, false)), Some("distribute.horizontal"));
    assert_eq!(id_for(mac, Context::Canvas, "T", mods(false, true, false, true)), Some("arrange.tidy"));
    assert_eq!(id_for(other, Context::Canvas, "T", mods(true, true, true, true)), Some("arrange.tidy"));
    assert_eq!(id_for(other, Context::Canvas, "T", mods(false, true, true, true)), Some("text.alignCenter"));
    macos_tells_ctrl_from_cmd(mac, other);
}

fn macos_tells_ctrl_from_cmd(mac: Platform, other: Platform) {
    // macOS tells ⌃ from ⌘: ⌃G is layout grids, ⌘G groups; ⌃C picks a colour, ⌘C copies.
    assert_eq!(id_for(mac, Context::Canvas, "G", mods(false, false, false, true)), Some("view.layoutGrids"));
    assert_eq!(id_for(mac, Context::Canvas, "G", mods(false, false, true, false)), Some("object.group"));
    assert_eq!(id_for(mac, Context::Canvas, "C", mods(false, false, false, true)), Some("edit.pickColor"));
    assert_eq!(id_for(other, Context::Canvas, "C", mods(false, false, true, true)), Some("edit.copy"));
    assert_eq!(id_for(other, Context::Canvas, "4", mods(true, false, true, true)), Some("view.layoutGrids"));
    // Off macOS Ctrl may come as cmd, ctrl or both.
    for (cmd, ctrl) in [(true, false), (false, true), (true, true)] {
        assert_eq!(id_for(other, Context::Canvas, "Z", mods(false, false, cmd, ctrl)), Some("edit.undo"));
    }
    assert!(matches("Shift+Cmd+O", mac, "O", mods(true, false, true, false)));
    assert!(!matches("Shift+Cmd+O", mac, "O", mods(true, false, true, true)), "an extra ⌃ is another chord on macOS");
}

#[test]
fn display_per_platform() {
    let (mac, other) = (Platform::Mac, Platform::Other);
    assert_eq!(display("Shift+Cmd+K", mac), "⇧⌘K");
    assert_eq!(display("Shift+Cmd+K", other), "Ctrl+Shift+K");
    assert_eq!(display("Ctrl+Cmd+M", mac), "⌃⌘M");
    assert_eq!(display("Ctrl+Alt+M", other), "Ctrl+Alt+M");
    assert_eq!(display("Alt+Cmd+Enter", mac), "⌥⌘↵");
    assert_eq!(display("Alt+Cmd+Enter", other), "Ctrl+Alt+Enter");
    assert_eq!(display("Cmd+=", mac), "⌘+");
    assert_eq!(display("Shift+=", other), "+");
    assert_eq!(display("Delete", mac), "⌫");
    assert_eq!(display("Alt", other), "Alt");
    assert_eq!(display("Cmd", mac), "⌘");
    assert_eq!(display("", mac), "");
    assert_eq!(primary("object.mask", other), Some("Ctrl+Alt+M"));
    assert_eq!(primary("object.mask", mac), Some("Ctrl+Cmd+M"));
    assert_eq!(primary("view.outlines", mac), Some("Shift+Cmd+O"));
    assert_eq!(primary("polygon.none", mac), None);
}

/// Each way of zooming in has its zoom-out twin on the key beside it, as the keys arrive from a US
/// keyboard (#973: `-` alone zoomed out but `=` alone did nothing, so zooming back in was stuck).
#[test]
fn zoom_keys_come_in_pairs() {
    let none = Mods::default();
    let shift = mods(true, false, false, false);
    let cmd = mods(false, false, true, false);
    let shift_cmd = mods(true, false, true, false);
    for p in PLATFORMS {
        for (zoom_in, zoom_out, m) in [("Equals", "Minus", none), ("Equals", "Minus", cmd), ("Plus", "Minus", shift_cmd), ("Plus", "Minus", shift)] {
            assert_eq!(id_for(p, Context::Canvas, zoom_in, m), Some("view.zoomIn"), "{zoom_in} {m:?} on {p:?}");
            assert_eq!(id_for(p, Context::Canvas, zoom_out, m), Some("view.zoomOut"), "{zoom_out} {m:?} on {p:?}");
        }
        // The menus still show Cmd/Ctrl and `+`.
        assert_eq!(primary("view.zoomIn", p), Some("Cmd+="));
        assert_eq!(primary("view.zoomOut", p), Some("Cmd+-"));
    }
}

#[test]
fn tool_keys_come_from_the_keymap() {
    assert_eq!(ToolId::Move.shortcut(), "V");
    assert_eq!(ToolId::Section.shortcut(), "Shift+S");
    assert_eq!(ToolId::Eyedropper.shortcut(), "I");
    assert_eq!(ToolId::Polygon.shortcut(), "");
    assert_eq!(crate::catalog::shortcut("tool.text"), Some("T"));
}

#[test]
fn opacity_digits() {
    assert_eq!(opacity_from_digits("5"), Some(0.5));
    assert_eq!(opacity_from_digits("0"), Some(1.0));
    assert_eq!(opacity_from_digits("45"), Some(0.45));
    assert_eq!(opacity_from_digits("05"), Some(0.05));
    assert_eq!(opacity_from_digits("00"), Some(0.0));
    assert_eq!(opacity_from_digits(""), None);
    assert_eq!(opacity_from_digits("123"), None);
    assert_eq!(opacity_from_digits("9999999999"), None, "ten digits must not overflow u32");
    assert_eq!(opacity_from_digits("4x"), None);
    assert_eq!(opacity_from_digits("٣"), None, "ASCII digits only");

    let mut s = Session::new();
    let a = rect(&mut s, 0.0, 0.0);
    let b = rect(&mut s, 20.0, 0.0);
    ex(&mut s, "select.set", json!({"ids": [a, b]}));
    ex(&mut s, "object.opacityKeys", json!({"digits": "3"}));
    let op = |s: &Session, id| s.doc().unwrap().node(id).unwrap().opacity;
    assert_eq!((op(&s, a), op(&s, b)), (0.3, 0.3));
    ex(&mut s, "object.opacityKeys", json!({"digits": "45", "ids": [a]}));
    assert_eq!((op(&s, a), op(&s, b)), (0.45, 0.3));
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(op(&s, a), 0.3, "one undo step");
    assert!(s.execute("object.opacityKeys", &json!({"digits": "123"})).is_err());
    assert!(
        matches!(s.execute("object.opacityKeys", &json!({"digits": "9999999999"})), Err(EngineError::BadParams { .. })),
        "an overflowing digit string is a parameter error, not a panic"
    );
    assert!(s.execute("object.opacityKeys", &json!({})).is_err());
    ex(&mut s, "select.none", json!({}));
    assert!(s.execute("object.opacityKeys", &json!({"digits": "1"})).is_err(), "nothing selected");
}

#[test]
fn zoom_tool_steps_about_a_point_and_fits_an_area() {
    let mut s = Session::new();
    ex(&mut s, "view.set", json!({"zoom": 1.0, "cx": 0, "cy": 0, "vw": 1000, "vh": 800}));
    // A click at (100, 50) zooms in a step and keeps that point where it was on screen.
    let before = s.ds().unwrap().view.page_to_screen(100.0, 50.0);
    let v = ex(&mut s, "view.zoomAt", json!({"x": 100, "y": 50}));
    assert_eq!(v["zoom"], 2.0);
    let after = s.ds().unwrap().view.page_to_screen(100.0, 50.0);
    assert!((before.0 - after.0).abs() < 1e-6 && (before.1 - after.1).abs() < 1e-6);
    let v = ex(&mut s, "view.zoomAt", json!({"x": 100, "y": 50, "out": true}));
    assert_eq!(v["zoom"], 1.0);
    // A dragged area fills the view.
    let v = ex(&mut s, "view.zoomAt", json!({"x": 0, "y": 0, "width": 100, "height": 50}));
    assert!(v["zoom"].as_f64().unwrap() > 5.0);
    assert_eq!((v["cx"].as_f64(), v["cy"].as_f64()), (Some(50.0), Some(25.0)));
    // Degenerate areas zoom a step instead; junk is refused.
    let z = s.ds().unwrap().view.zoom;
    ex(&mut s, "view.zoomAt", json!({"x": 0, "y": 0, "width": 0, "height": 50}));
    assert!(s.ds().unwrap().view.zoom > z);
    assert!(s.execute("view.zoomAt", &json!({"x": "a", "y": 0})).is_err());
    assert!(s.execute("view.zoomAt", &json!({"y": 0})).is_err());
}

#[test]
fn panel_toggles_request_the_ui() {
    let mut s = Session::new();
    for (cmd, kind) in
        [("view.designPrototype", "panel.designPrototype"), ("view.devMode", "panel.devMode"), ("view.leftSidebar", "panel.leftSidebar")]
    {
        s.ui_requests.clear();
        ex(&mut s, cmd, json!({}));
        assert!(s.ui_requests.iter().any(|r| r.kind == kind), "{cmd}");
    }
}

#[test]
fn vector_edit_paint_bucket_is_b() {
    use crate::vectoredit::VTool;
    let mut s = Session::new();
    let r = rect(&mut s, 0.0, 0.0);
    ex(&mut s, "vector.edit.begin", json!({"id": r}));
    assert!(crate::vectoredit::key(&mut s, "B", false, false, false).unwrap());
    assert_eq!(s.ds().unwrap().vector_edit.as_ref().unwrap().tool, VTool::Paint);
    assert_eq!(VTool::Paint.shortcut(), "Shift+B");
}

#[test]
fn ctrl_tab_cycles_documents() {
    let mut s = Session::new();
    ex(&mut s, "file.new", json!({}));
    ex(&mut s, "file.new", json!({}));
    let n = s.docs.len();
    assert!(n >= 3);
    let start = s.active;
    ex(&mut s, "doc.nextTab", json!({}));
    assert_eq!(s.active, (start + 1) % n);
    ex(&mut s, "doc.previousTab", json!({}));
    ex(&mut s, "doc.previousTab", json!({}));
    assert_eq!(s.active, (start + n - 1) % n, "wraps around");
    assert_eq!(lookup(Platform::Other, Context::Canvas, "Tab", mods(false, false, true, true)).map(|b| b.id), Some("doc.nextTab"));
    assert_eq!(lookup(Platform::Mac, Context::Canvas, "Tab", mods(true, false, false, true)).map(|b| b.id), Some("doc.previousTab"));
}

/// "Adjust the scale of the Figma UI [...] Windows: Reset to Default: Control ⇧Shift Alt 0; Make
/// Larger: Control ⇧Shift Alt =; Make Smaller: Control ⇧Shift Alt -" (Accessibility at Figma; ⌘ on
/// a Mac). The steps go through the sizes Preferences offers, stop at the ends, and are kept at once.
#[test]
fn the_interface_size_keys_step_through_the_preferences_sizes() {
    let m = mods(true, true, true, false);
    for p in PLATFORMS {
        for c in [Context::Canvas, Context::Text] {
            assert_eq!(id_for(p, c, "=", m), Some("pref.uiScaleUp"), "{p:?} {c:?}");
            assert_eq!(id_for(p, c, "-", m), Some("pref.uiScaleDown"), "{p:?} {c:?}");
            assert_eq!(id_for(p, c, "0", m), Some("pref.uiScaleReset"), "{p:?} {c:?}");
        }
    }
    let mut s = Session::new();
    let scale = |s: &Session| s.prefs.ui_scale;
    ex(&mut s, "pref.uiScale", json!({"scale": 1}));
    s.prefs_unsaved = false;
    ex(&mut s, "pref.uiScaleUp", json!({}));
    assert_eq!(scale(&s), 1.1);
    assert!(s.prefs_unsaved, "kept at once");
    ex(&mut s, "pref.uiScaleUp", json!({}));
    assert_eq!(scale(&s), 1.25);
    ex(&mut s, "pref.uiScaleDown", json!({}));
    assert_eq!(scale(&s), 1.1);
    assert_eq!(ex(&mut s, "pref.uiScaleReset", json!({}))["scale"], json!(1.0));
    assert_eq!(scale(&s), 1.0);
    // Between two sizes a step lands on the next one; at an end it stays.
    ex(&mut s, "pref.uiScale", json!({"scale": 1.3}));
    ex(&mut s, "pref.uiScaleUp", json!({}));
    assert_eq!(scale(&s), 1.5);
    ex(&mut s, "pref.uiScale", json!({"scale": 1.3}));
    ex(&mut s, "pref.uiScaleDown", json!({}));
    assert_eq!(scale(&s), 1.25);
    ex(&mut s, "pref.uiScale", json!({"scale": 2}));
    ex(&mut s, "pref.uiScaleUp", json!({}));
    assert_eq!(scale(&s), 2.0);
    ex(&mut s, "pref.uiScale", json!({"scale": 0.75}));
    ex(&mut s, "pref.uiScaleDown", json!({}));
    assert_eq!(scale(&s), 0.5);
    ex(&mut s, "pref.uiScaleDown", json!({}));
    assert_eq!(scale(&s), 0.5);
}

/// The Layers panel's Windows key is Ctrl+1 in one help-center article ("Mac: Option 1 Windows:
/// Ctrl 1", Select layers and objects) and Alt+1 in another (Explore the navigation bar and left
/// sidebar): both open it there. macOS keeps Option+1 alone; Cmd+1 is not documented.
#[test]
fn the_layers_panel_answers_both_of_its_documented_windows_keys() {
    let (alt, ctrl, cmd) = (mods(false, true, false, false), mods(false, false, false, true), mods(false, false, true, false));
    assert_eq!(id_for(Platform::Other, Context::Canvas, "1", alt), Some("view.layers"));
    assert_eq!(id_for(Platform::Other, Context::Canvas, "1", ctrl), Some("view.layers"));
    assert_eq!(id_for(Platform::Mac, Context::Canvas, "1", alt), Some("view.layers"));
    assert_eq!(id_for(Platform::Mac, Context::Canvas, "1", cmd), None);
}
