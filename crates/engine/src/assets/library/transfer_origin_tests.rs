#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use figcraft_doc::{Document, NodeId, NodeKind};
use serde_json::{Value, json};

use crate::{EngineError, Session};

struct Fixture {
    root: PathBuf,
    library: PathBuf,
    source: PathBuf,
    target: PathBuf,
    decoy: PathBuf,
    id: String,
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn run(s: &mut Session, command: &str, params: Value) -> Value {
    s.execute(command, &params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("figcraft-library-transfer-{}-{name}", std::process::id()));
        assert!(!root.exists(), "each test owns a fresh synthetic directory");
        let library = root.join("project/lib/Kit.figcraft");
        let source = root.join("project/a/Page.figcraft");
        let target = root.join("other/b/Target.figcraft");
        let decoy = root.join("other/lib/Kit.figcraft");
        for path in [&library, &source, &target, &decoy] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        }
        let mut published = Session::new();
        run(&mut published, "node.create", json!({"type":"component", "name":"Card", "width":40, "height":20}));
        let manifest = run(&mut published, "library.publish", json!({}));
        let id = manifest["id"].as_str().unwrap().to_owned();
        run(&mut published, "file.saveAs", json!({"path":text(&library)}));
        // Same library identity at the wrong target-relative path, but a different version/shape.
        // An ID check alone therefore cannot conceal incorrect path authority.
        std::fs::copy(&library, &decoy).unwrap();
        let mut wrong = Session::empty();
        run(&mut wrong, "file.open", json!({"path":text(&decoy)}));
        let main = wrong.doc().unwrap().node(wrong.page().unwrap()).unwrap().children[0];
        run(&mut wrong, "node.set", json!({"ids":[main], "props":{"width":400}}));
        let next = run(&mut wrong, "library.publish", json!({}));
        assert_eq!(next["id"], id);
        assert_eq!(next["version"], 2);
        run(&mut wrong, "file.save", json!({}));
        let mut consumer = Session::new();
        run(&mut consumer, "library.add", json!({"path":text(&library)}));
        run(&mut consumer, "library.place", json!({"library":id, "component":"Card"}));
        run(&mut consumer, "file.saveAs", json!({"path":text(&source)}));
        let (stored, issues) = figcraft_format::load_bytes(&std::fs::read(&source).unwrap()).unwrap();
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(stored.libraries.enabled[0].path, text(&Path::new("..").join("lib").join("Kit.figcraft")));
        let mut target_session = Session::new();
        run(&mut target_session, "file.saveAs", json!({"path":text(&target)}));
        Self { root, library, source, target, decoy, id }
    }

    fn target_session(&self) -> Session {
        let mut s = Session::empty();
        run(&mut s, "file.open", json!({"path":text(&self.target)}));
        s
    }

    fn source_bytes(&self) -> Vec<u8> {
        std::fs::read(&self.source).unwrap()
    }

    fn source_doc(&self) -> Document {
        figcraft_format::load_bytes(&self.source_bytes()).unwrap().0
    }

    fn open_data(&self, s: &mut Session, bytes: &[u8]) {
        // Even an absolute-looking name hint is not authority to resolve local references.
        run(s, "file.open", json!({"data":base64::engine::general_purpose::STANDARD.encode(bytes), "name":text(&self.source)}));
        assert!(s.library_origin().is_none());
    }

    fn assert_followed(&self, s: &mut Session) {
        let link = s.doc().unwrap().libraries.link(&self.id).unwrap();
        let resolved = if Path::new(&link.path).is_absolute() {
            PathBuf::from(&link.path)
        } else {
            Path::new(s.library_origin().unwrap()).parent().unwrap().join(&link.path)
        };
        assert_eq!(
            std::fs::canonicalize(resolved).unwrap(),
            std::fs::canonicalize(&self.library).unwrap(),
            "the source library, not the same-ID target decoy"
        );
        let listing = run(s, "library.list", json!({}));
        let row = listing["libraries"].as_array().unwrap().iter().find(|row| row["id"] == self.id).unwrap();
        assert!(row.get("error").is_none(), "{listing}");
        assert_eq!(row["version"], 1, "the decoy is version2: {listing}");
        let updates = run(s, "library.updates", json!({}));
        assert_eq!(updates["errors"], json!([]), "{updates}");
        assert_eq!(updates["count"], 0, "the decoy must not supply updates: {updates}");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn instance(s: &Session) -> NodeId {
    s.doc()
        .unwrap()
        .pages
        .iter()
        .flat_map(|page| s.doc().unwrap().node(*page).unwrap().children.iter().copied())
        .find(|node| matches!(s.doc().unwrap().node(*node).unwrap().kind, NodeKind::Instance { .. }))
        .unwrap()
}

fn copy_instance(s: &mut Session) {
    let selected = instance(s);
    run(s, "select.set", json!({"ids":[selected]}));
    run(s, "edit.copy", json!({}));
}

fn assert_remote_read_only(s: &mut Session) {
    let main = match s.doc().unwrap().node(instance(s)).unwrap().kind {
        NodeKind::Instance { component, .. } => component,
        _ => unreachable!(),
    };
    assert!(s.doc().unwrap().is_remote(main));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    assert!(matches!(s.execute("node.set", &json!({"ids":[main], "props":{"name":"Refused"}})), Err(EngineError::Disabled(..))));
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), history);
}

#[test]
fn transfer_path_import_keeps_source_library_across_undo_save_and_reopen() {
    let f = Fixture::new("path-import");
    let mut s = f.target_session();
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    run(&mut s, "file.import", json!({"path":text(&f.source)}));
    f.assert_followed(&mut s);
    assert_remote_read_only(&mut s);
    let after = s.doc().unwrap().clone();
    assert_eq!(s.undo_depth(), (history.0 + 1, 0));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc().unwrap(), &before);
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.doc().unwrap(), &after);
    f.assert_followed(&mut s);
    run(&mut s, "file.save", json!({}));
    let mut reopened = Session::empty();
    run(&mut reopened, "file.open", json!({"path":text(&f.target)}));
    f.assert_followed(&mut reopened);
    assert!(reopened.doc().unwrap().validate().is_empty());
}

#[test]
fn transfer_foreign_clipboard_keeps_source_library_and_existing_clipboard_path() {
    let f = Fixture::new("foreign-paste");
    let mut s = Session::empty();
    run(&mut s, "file.open", json!({"path":text(&f.source)}));
    copy_instance(&mut s);
    assert_eq!(s.clipboard.source_path.as_deref(), Some(text(&f.source).as_str()));
    run(&mut s, "file.open", json!({"path":text(&f.target)}));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    run(&mut s, "edit.paste", json!({}));
    f.assert_followed(&mut s);
    assert_remote_read_only(&mut s);
    assert_eq!(s.clipboard.source_path.as_deref(), Some(text(&f.source).as_str()));
    assert_eq!(s.undo_depth(), (history.0 + 1, 0));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc().unwrap(), &before);
    run(&mut s, "edit.redo", json!({}));
    f.assert_followed(&mut s);
}

#[test]
fn transfer_copy_after_save_as_uses_retained_origin_instead_of_current_source_path() {
    let f = Fixture::new("save-as-copy");
    let relocated = f.root.join("moved/b/Copy.figcraft");
    let second_decoy = f.root.join("moved/lib/Kit.figcraft");
    std::fs::create_dir_all(relocated.parent().unwrap()).unwrap();
    std::fs::create_dir_all(second_decoy.parent().unwrap()).unwrap();
    std::fs::copy(&f.decoy, &second_decoy).unwrap();
    let mut s = Session::empty();
    run(&mut s, "file.open", json!({"path":text(&f.source)}));
    let retained = s.library_origin().unwrap().to_owned();
    run(&mut s, "file.saveAs", json!({"path":text(&relocated)}));
    assert_eq!(s.library_origin(), Some(retained.as_str()));
    copy_instance(&mut s);
    assert_eq!(s.clipboard.source_path.as_deref(), Some(text(&relocated).as_str()));
    run(&mut s, "file.open", json!({"path":text(&f.target)}));
    run(&mut s, "edit.paste", json!({}));
    f.assert_followed(&mut s);
    assert_eq!(s.clipboard.source_path.as_deref(), Some(text(&relocated).as_str()));
}

#[test]
fn transfer_unknown_origin_data_import_refuses_without_target_or_name_hint_fallback() {
    let f = Fixture::new("unknown-data-import");
    let mut s = f.target_session();
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let error = s
        .execute("file.import", &json!({"data":base64::engine::general_purpose::STANDARD.encode(f.source_bytes()), "name":text(&f.source)}))
        .unwrap_err();
    assert!(error.to_string().contains("origin"), "{error}");
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), history);
}

#[test]
fn transfer_unknown_origin_foreign_clipboard_refuses_but_same_document_paste_keeps_cache() {
    let f = Fixture::new("unknown-clipboard");
    let mut s = Session::empty();
    f.open_data(&mut s, &f.source_bytes());
    copy_instance(&mut s);
    assert!(s.clipboard.source_path.is_none());
    let link = s.doc().unwrap().libraries.enabled[0].clone();
    run(&mut s, "edit.paste", json!({}));
    assert_eq!(s.doc().unwrap().libraries.enabled[0], link, "a same-document copy needs no origin conversion");
    run(&mut s, "file.open", json!({"path":text(&f.target)}));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let error = s.execute("edit.paste", &json!({})).unwrap_err();
    assert!(error.to_string().contains("origin"), "{error}");
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), history);
    assert!(s.clipboard.source_path.is_none());
}

#[test]
fn transfer_existing_destination_link_keeps_authority_even_when_source_origin_is_unknown() {
    for paste in [false, true] {
        let f = Fixture::new(if paste { "authority-paste" } else { "authority-import" });
        let mut s = f.target_session();
        run(&mut s, "library.add", json!({"path":text(&f.library)}));
        run(&mut s, "library.place", json!({"library":f.id, "component":"Card"}));
        let link = s.doc().unwrap().libraries.link(&f.id).unwrap().clone();
        if paste {
            f.open_data(&mut s, &f.source_bytes());
            copy_instance(&mut s);
            run(&mut s, "file.open", json!({"path":text(&f.target)}));
            run(&mut s, "edit.paste", json!({}));
        } else {
            run(&mut s, "file.import", json!({"data":base64::engine::general_purpose::STANDARD.encode(f.source_bytes()), "name":"Source.figcraft"}));
        }
        assert_eq!(s.doc().unwrap().libraries.link(&f.id), Some(&link));
        f.assert_followed(&mut s);
        assert_remote_read_only(&mut s);
    }
}

#[test]
fn transfer_explicit_absolute_links_work_without_an_origin() {
    let f = Fixture::new("absolute-data-import");
    let mut doc = f.source_doc();
    doc.libraries_mut().enabled[0].path = text(&f.library);
    let bytes = figcraft_format::save_bytes(&doc, None).unwrap();
    let mut s = f.target_session();
    run(&mut s, "file.import", json!({"data":base64::engine::general_purpose::STANDARD.encode(bytes), "name":"No-origin.figcraft"}));
    f.assert_followed(&mut s);
}

#[test]
fn transfer_preserves_nonlocal_refusal_and_cached_read_root_authority() {
    let f = Fixture::new("access-gates");
    let mut doc = f.source_doc();
    let blocked = r"\\outside.invalid\share\Kit.figcraft";
    doc.libraries_mut().enabled[0].path = blocked.into();
    let mut s = f.target_session();
    let bytes = figcraft_format::save_bytes(&doc, None).unwrap();
    run(&mut s, "file.import", json!({"data":base64::engine::general_purpose::STANDARD.encode(bytes), "name":"Blocked.figcraft"}));
    assert_eq!(s.doc().unwrap().libraries.link(&f.id).unwrap().path, blocked);
    let listing = run(&mut s, "library.list", json!({}));
    assert!(listing["libraries"][0]["error"].as_str().unwrap().contains("only from this computer"), "{listing}");
    let mut local = f.target_session();
    run(&mut local, "file.import", json!({"path":text(&f.source)}));
    f.assert_followed(&mut local); // Warm cache first; authority must still be checked afterward.
    let roots = Arc::new(crate::file_access::Roots::new(&[text(f.target.parent().unwrap())], &[]).unwrap());
    crate::file_access::confined(Some(&roots), || {
        let listing = run(&mut local, "library.list", json!({}));
        assert!(listing["libraries"][0].get("error").is_some(), "{listing}");
        assert_eq!(run(&mut local, "library.updates", json!({}))["errors"].as_array().unwrap().len(), 1);
    });
    f.assert_followed(&mut local);
}
