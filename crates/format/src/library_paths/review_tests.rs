use super::*;
use crate::{Snapshot, Version};
use figcraft_doc::LibraryLink;
use std::io::{Cursor, Read};
use std::sync::Arc;

fn valid() -> String {
    let mut doc = Document::new("Stored version");
    doc.libraries_mut().enabled.push(LibraryLink {
        id: "library-id".into(),
        name: "Library".into(),
        path: "../lib/Library.figcraft".into(),
        version: 1,
    });
    serde_json::to_string(&crate::to_json(&doc).unwrap()).unwrap()
}

fn round_trip(raw: String, unreadable: bool) {
    if unreadable {
        assert!(crate::from_json(&raw).is_err(), "the pre-save snapshot really is lazily unreadable");
    }
    let mut v = Version::new("v1", "Original", "", 1, Arc::new(Document::new("Unused")));
    v.snapshot = Snapshot::Stored { json: Arc::new(raw.as_bytes().to_vec()), images: BTreeMap::new() };
    let bytes = crate::save_bytes_with_history_paths(&Document::new("Current"), None, &[v], &[], &|p| {
        (p == "../lib/Library.figcraft").then(|| "../../lib/Library.figcraft".into())
    })
    .unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut stored = String::new();
    archive.by_name("versions/v1.json").unwrap().read_to_string(&mut stored).unwrap();
    assert_eq!(stored, raw, "duplicate fields must preserve the original stored bytes, not repair or collapse them");
    let loaded = crate::load_bytes_with_versions(&bytes).unwrap();
    assert_eq!(loaded.versions.len(), 1);
    if unreadable {
        assert!(loaded.versions[0].document().is_err());
    }
}

#[test]
fn issue_2122_review_duplicate_known_path_remains_lazily_unreadable() {
    let raw = valid().replace(r#""path":"../lib/Library.figcraft""#, r#""path":"earlier.figcraft","path":"../lib/Library.figcraft""#);
    round_trip(raw, true);
}

#[test]
fn issue_2122_review_escaped_duplicate_known_path_remains_lazily_unreadable() {
    let raw = valid().replace(r#""path":"../lib/Library.figcraft""#, r#""\u0070ath":"earlier.figcraft","path":"../lib/Library.figcraft""#);
    round_trip(raw, true);
}

#[test]
fn issue_2122_review_duplicate_libraries_field_remains_lazily_unreadable() {
    let raw = valid().replacen(r#""libraries":{"#, r#""libraries":{},"libraries":{"#, 1);
    round_trip(raw, true);
}

#[test]
fn issue_2122_review_duplicate_unknown_fields_keep_exact_raw_bytes_at_every_rewritten_object() {
    for raw in [
        valid().replacen('{', r#"{"future":1,"future":2,"#, 1),
        valid().replacen(r#""libraries":{"#, r#""libraries":{"future":1,"future":2,"#, 1),
        valid().replace(r#""path":"../lib/Library.figcraft""#, r#""future":1,"future":2,"path":"../lib/Library.figcraft""#),
    ] {
        round_trip(raw, false);
    }
}
