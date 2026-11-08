//! Source priority composes explicit loads with installed and built-in faces.
//! All installed stand-ins enter through the real browser-byte API; no OS font is installed.
use super::*;

const FAMILY: &str = "Composed Source Sans";
const KEY: &str = "composed source sans";

fn fixture() -> Option<(FontDb, Vec<u8>)> {
    let Some(font) = CRAFT_FONTS.first() else {
        eprintln!("skipping source-priority stand-in: no optional craft-fonts build input");
        return None;
    };
    Some((FontDb::default(), font.bytes.to_vec()))
}

fn face(bytes: &[u8], style: &str, weight: u16) -> Face {
    let mut face = Face::from_bytes(FontBytes::Shared(Arc::new(bytes.to_vec())), 0, Some(FAMILY), Some(style)).unwrap();
    face.axes.clear();
    face.instances.clear();
    face.weight = weight;
    face.italic = false;
    face
}

fn named_bold(face: &mut Face) {
    face.axes = vec![Axis { tag: "wght".into(), name: "Weight".into(), min: 100.0, default: 400.0, max: 900.0, hidden: false }];
    face.instances = vec![NamedInstance { name: "Bold".into(), coords: vec![700.0] }];
}

fn local(id: usize, style: &str) -> LocalFont {
    LocalFont { id, family: FAMILY.into(), style: style.into(), postscript: String::new() }
}

fn installed(db: &mut FontDb, bytes: &[u8], id: usize, style: &str, weight: u16, named: bool) -> u32 {
    db.merge_rescan(BTreeMap::from([(
        KEY.into(),
        (FAMILY.into(), vec![Pending { path: std::path::PathBuf::new(), index: 0, style: style.into(), postscript: None, local: Some(id) }]),
    )]));
    assert_eq!(db.load_local(id, bytes.to_vec()), Some(FAMILY.into()));
    let result = Arc::get_mut(db.faces.last_mut().unwrap()).unwrap();
    result.axes.clear();
    result.instances.clear();
    result.weight = weight;
    result.italic = false;
    if named {
        named_bold(result);
    }
    result.id
}

#[test]
fn loaded_named_instance_precedes_installed_literal_and_builtin_literal() {
    let Some((mut db, bytes)) = fixture() else { return };
    db.add(face(&bytes, "Bold", 700));
    let mut variable = face(&bytes, "Regular", 400);
    named_bold(&mut variable);
    let loaded = db.add_loaded(variable).id;
    let local = installed(&mut db, &bytes, 43001, "Bold", 700, false);
    assert_ne!(local, loaded);
    assert_eq!(db.find(FAMILY, "Bold").unwrap().id, loaded);
    assert_eq!(db.find_one(FAMILY, "Bold").unwrap().id, loaded);
}

#[test]
fn loaded_literal_precedes_installed_named_instance() {
    let Some((mut db, bytes)) = fixture() else { return };
    let local = installed(&mut db, &bytes, 43002, "Regular", 400, true);
    let loaded = db.add_loaded(face(&bytes, "Bold", 700)).id;
    assert_ne!(local, loaded);
    assert_eq!(db.find(FAMILY, "Bold").unwrap().id, loaded);
}

#[test]
fn latest_explicit_load_wins_across_later_installed_arrivals() {
    let Some((mut db, bytes)) = fixture() else { return };
    let first = db.add_loaded(face(&bytes, "Regular", 400)).id;
    installed(&mut db, &bytes, 43003, "Regular", 400, false);
    let latest = db.add_loaded(face(&bytes, "Regular", 400)).id;
    installed(&mut db, &bytes, 43004, "Regular", 400, false);
    db.add(face(&bytes, "Regular", 400));
    assert_ne!(first, latest);
    assert_eq!(db.find(FAMILY, "Regular").unwrap().id, latest);
}

#[test]
fn installed_literal_precedes_builtin_literal_without_explicit_load() {
    let Some((mut db, bytes)) = fixture() else { return };
    let builtin = db.add(face(&bytes, "Regular", 400)).id;
    let local = installed(&mut db, &bytes, 43005, "Regular", 400, false);
    assert_ne!(builtin, local);
    assert_eq!(db.find(FAMILY, "Regular").unwrap().id, local);
}

#[test]
fn installed_named_instance_precedes_builtin_literal() {
    let Some((mut db, bytes)) = fixture() else { return };
    db.add(face(&bytes, "Bold", 700));
    let local = installed(&mut db, &bytes, 43006, "Regular", 400, true);
    assert_eq!(db.find(FAMILY, "Bold").unwrap().id, local);
}

#[test]
fn pending_installed_match_waits_behind_explicit_load_but_can_fill_another_style() {
    let Some((mut db, bytes)) = fixture() else { return };
    let loaded = db.add_loaded(face(&bytes, "Regular", 400)).id;
    db.add_local(vec![local(43007, "Regular"), local(43008, "Bold")]);
    assert_eq!(db.find(FAMILY, "Regular").unwrap().id, loaded);
    assert!(db.wanted.is_empty(), "an equal pending installed face must not outrank an explicit load");
    assert!(db.pending.get(KEY).unwrap().1.iter().any(|entry| entry.local == Some(43007)), "the pending face remains available");
    db.find(FAMILY, "Bold");
    assert_eq!(db.wanted, [43008], "a closer pending style still fills the gap");
    assert_eq!(db.load_local(43008, bytes), Some(FAMILY.into()));
    assert_eq!(db.find(FAMILY, "Bold").unwrap().style, "Bold");
    assert_eq!(db.find(FAMILY, "Regular").unwrap().id, loaded);
}

#[test]
fn pending_literal_does_not_displace_equal_loaded_named_instance() {
    let Some((mut db, bytes)) = fixture() else { return };
    let mut variable = face(&bytes, "Regular", 400);
    named_bold(&mut variable);
    let loaded = db.add_loaded(variable).id;
    db.add_local(vec![local(43009, "Bold")]);
    assert_eq!(db.find(FAMILY, "Bold").unwrap().id, loaded);
    assert!(db.wanted.is_empty());
    assert!(db.pending.get(KEY).unwrap().1.iter().any(|entry| entry.local == Some(43009)));
}

#[test]
fn nearer_style_quality_precedes_explicit_source_priority() {
    let Some((mut db, bytes)) = fixture() else { return };
    db.add_loaded(face(&bytes, "Regular", 400));
    let local = installed(&mut db, &bytes, 43010, "Book", 500, false);
    assert_eq!(db.find(FAMILY, "Medium").unwrap().id, local, "actual face weight is closer even though its source ranks lower");
}

#[test]
fn equal_nearest_style_cost_uses_loaded_then_installed_then_builtin() {
    let Some((mut db, bytes)) = fixture() else { return };
    db.add(face(&bytes, "Regular", 400));
    let local = installed(&mut db, &bytes, 43011, "Book", 400, false);
    assert_eq!(db.find(FAMILY, "Medium").unwrap().id, local);
    let loaded = db.add_loaded(face(&bytes, "Normal", 400)).id;
    assert_eq!(db.find(FAMILY, "Medium").unwrap().id, loaded);
    let latest = db.add_loaded(face(&bytes, "Regular", 400)).id;
    assert_eq!(db.find(FAMILY, "Medium").unwrap().id, latest);
}

#[test]
fn exact_builtin_style_precedes_approximate_explicit_load() {
    let Some((mut db, bytes)) = fixture() else { return };
    let exact = db.add(face(&bytes, "Bold", 700)).id;
    db.add_loaded(face(&bytes, "Regular", 400));
    assert_eq!(db.find(FAMILY, "Bold").unwrap().id, exact);
}

#[test]
fn unreadable_installed_response_keeps_explicit_load_usable() {
    let Some((mut db, bytes)) = fixture() else { return };
    let loaded = db.add_loaded(face(&bytes, "Regular", 400)).id;
    db.merge_rescan(BTreeMap::from([(
        KEY.into(),
        (FAMILY.into(), vec![Pending { path: std::path::PathBuf::new(), index: 0, style: "Regular".into(), postscript: None, local: Some(43012) }]),
    )]));
    assert_eq!(db.load_local(43012, b"not a font".to_vec()), None);
    assert_eq!(db.find(FAMILY, "Regular").unwrap().id, loaded);
    assert!(!db.pending.contains_key(KEY));
}
