//! Preference commands used by the shortcut recorder and automation (#2036).

use super::CommandSpec;
use crate::keymap::Platform;
use crate::{Result, Session, bad, spec};
use serde_json::{Value, json};

fn platform(p: &Value, command: &str) -> Result<Platform> {
    match p.get("platform") {
        None => Ok(Platform::current()),
        Some(Value::String(value)) if value == "mac" => Ok(Platform::Mac),
        Some(Value::String(value)) if value == "other" => Ok(Platform::Other),
        _ => Err(bad(command, "platform must be mac or other")),
    }
}

fn change(s: &mut Session, p: &Value, reset: bool) -> Result<Value> {
    let command = if reset { "pref.resetShortcut" } else { "pref.shortcut" };
    let platform = platform(p, command)?;
    let replace = match p.get("replace") {
        None => false,
        Some(Value::Bool(value)) => *value,
        _ => return Err(bad(command, "replace must be a boolean")),
    };
    let before = s.prefs.shortcuts.clone();
    let conflicts = if reset {
        let id = match p.get("id") {
            None => None,
            Some(Value::String(id)) => Some(id.as_str()),
            _ => return Err(bad(command, "id must be a command id")),
        };
        s.prefs.shortcuts.reset(id, platform, replace)
    } else {
        let id = crate::params::req_string(command, p, "id")?;
        let keys = serde_json::from_value::<Vec<String>>(p.get("keys").cloned().unwrap_or(Value::Null))
            .map_err(|_| bad(command, "keys must be an array of shortcut strings"))?;
        s.prefs.shortcuts.set(&id, keys, platform, replace)
    }
    .map_err(|message| bad(command, message))?;
    let changed = s.prefs.shortcuts != before;
    s.prefs_unsaved |= changed;
    Ok(json!({"changed": changed, "conflicts": conflicts}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "pref.shortcut",
            "Set command shortcut",
            "{id, keys:[string], platform?:mac|other, replace?:bool}  [] unbinds; conflicts returned unchanged unless replace is true",
            false,
            |s, p| change(s, p, false)
        ),
        spec!(
            "pref.resetShortcut",
            "Reset command shortcuts",
            "{id?:string, platform?:mac|other, replace?:bool}  omit id to reset this platform; conflicts require replace",
            false,
            |s, p| change(s, p, true)
        ),
    ]
}
