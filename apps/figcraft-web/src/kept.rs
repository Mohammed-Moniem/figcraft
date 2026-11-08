//! The page's selective same-origin preferences: name and successfully joined files. Owner
//! credentials and all other preferences are deliberately excluded. Restoring never connects.

use figcraft_engine::Session;
use serde_json::json;

pub const STATE_KEY: &str = "figcraft.kept.v1";
pub const LEGACY_NAME_KEY: &str = "figcraft.userName";
pub const MAX_STATE_BYTES: usize = 512 * 1024;

/// Read failures are distinct from an absent or malformed item. The shell reports them and
/// prevents writes for this visit, so an unreadable old history cannot be overwritten as empty.
pub fn load(session: &mut Session, mut read: impl FnMut(&str) -> Result<Option<String>, ()>) -> (bool, bool) {
    let kept = read(STATE_KEY);
    let legacy = read(LEGACY_NAME_KEY);
    let readable = kept.is_ok() && legacy.is_ok();
    let restored = restore(session, kept.ok().flatten(), legacy.ok().flatten());
    (restored, readable)
}

/// Restore versioned state, or migrate the old name-only key. False reports unusable state
/// without including any payload in an error. The existing page/server/host preferences stay.
pub fn restore(session: &mut Session, kept: Option<String>, legacy_name: Option<String>) -> bool {
    let Some(kept) = kept else {
        restore_name(session, legacy_name);
        return true;
    };
    let value = (kept.len() <= MAX_STATE_BYTES).then(|| serde_json::from_str::<serde_json::Value>(&kept).ok()).flatten();
    let Some(value) = value.filter(|value| value["version"].as_u64() == Some(1)) else {
        restore_name(session, legacy_name);
        return false;
    };
    restore_name(session, value["name"].as_str().map(str::to_owned).or(legacy_name));
    // Use the engine's hostile-preferences validator; never deserialize the entire Prefs from
    // browser storage, which must not be able to replace owner keys or the host's server.
    if let Ok(prefs) = serde_json::from_value::<figcraft_engine::Prefs>(json!({"joined_files":value["joined_files"]})) {
        session.prefs.joined_files = prefs.joined_files;
    }
    true
}

/// Write one bounded selective snapshot atomically as one storage item. On failure the in-memory
/// change remains usable, but the old durable entry may return after reload (including Forget).
pub fn store(prefs: &figcraft_engine::Prefs, write: impl FnOnce(&str) -> bool) -> bool {
    store_named(&prefs.user_name, &prefs.joined_files, write)
}

/// The host can supply a visit-only author. Keep its locally restored name independently while
/// preserving this record's existing capability schema, validation and write-failure policy.
pub fn store_named(name: &str, joined_files: &[figcraft_engine::joined::JoinedFile], write: impl FnOnce(&str) -> bool) -> bool {
    let files = figcraft_engine::joined::persisted(joined_files);
    let value = json!({"version":1, "name":name, "joined_files":files}).to_string();
    value.len() <= MAX_STATE_BYTES && write(&value)
}

/// Put back the name the page kept, checked as one typed in Preferences: the page's
/// storage is anyone's to change.
pub fn restore_name(session: &mut Session, kept: Option<String>) {
    if let Some(name) = kept
        && let Err(e) = session.execute("pref.userName", &json!({"name": name}))
    {
        log::warn!("the kept name wasn't used: {e}");
    }
}

#[cfg(test)]
mod tests {
    use figcraft_engine::Session;
    use serde_json::json;

    use super::restore_name;
    use super::{MAX_STATE_BYTES, restore, store};

    fn remembered() -> figcraft_engine::Prefs {
        serde_json::from_value(json!({
            "user_name":"Synthetic visitor",
            "joined_files":[{"address":"wss://example.invalid/Shared/", "file":"f", "name":"Synthetic shared file", "link":"figcraft://join?server=wss://example.invalid/Shared/&file=f&token=synthetic-view-secret", "access":"view", "opened":123}],
            "publish_server":"wss://private.invalid/?key=synthetic-server-secret",
            "online_files":[{"server":"wss://private.invalid/", "file":"owner", "name":"Owned", "owner":"synthetic-owner-secret", "edit":"figcraft://join?server=wss://private.invalid/&file=owner&token=synthetic-edit-secret"}]
        })).unwrap()
    }

    #[test]
    fn selective_browser_state_survives_reload_without_owner_or_server_keys_or_connections() {
        assert_eq!(super::STATE_KEY, "figcraft.kept.v1", "the versioned storage key is stable across reloads");
        let before = remembered();
        let mut kept = String::new();
        assert!(store(&before, |v| {
            kept = v.into();
            true
        }));
        assert!(!kept.contains("synthetic-owner-secret") && !kept.contains("synthetic-server-secret"));
        assert!(!kept.contains("synthetic-edit-secret"));
        let mut after = Session::new();
        after.prefs.publish_server = "wss://host-selected.invalid/".into();
        after.prefs.number_wheel = true;
        assert!(restore(&mut after, Some(kept), None));
        assert_eq!(after.prefs.user_name, "Synthetic visitor");
        assert_eq!(after.prefs.joined_files, before.joined_files);
        assert!(after.prefs.online_files.is_empty());
        assert_eq!(after.prefs.publish_server, "wss://host-selected.invalid/");
        assert!(after.prefs.number_wheel);
        assert!(!after.live.available() && after.live.iter().next().is_none());
    }

    #[test]
    fn browser_forget_is_durable_only_when_the_storage_write_succeeds() {
        let prefs = remembered();
        let mut kept = String::new();
        assert!(store(&prefs, |v| {
            kept = v.into();
            true
        }));
        let old = kept.clone();
        let mut current = Session::new();
        assert!(restore(&mut current, Some(old.clone()), None));
        current.execute("collab.forgetJoined", &json!({"file":"f"})).unwrap();
        assert!(!store(&current.prefs, |_| false), "quota/unavailable storage must not claim a durable Forget");
        assert!(current.prefs.joined_files.is_empty(), "the local operation still works");
        let mut stale = Session::new();
        assert!(restore(&mut stale, Some(old), None));
        assert_eq!(stale.prefs.joined_files.len(), 1, "the failed write left the previous durable entry");
        assert!(store(&current.prefs, |v| {
            kept = v.into();
            true
        }));
        let mut fresh = Session::new();
        assert!(restore(&mut fresh, Some(kept), None));
        assert!(fresh.prefs.joined_files.is_empty());
    }

    #[test]
    fn browser_legacy_malformed_oversized_and_future_state_preserve_unrelated_preferences() {
        for data in [
            None,
            Some("broken synthetic-view-secret".into()),
            Some("x".repeat(MAX_STATE_BYTES + 1)),
            Some(json!({"version":2,"name":"Ignored"}).to_string()),
        ] {
            let valid = data.is_none();
            let mut session = Session::new();
            session.prefs = remembered();
            let before = session.prefs.clone();
            assert_eq!(restore(&mut session, data, Some(" Legacy visitor ".into())), valid);
            assert_eq!(session.prefs.user_name, "Legacy visitor");
            assert_eq!(session.prefs.online_files, before.online_files);
            assert_eq!(session.prefs.publish_server, before.publish_server);
            assert_eq!(session.prefs.joined_files, before.joined_files);
        }
        let mut session = Session::new();
        assert!(restore(
            &mut session,
            Some(json!({"version":1,"name":"Kept name","joined_files":[{"link":"broken"}],"publish_server":"ws://spoofed/"}).to_string()),
            None
        ));
        assert_eq!(session.prefs.user_name, "Kept name");
        assert!(session.prefs.joined_files.is_empty());
        assert_eq!(session.prefs.publish_server, Session::new().prefs.publish_server);
    }

    #[test]
    fn browser_storage_read_failures_are_not_mistaken_for_an_empty_history() {
        let mut session = Session::new();
        session.prefs = remembered();
        let before = session.prefs.clone();
        let (_, readable) = super::load(&mut session, |_| Err(()));
        assert!(!readable, "the shell must block writes and report the failed read");
        assert_eq!(session.prefs, before);
        let (restored, readable) = super::load(&mut session, |key| {
            assert!([super::STATE_KEY, super::LEGACY_NAME_KEY].contains(&key));
            Ok(None)
        });
        assert!(restored && readable, "an absent state is a usable new browser visit");
    }

    /// What one page kept, the next one restores; junk in the storage is cleaned like typing.
    #[test]
    fn the_name_survives_a_reload() {
        let mut before = Session::new();
        before.execute("pref.userName", &json!({"name": "  Alice "})).unwrap();
        let kept = before.prefs.user_name.clone();
        let mut after = Session::new();
        restore_name(&mut after, Some(kept));
        assert_eq!(after.prefs.user_name, "Alice");
        let mut fresh = Session::new();
        restore_name(&mut fresh, None);
        assert_eq!(fresh.prefs.user_name, Session::new().prefs.user_name, "nothing kept: the default");
        restore_name(&mut fresh, Some(format!("Bob\u{7}{}", "x".repeat(500))));
        assert!(fresh.prefs.user_name.starts_with("Bobx") && fresh.prefs.user_name.chars().count() == 64);
    }
}
