//! Explicit nonsecret browser appearance/input preferences. This record cannot contain server,
//! owner, joined-capability, plugin, document, account or native-path data.

use figcraft_engine::{Prefs, Session, pixel_grid};
use serde_json::{Value, json};

pub const STATE_KEY: &str = "figcraft.preferences.v1";
// Covers both bounded shortcut platforms (512 commands, eight keys, 80 bytes per chord), plus
// their known command ids and the five scalar preferences. Check before parsing or writing.
pub const MAX_STATE_BYTES: usize = 1024 * 1024;

fn accent(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// An absent record is a new visit. An unreadable or unsupported record is retained verbatim:
/// report failure and block writes to this record for this visit, independently of joined files.
pub fn load(session: &mut Session, read: impl FnOnce(&str) -> Result<Option<String>, ()>) -> (bool, bool) {
    match read(STATE_KEY) {
        Ok(None) => (true, true),
        Ok(Some(value)) if restore(session, &value) => (true, true),
        _ => (false, false),
    }
}

pub fn restore(session: &mut Session, input: &str) -> bool {
    if input.len() > MAX_STATE_BYTES {
        return false;
    }
    let Ok(value) = serde_json::from_str::<Value>(input) else { return false };
    if value["version"].as_u64() != Some(1) {
        return false;
    }
    let defaults = Prefs::default();
    // Each field is explicit. Never deserialize stored data as all Prefs, which could replace
    // the host's server, private paths, owner keys or other settings.
    session.prefs.selection_outline_opacity = value["selection_outline_opacity"]
        .as_f64()
        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
        .unwrap_or_else(|| defaults.selection_outline_opacity());
    session.prefs.pixel_grid_size = value["pixel_grid_size"].as_f64().and_then(pixel_grid::valid_size).unwrap_or(pixel_grid::DEFAULT_SIZE);
    session.prefs.accent = value["accent"].as_str().and_then(Prefs::parse_accent).map(accent).unwrap_or(defaults.accent);
    session.prefs.number_wheel = value["number_wheel"].as_bool().unwrap_or(defaults.number_wheel);
    session.prefs.keep_menu_open = value["keep_menu_open"].as_bool().unwrap_or(defaults.keep_menu_open);
    // This existing typed reader enforces per-platform budgets, known commands, reserved keys
    // and conflicting owners; assigning raw shortcut maps would bypass those checks.
    session.prefs.shortcuts = serde_json::from_value(value["shortcuts"].clone()).unwrap_or_default();
    true
}

pub fn store(prefs: &Prefs, write: impl FnOnce(&str) -> bool) -> bool {
    let value = json!({
        "version":1,
        "selection_outline_opacity":prefs.selection_outline_opacity(),
        "pixel_grid_size":pixel_grid::size_or_default(prefs.pixel_grid_size),
        "accent":accent(prefs.accent_rgb()),
        "number_wheel":prefs.number_wheel,
        "keep_menu_open":prefs.keep_menu_open,
        "shortcuts":prefs.shortcuts,
    })
    .to_string();
    value.len() <= MAX_STATE_BYTES && write(&value)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use figcraft_engine::{Prefs, Session, keymap::Platform};
    use serde_json::{Value, json};

    fn configured() -> Session {
        let mut s = Session::new();
        s.prefs.selection_outline_opacity = 35.5;
        s.prefs.pixel_grid_size = 0.125;
        s.prefs.accent = "#0d99ff".into();
        s.prefs.number_wheel = true;
        for platform in [Platform::Mac, Platform::Other] {
            assert!(s.prefs.shortcuts.set("shape.rectangle", vec!["Alt+F8".into()], platform, false).unwrap().is_empty());
        }
        s
    }

    fn encode(prefs: &Prefs) -> String {
        let mut text = String::new();
        assert!(super::store(prefs, |value| {
            text = value.into();
            true
        }));
        text
    }

    #[test]
    fn appearance_record_has_exactly_six_nonsecret_fields_and_a_version() {
        let mut s = configured();
        s.prefs.publish_server = "wss://private.invalid/?token=synthetic-server-secret".into();
        s.prefs.recent_files = vec!["/synthetic/private/native-path.figcraft".into()];
        s.prefs.user_name = "synthetic-private-name".into();
        s.prefs.plugin_settings = BTreeMap::from([("plugin".into(), BTreeMap::from([("key".into(), json!("synthetic-plugin-secret"))]))]);
        let private: Prefs = serde_json::from_value(json!({
            "joined_files":[{"address":"wss://example.invalid/","file":"f","name":"Synthetic","link":"figcraft://join?server=wss://example.invalid/&file=f&token=synthetic-joined-secret"}],
            "online_files":[{"server":"wss://example.invalid/","file":"owner","name":"Synthetic","owner":"synthetic-owner-secret","edit":"figcraft://join?server=wss://example.invalid/&file=owner&token=synthetic-edit-secret"}]
        })).unwrap();
        s.prefs.joined_files = private.joined_files;
        s.prefs.online_files = private.online_files;
        assert_eq!(s.prefs.joined_files.len(), 1);
        assert_eq!(s.prefs.online_files.len(), 1);
        let text = encode(&s.prefs);
        for secret in [
            "synthetic-server-secret",
            "synthetic-joined-secret",
            "synthetic-owner-secret",
            "synthetic-edit-secret",
            "synthetic-plugin-secret",
            "synthetic-private-name",
            "native-path",
        ] {
            assert!(!text.contains(secret), "private data cannot enter the appearance record");
        }
        let value: Value = serde_json::from_str(&text).unwrap();
        let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["accent", "keep_menu_open", "number_wheel", "pixel_grid_size", "selection_outline_opacity", "shortcuts", "version"]);
        assert_eq!(value["version"], 1);
        assert_eq!(value["accent"], "#0D99FF");
        assert!(text.len() <= super::MAX_STATE_BYTES);
    }

    #[test]
    fn both_wheel_values_and_fractional_preferences_reload_without_document_or_host_changes() {
        for on in [true, false] {
            let mut before = configured();
            before.prefs.number_wheel = on;
            let text = encode(&before.prefs);
            let mut after = Session::new();
            after.prefs.number_wheel = !on;
            after.prefs.publish_server = "wss://host-selected.invalid/Proxy/".into();
            after.prefs.user_name = "Visit-only host author".into();
            after.prefs.recent_files = vec!["/synthetic/retained.figcraft".into()];
            after.prefs.theme = "dark".into();
            let document = after.doc().unwrap().clone();
            assert!(super::restore(&mut after, &text));
            assert_eq!(after.prefs.selection_outline_opacity(), 35.5);
            assert_eq!(after.prefs.pixel_grid_size, 0.125);
            assert_eq!(after.prefs.accent, "#0D99FF");
            assert_eq!(after.prefs.number_wheel, on);
            assert_eq!(after.prefs.shortcuts, before.prefs.shortcuts);
            assert_eq!(after.prefs.publish_server, "wss://host-selected.invalid/Proxy/");
            assert_eq!(after.prefs.user_name, "Visit-only host author");
            assert_eq!(after.prefs.recent_files, ["/synthetic/retained.figcraft"]);
            assert_eq!(after.prefs.theme, "dark");
            assert_eq!(after.doc().unwrap(), &document);
            assert_eq!(after.undo_depth(), (0, 0));
            assert!(!after.ds().unwrap().is_dirty());
            assert!(!after.live.available() && after.live.iter().next().is_none());
        }
    }

    #[test]
    fn malformed_scalar_fields_fall_back_independently_without_losing_valid_neighbors() {
        let encoded: Value = serde_json::from_str(&encode(&configured().prefs)).unwrap();
        for (field, invalid, expected) in [
            ("selection_outline_opacity", json!(101), json!(100.0)),
            ("selection_outline_opacity", json!(-1), json!(100.0)),
            ("selection_outline_opacity", json!("35.5"), json!(100.0)),
            ("selection_outline_opacity", Value::Null, json!(100.0)),
            ("pixel_grid_size", json!(0), json!(1.0)),
            ("pixel_grid_size", json!(1e300), json!(1.0)),
            ("pixel_grid_size", json!(0.124), json!(1.0)),
            ("pixel_grid_size", json!([]), json!(1.0)),
            ("accent", json!("#0D99FF80"), json!("#02DA96")),
            ("accent", json!("#GGGGGG"), json!("#02DA96")),
            ("accent", json!({}), json!("#02DA96")),
            ("number_wheel", json!("true"), json!(false)),
            ("number_wheel", json!(1), json!(false)),
            ("keep_menu_open", json!("true"), json!(false)),
            ("keep_menu_open", json!(1), json!(false)),
        ] {
            let mut value = encoded.clone();
            value[field] = invalid;
            value["publish_server"] = json!("wss://spoofed.invalid/");
            let mut s = Session::new();
            let original_server = s.prefs.publish_server.clone();
            assert!(super::restore(&mut s, &value.to_string()));
            let result: Value = serde_json::from_str(&encode(&s.prefs)).unwrap();
            assert_eq!(result[field], expected, "{field}");
            for neighbor in ["selection_outline_opacity", "pixel_grid_size", "accent", "number_wheel", "keep_menu_open", "shortcuts"] {
                if neighbor != field {
                    assert_eq!(result[neighbor], encoded[neighbor], "{field} cannot discard {neighbor}");
                }
            }
            assert_eq!(s.prefs.publish_server, original_server);
        }
    }

    #[test]
    fn absent_state_keeps_visit_preferences_and_unreadable_or_future_state_blocks_overwrite() {
        let mut s = configured();
        let before = s.prefs.clone();
        assert_eq!(super::load(&mut s, |_| Ok(None)), (true, true));
        assert_eq!(s.prefs, before);
        for raw in [
            "broken synthetic-private-secret".into(),
            json!({"version":2}).to_string(),
            "[]".into(),
            "null".into(),
            json!({"version":1.0}).to_string(),
        ] {
            assert_eq!(super::load(&mut s, |_| Ok(Some(raw))), (false, false));
            assert_eq!(s.prefs, before);
        }
        assert_eq!(super::load(&mut s, |_| Err(())), (false, false));
        assert_eq!(s.prefs, before);
    }

    #[test]
    fn appearance_input_budget_is_checked_before_parse_and_at_its_exact_boundary() {
        let base = "{\"version\":1}";
        let exact = format!("{base}{}", " ".repeat(super::MAX_STATE_BYTES - base.len()));
        let mut s = Session::new();
        assert!(super::restore(&mut s, &exact));
        let before = s.prefs.clone();
        assert!(!super::restore(&mut s, &(exact + " ")));
        assert_eq!(s.prefs, before);
    }

    #[test]
    fn stored_shortcut_validation_retains_both_platform_limits_reserved_keys_and_conflicts() {
        let mut value: Value = serde_json::from_str(&encode(&configured().prefs)).unwrap();
        value["shortcuts"] = json!({
            "mac":{
                "shape.rectangle":["Escape"],
                "file.revert":["Alt+F1","Alt+F2","Alt+F3","Alt+F4","Alt+F5","Alt+F6","Alt+F7","Alt+F8","Alt+F9"],
                "edit.undo":["x".repeat(81)],
                "missing.command":["Alt+F10"]
            },
            "other":{"shape.rectangle":["Alt+F8"],"file.revert":["Alt+F8"]}
        });
        let mut s = Session::new();
        assert!(super::restore(&mut s, &value.to_string()));
        assert_eq!(s.prefs.shortcuts, Prefs::default().shortcuts);
        assert_eq!(s.prefs.selection_outline_opacity(), 35.5);
        assert_eq!(s.prefs.pixel_grid_size, 0.125);
        assert_eq!(s.prefs.accent, "#0D99FF");
        assert!(s.prefs.number_wheel);
    }

    #[test]
    fn serializing_corrupt_programmatic_scalars_uses_finite_safe_defaults() {
        let mut s = configured();
        s.prefs.selection_outline_opacity = f64::NAN;
        s.prefs.pixel_grid_size = f64::INFINITY;
        s.prefs.accent = "synthetic-private-secret".into();
        let text = encode(&s.prefs);
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["selection_outline_opacity"], 100.0);
        assert_eq!(value["pixel_grid_size"], 1.0);
        assert_eq!(value["accent"], "#02DA96");
        assert!(!text.contains("synthetic-private-secret"));
    }

    #[test]
    fn menu_keep_open_optional_browser_field_roundtrips_and_defaults_off_for_legacy_records() {
        for value in [true, false] {
            let mut before = Session::new();
            let mut prefs = serde_json::to_value(&before.prefs).unwrap();
            prefs["keep_menu_open"] = json!(value);
            before.prefs = serde_json::from_value(prefs).unwrap();
            let text = encode(&before.prefs);
            let encoded: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(encoded["keep_menu_open"], value);
            let mut after = Session::new();
            assert!(super::restore(&mut after, &text));
            assert_eq!(serde_json::to_value(&after.prefs).unwrap()["keep_menu_open"], value);
            assert!(!after.ds().unwrap().is_dirty());
            assert_eq!(after.undo_depth(), (0, 0));
        }
        for invalid in [Value::Null, json!("true"), json!(1), json!({})] {
            let mut s = Session::new();
            assert!(super::restore(&mut s, &json!({"version":1,"keep_menu_open":invalid,"pixel_grid_size":0.125}).to_string()));
            assert_eq!(serde_json::to_value(&s.prefs).unwrap()["keep_menu_open"], false);
            assert_eq!(s.prefs.pixel_grid_size, 0.125);
        }
        let mut s = Session::new();
        assert!(super::restore(&mut s, r#"{"version":1}"#));
        assert_eq!(serde_json::to_value(&s.prefs).unwrap()["keep_menu_open"], false);
    }
}
