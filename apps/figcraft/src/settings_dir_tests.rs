//! Where this modified build's settings live: a portable copy keeps its own profile next to the
//! executable; an installed copy uses FigCraftLocal and leaves the upstream FigCraft profile alone. Every
//! folder here is a temporary one: the probe sees no real environment, executable or profile.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use craft_settings::Probe;
use figcraft_engine::Prefs;

use crate::services::{Desktop, settings_dir, write_prefs};

/// A fresh temporary folder with an `app` folder (the executable's) and an `appdata` folder (the
/// platform's config folder).
fn root(name: &str) -> PathBuf {
    let r = std::env::temp_dir().join(format!("figcraft-settings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&r);
    std::fs::create_dir_all(r.join("app")).unwrap();
    std::fs::create_dir_all(r.join("appdata")).unwrap();
    r
}

/// What the rules see: the executable in `root/app`, the platform folder `platform`, and
/// `FIGCRAFTLOCAL_CONFIG_DIR` set to `config_dir` when given.
fn probe(root: &Path, platform: Option<PathBuf>, config_dir: Option<&'static str>) -> Probe {
    Probe {
        env: Box::new(move |name| config_dir.filter(|_| name == "FIGCRAFTLOCAL_CONFIG_DIR").map(OsString::from)),
        exe_dir: Some(root.join("app")),
        platform_config_dir: platform,
    }
}

/// Preferences with a name to recognise them by.
fn prefs(name: &str) -> Prefs {
    Prefs { user_name: name.into(), ..Prefs::default() }
}

/// The user name of the preferences the app starts with from `dir`.
fn loaded_name(dir: PathBuf) -> Option<String> {
    Desktop::load_from(Some(dir), Arc::new(Mutex::new(None))).prefs.map(|p| p.user_name)
}

#[test]
fn direct_launch_uses_a_separate_profile_and_preserves_upstream_settings() {
    let root = root("installed");
    let old = root.join("appdata").join("FigCraft");
    write_prefs(&old, &prefs("Ada"));
    let dir = settings_dir(&probe(&root, Some(root.join("appdata")), None));
    let local = root.join("appdata").join("FigCraftLocal");
    assert_eq!(dir, Some(local.clone()));
    assert_eq!(loaded_name(local.clone()), None, "the modified build starts with its own profile");
    write_prefs(&local, &prefs("Grace"));
    assert_eq!(loaded_name(local), Some("Grace".into()));
    assert_eq!(loaded_name(old), Some("Ada".into()), "upstream preferences remain untouched");
    assert!(!root.join("app").join("config").exists(), "nothing is written next to the executable");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_portable_marker_keeps_the_settings_next_to_the_executable_and_takes_the_old_ones() {
    let root = root("portable");
    let old = root.join("appdata").join("FigCraftLocal");
    write_prefs(&old, &prefs("Ada"));
    std::fs::write(root.join("app").join("portable"), "").unwrap();
    let portable = root.join("app").join("config");
    let dir = settings_dir(&probe(&root, Some(root.join("appdata")), None));
    assert_eq!(dir, Some(portable.clone()));
    assert_eq!(loaded_name(portable.clone()), Some("Ada".into()), "the copy starts with the settings it had");
    assert_eq!(loaded_name(old.clone()), Some("Ada".into()), "the installed app's stay where they were");
    // Later starts keep the portable copy's own settings.
    write_prefs(&portable, &prefs("Grace"));
    assert_eq!(settings_dir(&probe(&root, Some(root.join("appdata")), None)), Some(portable.clone()));
    assert_eq!(loaded_name(portable), Some("Grace".into()), "never overwritten by the old ones again");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_new_portable_copy_starts_without_settings() {
    let root = root("fresh");
    std::fs::write(root.join("app").join("portable"), "").unwrap();
    let portable = root.join("app").join("config");
    assert_eq!(settings_dir(&probe(&root, Some(root.join("appdata")), None)), Some(portable.clone()));
    assert_eq!(loaded_name(portable), None, "a first run");
    // No platform folder at all: the marker still decides.
    assert_eq!(settings_dir(&probe(&root, None, None)), Some(root.join("app").join("config")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn modified_config_dir_names_the_folder_and_empty_means_none() {
    let root = root("env");
    std::fs::write(root.join("app").join("portable"), "").unwrap();
    let named = settings_dir(&probe(&root, Some(root.join("appdata")), Some("elsewhere")));
    assert_eq!(named, Some(PathBuf::from("elsewhere")), "ahead of the marker");
    assert_eq!(settings_dir(&probe(&root, Some(root.join("appdata")), Some(""))), None, "isolated: nothing read or written");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn without_a_platform_folder_there_are_no_settings() {
    let root = root("nowhere");
    assert_eq!(settings_dir(&probe(&root, None, None)), None, "not the current directory");
    let _ = std::fs::remove_dir_all(&root);
}
