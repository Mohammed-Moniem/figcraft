//! #2092: conflicting installed faces remain available and win matching built-in faces.

use super::*;

fn stand_in() -> Option<(FontDb, u32, Vec<u8>)> {
    let Some(font) = CRAFT_FONTS.first() else {
        eprintln!("skipping installed-font stand-in: no optional craft-fonts build input");
        return None;
    };
    let bytes = font.bytes.to_vec();
    let mut regular = Face::from_bytes(FontBytes::Static(font.bytes), 0, Some("Conflict Test Sans"), Some("Regular")).unwrap();
    regular.axes.clear();
    regular.instances.clear();
    regular.weight = 400;
    let mut db = FontDb::default();
    let id = db.add(regular).id;
    let mut bold = Face::from_bytes(FontBytes::Static(font.bytes), 0, Some("Conflict Test Sans"), Some("Bold")).unwrap();
    bold.axes.clear();
    bold.instances.clear();
    bold.weight = 700;
    db.add(bold);
    Some((db, id, bytes))
}

fn browser_font(id: usize, style: &str) -> LocalFont {
    LocalFont { id, family: "Conflict Test Sans".into(), style: style.into(), postscript: String::new() }
}

fn browser_pending(id: usize, style: &str) -> BTreeMap<String, (String, Vec<Pending>)> {
    BTreeMap::from([(
        "conflict test sans".into(),
        (
            "Conflict Test Sans".into(),
            vec![Pending { path: std::path::PathBuf::new(), index: 0, style: style.into(), postscript: None, local: Some(id) }],
        ),
    )])
}

struct TempFont(std::path::PathBuf);
impl TempFont {
    fn new(bytes: &[u8], case: &str, id: u32) -> Self {
        let dir = std::env::temp_dir().join(format!("figcraft-font-priority-{case}-{}-{id}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let fixture = Self(dir);
        std::fs::write(fixture.0.join("stand-in.ttf"), bytes).unwrap();
        fixture
    }
    fn found(&self, style: &str) -> BTreeMap<String, (String, Vec<Pending>)> {
        BTreeMap::from([(
            "conflict test sans".into(),
            (
                "Conflict Test Sans".into(),
                vec![Pending { path: self.0.join("stand-in.ttf"), index: 0, style: style.into(), postscript: None, local: None }],
            ),
        )])
    }
}
impl Drop for TempFont {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn installed_priority_initial_browser_scan_requests_a_conflicting_style_once() {
    let Some((mut db, original, bytes)) = stand_in() else { return };
    assert!(db.add_local(vec![browser_font(42001, "Regular")]).is_empty());
    assert_eq!(db.find("Conflict Test Sans", "Regular").unwrap().id, original, "built-in remains while bytes are pending");
    db.find("Conflict Test Sans", "Regular");
    assert_eq!(db.wanted, [42001], "a matching built-in must not hide the installed face");
    assert_eq!(db.load_local(42001, bytes), Some("Conflict Test Sans".into()));
    let installed = db.find("Conflict Test Sans", "Regular").unwrap().id;
    assert_ne!(installed, original);
    assert_eq!(db.find("Conflict Test Sans", "Regular").unwrap().id, installed);
    assert_eq!(db.wanted, [42001]);
}

#[test]
fn installed_priority_browser_response_displaces_matching_builtin() {
    let Some((mut db, original, bytes)) = stand_in() else { return };
    db.merge_rescan(browser_pending(42002, "Regular"));
    assert_eq!(db.load_local(42002, bytes), Some("Conflict Test Sans".into()));
    assert_ne!(db.find("Conflict Test Sans", "Regular").unwrap().id, original);
    assert_eq!(db.find("Conflict Test Sans", "Bold").unwrap().style, "Bold", "an absent installed style retains the built-in");
}

#[test]
fn installed_priority_initial_native_scan_reads_a_conflicting_style() {
    let Some((mut db, original, bytes)) = stand_in() else { return };
    let fixture = TempFont::new(&bytes, "scan", original);
    db.merge_scan(fixture.found("Regular"));
    assert_ne!(db.find("Conflict Test Sans", "Regular").unwrap().id, original);
    assert!(db.loaded_files.contains(&(fixture.0.join("stand-in.ttf"), 0)));
}

#[test]
fn installed_priority_native_rescan_wins_matching_builtin() {
    let Some((mut db, original, bytes)) = stand_in() else { return };
    let fixture = TempFont::new(&bytes, "rescan", original);
    db.merge_rescan(fixture.found("Regular"));
    assert_ne!(db.find("Conflict Test Sans", "Regular").unwrap().id, original);
}

#[test]
fn installed_priority_find_one_reads_the_conflicting_installed_style() {
    let Some((mut db, original, bytes)) = stand_in() else { return };
    let fixture = TempFont::new(&bytes, "one", original);
    db.merge_rescan(fixture.found("Regular"));
    assert_ne!(db.find_one("Conflict Test Sans", "Regular").unwrap().id, original);
}

#[test]
fn installed_priority_unreadable_installed_bytes_keep_builtin() {
    let Some((mut db, original, _)) = stand_in() else { return };
    db.merge_rescan(browser_pending(42003, "Regular"));
    assert_eq!(db.load_local(42003, b"not a font".to_vec()), None);
    assert_eq!(db.find("Conflict Test Sans", "Regular").unwrap().id, original);
    assert!(!db.pending.contains_key("conflict test sans"));
}

#[test]
fn installed_priority_a_different_installed_style_does_not_replace_exact_builtin() {
    let Some((mut db, original, bytes)) = stand_in() else { return };
    let fixture = TempFont::new(&bytes, "other", original);
    db.merge_rescan(fixture.found("Italic"));
    assert_eq!(db.find("Conflict Test Sans", "Regular").unwrap().id, original);
    assert_eq!(db.find("Conflict Test Sans", "Bold").unwrap().style, "Bold");
}

#[test]
fn installed_priority_named_instance_matches_before_builtin_exact_style() {
    let Some((mut db, _, bytes)) = stand_in() else { return };
    let mut installed = Face::from_bytes(FontBytes::Shared(Arc::new(bytes)), 0, Some("Conflict Test Sans"), Some("Regular")).unwrap();
    installed.installed = true;
    installed.axes = vec![Axis { tag: "wght".into(), name: "Weight".into(), min: 100.0, default: 400.0, max: 900.0, hidden: false }];
    installed.instances = vec![NamedInstance { name: "Bold".into(), coords: vec![700.0] }];
    let id = db.add(installed).id;
    assert_eq!(db.find("Conflict Test Sans", "Bold").unwrap().id, id, "a named instance is a matching installed style");
}

#[test]
fn installed_priority_reoffering_browser_handle_keeps_one_face() {
    let Some((mut db, _, bytes)) = stand_in() else { return };
    db.add_local(vec![browser_font(42004, "Regular")]);
    db.find("Conflict Test Sans", "Regular");
    db.load_local(42004, bytes);
    let before = db.face_count();
    let chosen = db.find("Conflict Test Sans", "Regular").unwrap().id;
    db.add_local(vec![browser_font(42004, "Regular")]);
    assert_eq!(db.find("Conflict Test Sans", "Regular").unwrap().id, chosen);
    assert_eq!(db.face_count(), before);
    assert!(db.pending.get("conflict test sans").is_none_or(|(_, v)| v.is_empty()));
    assert_eq!(db.wanted, [42004]);
}
