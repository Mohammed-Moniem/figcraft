//! Saved, platform-specific replacements around the immutable default table.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize};

use super::{Binding, Context, Group, KEYS, Mods, Platform, bindings, matches, normalize, parse};

const MAX_COMMANDS: usize = 512;
const MAX_KEYS: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Hash, Serialize)]
pub struct ShortcutOverrides {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    mac: BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    other: BTreeMap<String, Vec<String>>,
}

impl<'de> Deserialize<'de> for ShortcutOverrides {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // A bad hand-edited shortcut never discards the rest of the preferences. Bound the
        // retained data and ignore keys the app cannot dispatch (including reserved gestures).
        let v = serde_json::Value::deserialize(d)?;
        let mut out = Self::default();
        for (name, platform) in [("mac", Platform::Mac), ("other", Platform::Other)] {
            let Some(map) = v.get(name).and_then(|v| v.as_object()) else { continue };
            for (id, value) in map.iter().take(MAX_COMMANDS) {
                let Ok(keys) = serde_json::from_value::<Vec<String>>(value.clone()) else { continue };
                if validate(id, &keys, platform).is_ok() {
                    out.map_mut(platform).insert(id.clone(), keys);
                }
            }
            // Corrupt settings may contain two owners. Drop conflicting overrides rather than
            // showing a key on a command that can never receive it. Legitimate saved replacements
            // already contain the displaced owner's remaining keys (or an empty list).
            let invalid: Vec<String> =
                out.map(platform).iter().filter_map(|(id, keys)| (!out.conflicts(id, keys, platform).is_empty()).then_some(id.clone())).collect();
            for id in invalid {
                out.map_mut(platform).remove(&id);
            }
        }
        Ok(out)
    }
}

/// Canonical metadata for a keymap command, or a registered/menu command without a default key.
pub fn command_binding(id: &str) -> Option<Binding> {
    KEYS.iter().find(|b| b.id == id).copied().or_else(|| {
        let id = crate::find_command(id).map(|c| c.id).or_else(|| crate::catalog::items().find(|m| m.id == id && !id.is_empty()).map(|m| m.id))?;
        Some(Binding::new(Group::Essential, id, &[]))
    })
}

/// Escape always steps out of the current interaction and cancels shortcut recording.
pub fn editable(id: &str) -> bool {
    id != "select.none" && command_binding(id).is_some()
}

/// A captured chord in the same notation as the default table. Ctrl and Cmd are distinct on Mac.
pub fn chord(platform: Platform, key: &str, mut m: Mods) -> Option<String> {
    let (key, shift) = normalize(key)?;
    m.shift |= shift;
    let mut parts = Vec::new();
    if m.shift {
        parts.push("Shift".to_string());
    }
    if m.alt {
        parts.push("Alt".to_string());
    }
    if m.cmd || (platform == Platform::Other && m.ctrl) {
        parts.push(if platform == Platform::Mac { "Cmd" } else { "Ctrl" }.to_string());
    }
    if m.ctrl && platform == Platform::Mac {
        parts.push("Ctrl".to_string());
    }
    parts.push(if key.chars().count() == 1 { key.to_ascii_uppercase() } else { key.to_string() });
    Some(parts.join("+"))
}

fn overlaps(a: Context, b: Context) -> bool {
    a == b || a == Context::Both || b == Context::Both
}

fn same_key(a: &str, b: &str, platform: Platform) -> bool {
    parse(b).is_some_and(|(m, k)| matches(a, platform, k, m))
}

fn validate(id: &str, keys: &[String], platform: Platform) -> Result<(), &'static str> {
    let Some(binding) = command_binding(id) else { return Err("Unknown command") };
    if !editable(id) {
        return Err("Escape is reserved for cancelling interactions");
    }
    if keys.len() > MAX_KEYS {
        return Err("Too many shortcuts");
    }
    for key in keys {
        if key.len() > 80 {
            return Err("Shortcut is too long");
        }
        let Some((m, k)) = parse(key) else { return Err("Invalid shortcut") };
        let original = bindings(id, platform).any(|default| same_key(default, key, platform));
        if (k == "Escape" || k.starts_with("Arrow") || (k == "Space" || k.eq_ignore_ascii_case("Z")) && !m.cmd && !m.ctrl && !m.alt) && !original {
            return Err("This key is reserved for a canvas gesture");
        }
        // Clipboard events replace these keys before the app sees them. Their original owners
        // can keep them; another command cannot claim them. Text fields retain their OS keys.
        let native = (m.cmd || (platform == Platform::Other && m.ctrl)) && ["C", "X", "V"].iter().any(|x| k.eq_ignore_ascii_case(x));
        let contextual = matches!(k, "Enter" | "Delete" | "Tab") && !m.cmd && !m.ctrl && !m.alt;
        if (native || contextual) && super::lookup(platform, Context::Canvas, k, m).is_none_or(|b| b.id != id) {
            return Err("This key is reserved for native editing");
        }
        if binding.context != Context::Canvas && !m.cmd && !m.ctrl && !m.alt && !["view.nextRegion", "view.previousRegion"].contains(&id) {
            return Err("Text shortcuts need a modifier");
        }
    }
    Ok(())
}

impl ShortcutOverrides {
    fn map(&self, p: Platform) -> &BTreeMap<String, Vec<String>> {
        if p == Platform::Mac { &self.mac } else { &self.other }
    }
    fn map_mut(&mut self, p: Platform) -> &mut BTreeMap<String, Vec<String>> {
        if p == Platform::Mac { &mut self.mac } else { &mut self.other }
    }

    pub fn overridden(&self, id: &str, p: Platform) -> bool {
        self.map(p).contains_key(id)
    }

    /// All effective keys, borrowed from preferences or from the unchanged default table.
    pub fn keys<'a>(&'a self, id: &str, p: Platform) -> Vec<&'a str> {
        self.map(p).get(id).map_or_else(|| bindings(id, p).collect(), |keys| keys.iter().map(String::as_str).collect())
    }

    pub fn primary<'a>(&'a self, id: &str, p: Platform) -> Option<&'a str> {
        match self.map(p).get(id) {
            Some(keys) => keys.first().map(String::as_str),
            None => super::primary(id, p),
        }
    }

    pub fn lookup(&self, p: Platform, context: Context, key: &str, m: Mods) -> Option<Binding> {
        KEYS.iter()
            .copied()
            .chain(self.map(p).keys().filter(|id| !KEYS.iter().any(|b| b.id == id.as_str())).filter_map(|id| command_binding(id)))
            .find(|b| b.live_in(context) && self.keys(b.id, p).iter().any(|spec| matches(spec, p, key, m)))
    }

    /// Owners of a proposed replacement where the contexts overlap. Asking never mutates prefs.
    pub fn conflicts(&self, id: &str, keys: &[String], p: Platform) -> Vec<String> {
        let Some(binding) = command_binding(id) else { return Vec::new() };
        let ids: BTreeSet<&str> = KEYS.iter().map(|b| b.id).chain(self.map(p).keys().map(String::as_str)).collect();
        ids.into_iter()
            .filter(|other| *other != id)
            .filter(|other| command_binding(other).is_some_and(|b| overlaps(binding.context, b.context)))
            .filter(|other| self.keys(other, p).iter().any(|old| keys.iter().any(|key| same_key(old, key, p))))
            .map(str::to_string)
            .collect()
    }

    /// Returns conflicting ids, with no change, unless replacement was explicitly chosen.
    pub fn set(&mut self, id: &str, keys: Vec<String>, p: Platform, replace: bool) -> Result<Vec<String>, &'static str> {
        validate(id, &keys, p)?;
        self.apply(id, keys, p, replace, false)
    }

    fn apply(&mut self, id: &str, keys: Vec<String>, p: Platform, replace: bool, restoring: bool) -> Result<Vec<String>, &'static str> {
        let conflicts = self.conflicts(id, &keys, p);
        if !conflicts.is_empty() && !replace {
            return Ok(conflicts);
        }
        let additions = conflicts.iter().filter(|owner| !self.overridden(owner, p)).count() + usize::from(!self.overridden(id, p));
        // A reset removes the selected id's override after preserving displaced owners.
        let projected = self.map(p).len() + additions - usize::from(restoring);
        if projected > MAX_COMMANDS {
            return Err("Too many custom shortcuts");
        }
        for other in conflicts {
            let remaining: Vec<String> =
                self.keys(&other, p).into_iter().filter(|old| !keys.iter().any(|key| same_key(old, key, p))).map(str::to_string).collect();
            self.map_mut(p).insert(other, remaining);
        }
        if restoring {
            self.map_mut(p).remove(id);
        } else {
            self.map_mut(p).insert(id.to_string(), keys);
        }
        Ok(Vec::new())
    }

    pub fn reset(&mut self, id: Option<&str>, p: Platform, replace: bool) -> Result<Vec<String>, &'static str> {
        let Some(id) = id else {
            self.map_mut(p).clear();
            return Ok(Vec::new());
        };
        if command_binding(id).is_none() {
            return Err("Unknown command");
        }
        // These keys come from our own canonical table, not untrusted capture or JSON input.
        let keys = bindings(id, p).map(str::to_string).collect();
        self.apply(id, keys, p, replace, true)
    }
}
