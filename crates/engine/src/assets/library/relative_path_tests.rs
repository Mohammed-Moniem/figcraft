#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use figcraft_doc::{Document, LibraryLink};
use serde_json::{Value, json};

use crate::Session;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("figcraft-relative-library-{}-{name}", std::process::id()));
        assert!(!path.exists(), "a test owns a new directory");
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn directory(&self, relative: &str) -> PathBuf {
        let path = self.0.join(relative);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn execute(session: &mut Session, command: &str, params: Value) -> Value {
    session.execute(command, &params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn published(path: &Path, name: &str) -> LibraryLink {
    let mut session = Session::new();
    execute(&mut session, "node.create", json!({"type": "component", "name": "Sidebar", "width": 200, "height": 300}));
    let manifest = execute(&mut session, "library.publish", json!({"description": name}));
    execute(&mut session, "file.saveAs", json!({"path": text(path)}));
    LibraryLink { id: manifest["id"].as_str().unwrap().to_string(), name: "Untitled".into(), path: text(path), version: 1 }
}

fn write_linked(path: &Path, mut link: LibraryLink, relative: bool) {
    let mut doc = Document::new("Portable library consumer");
    if relative {
        link.path = text(&Path::new("..").join("lib").join("Library.figcraft"));
    }
    doc.libraries_mut().enabled.push(link);
    let bytes = figcraft_format::save_bytes(&doc, None).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn raw_link(path: &Path) -> LibraryLink {
    let (doc, issues) = figcraft_format::load_bytes(&std::fs::read(path).unwrap()).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(doc.libraries.enabled.len(), 1);
    doc.libraries.enabled[0].clone()
}

fn readable(session: &mut Session, id: &str) {
    let listing = execute(session, "library.list", json!({}));
    let row = listing["libraries"].as_array().unwrap().iter().find(|row| row["id"] == id).unwrap();
    assert!(row.get("error").is_none(), "{listing}");
    assert_eq!(row["version"], 1);
    let updates = execute(session, "library.updates", json!({}));
    assert_eq!(updates["errors"], json!([]), "{updates}");
    assert_eq!(updates["count"], 0);
}

#[test]
fn issue_2122_first_native_save_keeps_the_library_after_the_project_moves() {
    let scratch = Scratch::new("first-save-move");
    let lib = scratch.directory("a/lib").join("Library.figcraft");
    let page = scratch.directory("a/pages").join("Page.figcraft");
    let link = published(&lib, "First save");
    let mut session = Session::new();
    execute(&mut session, "library.add", json!({"path": text(&lib)}));
    execute(&mut session, "library.place", json!({"library": link.id, "component": "Sidebar"}));
    execute(&mut session, "file.saveAs", json!({"path": text(&page)}));
    let stored = raw_link(&page);
    assert_eq!(stored.id, link.id);
    assert!(!Path::new(&stored.path).is_absolute(), "a native save must store a portable link: {}", stored.path);
    assert_eq!(std::fs::canonicalize(page.parent().unwrap().join(&stored.path)).unwrap(), std::fs::canonicalize(&lib).unwrap());
    drop(session);
    std::fs::rename(scratch.0.join("a"), scratch.0.join("b")).unwrap();
    let mut reopened = Session::empty();
    execute(&mut reopened, "file.open", json!({"path": text(&scratch.0.join("b/pages/Page.figcraft"))}));
    readable(&mut reopened, &link.id);
    assert!(reopened.doc().unwrap().validate().is_empty());
}

#[test]
fn issue_2122_identical_relative_links_in_two_open_documents_use_their_own_folders() {
    let scratch = Scratch::new("two-documents");
    let mut fixtures = Vec::new();
    for (folder, name) in [("a", "First library"), ("b", "Second library")] {
        let library = scratch.directory(&format!("{folder}/lib")).join("Library.figcraft");
        let page = scratch.directory(&format!("{folder}/pages")).join("Page.figcraft");
        let link = published(&library, name);
        write_linked(&page, link.clone(), true);
        fixtures.push((page, link));
    }
    assert_ne!(fixtures[0].1.id, fixtures[1].1.id);
    let mut session = Session::empty();
    for index in [0, 1, 0, 1] {
        let (page, link) = &fixtures[index];
        execute(&mut session, "file.open", json!({"path": text(page)}));
        readable(&mut session, &link.id);
    }
}

#[test]
fn issue_2122_save_as_rebases_a_relative_link_without_changing_its_library() {
    let scratch = Scratch::new("save-as");
    let library = scratch.directory("a/lib").join("Library.figcraft");
    let page = scratch.directory("a/pages").join("Page.figcraft");
    let destination = scratch.directory("archive/deep").join("Copy.figcraft");
    let link = published(&library, "Save As");
    write_linked(&page, link.clone(), true);
    let mut session = Session::empty();
    execute(&mut session, "file.open", json!({"path": text(&page)}));
    execute(&mut session, "file.saveAs", json!({"path": text(&destination)}));
    let stored = raw_link(&destination);
    assert_eq!(stored.id, link.id);
    assert!(!Path::new(&stored.path).is_absolute());
    assert_eq!(std::fs::canonicalize(destination.parent().unwrap().join(&stored.path)).unwrap(), std::fs::canonicalize(&library).unwrap());
    let mut reopened = Session::empty();
    execute(&mut reopened, "file.open", json!({"path": text(&destination)}));
    readable(&mut reopened, &link.id);
}

#[test]
fn issue_2122_relative_resolution_preserves_read_authority_even_after_a_cached_load() {
    let scratch = Scratch::new("read-authority");
    let library = scratch.directory("a/lib").join("Library.figcraft");
    let pages = scratch.directory("a/pages");
    let page = pages.join("Page.figcraft");
    let link = published(&library, "Confined library");
    write_linked(&page, link.clone(), true);
    let mut session = Session::empty();
    execute(&mut session, "file.open", json!({"path": text(&page)}));
    readable(&mut session, &link.id);
    let roots = Arc::new(crate::file_access::Roots::new(&[text(&pages)], &[]).unwrap());
    crate::file_access::confined(Some(&roots), || {
        let listing = execute(&mut session, "library.list", json!({}));
        assert!(listing["libraries"][0].get("error").is_some(), "the previously cached sibling library is outside the read root: {listing}");
        let updates = execute(&mut session, "library.updates", json!({}));
        assert_eq!(updates["errors"].as_array().unwrap().len(), 1);
    });
    readable(&mut session, &link.id);
}

#[test]
fn issue_2122_existing_absolute_library_links_remain_readable() {
    let scratch = Scratch::new("absolute-compatibility");
    let library = scratch.directory("a/lib").join("Library.figcraft");
    let page = scratch.directory("a/pages").join("Page.figcraft");
    let link = published(&library, "Legacy absolute");
    write_linked(&page, link.clone(), false);
    let mut session = Session::empty();
    execute(&mut session, "file.open", json!({"path": text(&page)}));
    readable(&mut session, &link.id);
}

// Additional compatibility controls, first executed only after the original and review RED runs.

#[test]
fn issue_2122_save_as_keeps_the_library_in_the_same_session_and_after_public_undo_redo() {
    let scratch = Scratch::new("save-as-history");
    let library = scratch.directory("a/lib").join("Library.figcraft");
    let page = scratch.directory("a/pages").join("Page.figcraft");
    let destination = scratch.directory("a/archive/deep").join("Copy.figcraft");
    let link = published(&library, "In-memory history");
    write_linked(&page, link.clone(), true);
    let mut session = Session::empty();
    execute(&mut session, "file.open", json!({"path": text(&page)}));
    execute(&mut session, "shape.rectangle", json!({"name":"Local edit","width":10,"height":10}));
    execute(&mut session, "file.saveAs", json!({"path": text(&destination)}));
    readable(&mut session, &link.id);
    execute(&mut session, "edit.undo", json!({}));
    readable(&mut session, &link.id);
    execute(&mut session, "edit.redo", json!({}));
    readable(&mut session, &link.id);
    assert_eq!(raw_link(&destination).id, link.id);
}

#[test]
fn issue_2122_lazy_native_versions_rebase_when_save_as_changes_folders() {
    let scratch = Scratch::new("lazy-versions");
    let library = scratch.directory("a/lib").join("Library.figcraft");
    let page = scratch.directory("a/pages").join("Page.figcraft");
    let destination = scratch.directory("a/archive/deep").join("Copy.figcraft");
    let link = published(&library, "Lazy version");
    write_linked(&page, link.clone(), true);
    let mut first = Session::empty();
    execute(&mut first, "file.open", json!({"path": text(&page)}));
    let version = execute(&mut first, "file.saveVersion", json!({"name":"Before relocation"}));
    execute(&mut first, "file.save", json!({}));
    drop(first);
    // Reload first: the archived history is a lazy Stored snapshot rather than a live Doc.
    let mut second = Session::empty();
    execute(&mut second, "file.open", json!({"path": text(&page)}));
    execute(&mut second, "file.saveAs", json!({"path": text(&destination)}));
    drop(second);
    std::fs::rename(scratch.0.join("a"), scratch.0.join("b")).unwrap();
    let mut moved = Session::empty();
    execute(&mut moved, "file.open", json!({"path":text(&scratch.0.join("b/archive/deep/Copy.figcraft"))}));
    readable(&mut moved, &link.id);
    execute(&mut moved, "version.restore", json!({"id":version["id"]}));
    readable(&mut moved, &link.id);
    assert!(moved.doc().unwrap().validate().is_empty());
}

#[test]
fn issue_2122_branch_heads_and_branch_versions_retain_their_library_after_a_project_move() {
    let scratch = Scratch::new("branches");
    let library = scratch.directory("a/lib").join("Library.figcraft");
    let page = scratch.directory("a/pages").join("Page.figcraft");
    let destination = scratch.directory("a/archive/deep").join("Copy.figcraft");
    let link = published(&library, "Branch library");
    write_linked(&page, link.clone(), true);
    let mut first = Session::empty();
    execute(&mut first, "file.open", json!({"path": text(&page)}));
    let branch = execute(&mut first, "branch.create", json!({"name":"Portable branch"}));
    let version = execute(&mut first, "file.saveVersion", json!({"name":"Branch before relocation"}));
    execute(&mut first, "file.saveAs", json!({"path": text(&destination)}));
    readable(&mut first, &link.id);
    drop(first);
    std::fs::rename(scratch.0.join("a"), scratch.0.join("b")).unwrap();
    let mut moved = Session::empty();
    execute(&mut moved, "file.open", json!({"path":text(&scratch.0.join("b/archive/deep/Copy.figcraft"))}));
    execute(&mut moved, "branch.open", json!({"id":branch["id"]}));
    readable(&mut moved, &link.id);
    execute(&mut moved, "version.restore", json!({"id":version["id"]}));
    readable(&mut moved, &link.id);
}

#[test]
fn issue_2122_file_and_version_duplicates_inherit_origin_before_their_first_save() {
    let scratch = Scratch::new("duplicates");
    let library = scratch.directory("a/lib").join("Library.figcraft");
    let page = scratch.directory("a/pages").join("Page.figcraft");
    let file_copy = scratch.directory("a/file-copies/deep").join("Copy.figcraft");
    let version_copy = scratch.directory("a/version-copies/deep").join("Copy.figcraft");
    let link = published(&library, "Duplicated library");
    write_linked(&page, link.clone(), true);
    let mut first = Session::empty();
    execute(&mut first, "file.open", json!({"path":text(&page)}));
    let version = execute(&mut first, "file.saveVersion", json!({"name":"Original version"}));
    execute(&mut first, "file.duplicate", json!({}));
    readable(&mut first, &link.id);
    execute(&mut first, "file.saveAs", json!({"path":text(&file_copy)}));
    execute(&mut first, "doc.switch", json!({"index":0}));
    execute(&mut first, "version.duplicate", json!({"id":version["id"]}));
    readable(&mut first, &link.id);
    execute(&mut first, "file.saveAs", json!({"path":text(&version_copy)}));
    drop(first);
    std::fs::rename(scratch.0.join("a"), scratch.0.join("b")).unwrap();
    for path in ["b/file-copies/deep/Copy.figcraft", "b/version-copies/deep/Copy.figcraft"] {
        let mut moved = Session::empty();
        execute(&mut moved, "file.open", json!({"path":text(&scratch.0.join(path))}));
        readable(&mut moved, &link.id);
    }
}

#[test]
fn issue_2122_data_without_an_origin_refuses_cwd_and_uses_its_first_explicit_save_folder() {
    let scratch = Scratch::new("data-origin");
    let library = scratch.directory("a/lib").join("Library.figcraft");
    let page = scratch.directory("a/pages").join("Page.figcraft");
    let destination = scratch.directory("a/archive/deep").join("Copy.figcraft");
    let link = published(&library, "Data origin");
    write_linked(&page, link.clone(), true);
    let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, std::fs::read(&page).unwrap());
    let mut session = Session::empty();
    execute(&mut session, "file.open", json!({"name":"From data.figcraft","data":data}));
    let listing = execute(&mut session, "library.list", json!({}));
    assert!(listing["libraries"][0].get("error").is_some(), "no origin must not read relative to process CWD: {listing}");
    execute(&mut session, "file.saveAs", json!({"path":text(&page)}));
    readable(&mut session, &link.id);
    execute(&mut session, "file.saveAs", json!({"path":text(&destination)}));
    readable(&mut session, &link.id);
    let mut reopened = Session::empty();
    execute(&mut reopened, "file.open", json!({"path":text(&destination)}));
    readable(&mut reopened, &link.id);
}

#[test]
fn issue_2122_a_saved_origin_never_grants_access_to_a_network_or_device_library() {
    let scratch = Scratch::new("origin-local-only");
    let page = scratch.directory("a/pages").join("Page.figcraft");
    for raw in [
        r"\\attacker.example\share\Library.figcraft",
        "//attacker.example/share/Library.figcraft",
        "file://attacker.example/share/Library.figcraft",
        r"\\?\UNC\attacker.example\share\Library.figcraft",
        r"\\.\pipe\Library",
        "https://attacker.example/Library.figcraft",
    ] {
        let mut doc = Document::new("Untrusted reference");
        doc.libraries_mut().enabled.push(LibraryLink { id: "untrusted".into(), name: "Untrusted".into(), path: raw.into(), version: 1 });
        std::fs::write(&page, figcraft_format::save_bytes(&doc, None).unwrap()).unwrap();
        let mut session = Session::empty();
        execute(&mut session, "file.open", json!({"path":text(&page)}));
        let listing = execute(&mut session, "library.list", json!({}));
        assert!(listing["libraries"][0]["error"].as_str().unwrap().contains("only from this computer"), "{raw}: {listing}");
    }
}
