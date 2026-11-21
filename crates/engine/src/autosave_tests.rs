//! Autosave checkpoints: only for unsaved changes since the last version, capped (oldest
//! autosaves go first, user versions stay), kept in the file with their flag.

use serde_json::{Value, json};

use crate::Session;

fn rect(s: &mut Session, x: f64) {
    s.execute("shape.rectangle", &json!({"x": x, "y": 0, "width": 10, "height": 10})).unwrap();
}

fn autosave(s: &mut Session) -> bool {
    s.execute("version.autosave", &json!({})).unwrap()["saved"].as_bool().unwrap()
}

fn list(s: &mut Session) -> Vec<Value> {
    s.execute("version.list", &json!({})).unwrap().as_array().unwrap().clone()
}

#[test]
fn autosaves_follow_unsaved_changes_only() {
    let mut s = Session::new();
    assert!(!autosave(&mut s), "nothing changed");
    rect(&mut s, 0.0);
    assert!(autosave(&mut s));
    let l = list(&mut s);
    assert_eq!((l[0]["name"].as_str(), l[0]["auto"].as_bool()), (Some("Autosave"), Some(true)));
    assert!(!autosave(&mut s), "no change since the checkpoint");
    // A version the user saves is a checkpoint too.
    rect(&mut s, 20.0);
    s.execute("file.saveVersion", &json!({"name": "Mine"})).unwrap();
    assert!(!autosave(&mut s));
    assert!(list(&mut s)[1].get("auto").is_none());
    // Not while a version is shown.
    rect(&mut s, 40.0);
    s.execute("version.preview", &json!({"index": 0})).unwrap();
    assert!(!autosave(&mut s));
    s.execute("version.exitPreview", &json!({})).unwrap();
    assert!(autosave(&mut s));
}

#[test]
fn oldest_autosaves_make_room_and_user_versions_stay() {
    let mut s = Session::new();
    rect(&mut s, 0.0);
    s.execute("file.saveVersion", &json!({"name": "Keep me"})).unwrap();
    for i in 0..(crate::cmd::file::MAX_AUTOSAVES + 5) {
        rect(&mut s, i as f64 * 20.0);
        assert!(autosave(&mut s));
    }
    let l = list(&mut s);
    assert_eq!(l.iter().filter(|v| v["auto"] == true).count(), crate::cmd::file::MAX_AUTOSAVES);
    assert_eq!(l[0]["name"], "Keep me");
}

#[test]
fn autosaves_keep_their_flag_in_the_file() {
    let dir = std::env::temp_dir().join(format!("figcraft-autosave-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("auto.figcraft").to_string_lossy().to_string();
    let mut s = Session::new();
    rect(&mut s, 0.0);
    assert!(autosave(&mut s));
    s.execute("file.saveVersion", &json!({"name": "Named"})).unwrap();
    s.execute("file.save", &json!({"path": path})).unwrap();
    let mut t = Session::empty();
    t.execute("file.open", &json!({"path": path})).unwrap();
    let l = list(&mut t);
    assert_eq!(l.len(), 2);
    assert_eq!(l[0]["auto"], true);
    assert!(l[1].get("auto").is_none());
    // Just opened: nothing to autosave.
    assert!(!autosave(&mut t));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn files_that_save_themselves_still_get_checkpoints() {
    let dir = std::env::temp_dir().join(format!("figcraft-autosave-on-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("auto.figcraft").to_string_lossy().to_string();
    let mut s = Session::new();
    s.execute("pref.autoSave", &json!({"on": true})).unwrap();
    rect(&mut s, 0.0);
    s.execute("file.save", &json!({"path": path})).unwrap();
    assert!(autosave(&mut s), "saved, but changed since the last version");
    assert!(!autosave(&mut s), "no change since the checkpoint");
    // Just opened: nothing to checkpoint.
    let mut t = Session::empty();
    t.execute("pref.autoSave", &json!({"on": true})).unwrap();
    t.execute("file.open", &json!({"path": path})).unwrap();
    assert!(!autosave(&mut t));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_encoded_elsewhere_is_saved_as_of_its_snapshot() {
    use crate::cmd::file::FileSnapshot;
    let dir = std::env::temp_dir().join(format!("figcraft-snapshot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("snap.figcraft").to_string_lossy().to_string();
    let mut s = Session::new();
    s.execute("file.save", &json!({"path": path})).unwrap();
    rect(&mut s, 0.0);
    let file = FileSnapshot::of(s.ds().unwrap());
    let bytes = file.encode().unwrap();
    rect(&mut s, 50.0);
    assert!(file.save(&mut s, &bytes).unwrap());
    assert!(s.ds().unwrap().is_dirty(), "the edit since the snapshot is still unsaved");
    let mut t = Session::empty();
    t.execute("file.open", &json!({"path": path})).unwrap();
    assert_eq!(t.ds().unwrap().doc.page_content_bounds(t.ds().unwrap().page).unwrap().x1, 10.0, "the file is the snapshot");
    // Saved meanwhile: the older bytes never overwrite the newer file.
    let file = FileSnapshot::of(s.ds().unwrap());
    let bytes = file.encode().unwrap();
    rect(&mut s, 100.0);
    s.execute("file.save", &json!({})).unwrap();
    assert!(!file.save(&mut s, &bytes).unwrap());
    assert!(!s.ds().unwrap().is_dirty());
    let mut t = Session::empty();
    t.execute("file.open", &json!({"path": path})).unwrap();
    assert_eq!(t.ds().unwrap().doc.page_content_bounds(t.ds().unwrap().page).unwrap().x1, 110.0);
    let _ = std::fs::remove_dir_all(&dir);
}

// Native background-save controls: identical on the original and repaired source snapshots.
struct NativeSnapshotBranch {
    session: Session,
    directory: std::path::PathBuf,
    path: String,
    main: u64,
    branch: u64,
    id: String,
    _cleanup: NativeSnapshotCleanup,
}

struct NativeSnapshotCleanup(std::path::PathBuf);

struct NativeBranchCapture {
    snapshot: crate::cmd::file::FileSnapshot,
    bytes: Vec<u8>,
    branch: figcraft_format::Branch,
    revision: u64,
}

impl NativeSnapshotBranch {
    fn new(label: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory =
            std::env::temp_dir().join(format!("figcraft-native-snapshot-{}-{}-{label}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&directory).unwrap();
        let cleanup = NativeSnapshotCleanup(directory.clone());
        let path = directory.join("main.figcraft").to_string_lossy().into_owned();
        let mut session = Session::new();
        rect(&mut session, 0.0);
        session.execute("file.save", &json!({"path":path})).unwrap();
        let main = session.ds().unwrap().uid;
        let created = session.execute("branch.create", &json!({"name":"Working branch"})).unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        let branch = session.ds().unwrap().uid;
        session.execute("file.save", &json!({})).unwrap();
        rect(&mut session, 20.0);
        session.execute("branch.list", &json!({})).unwrap();
        let fixture = Self { session, directory, path, main, branch, id, _cleanup: cleanup };
        assert!(fixture.main().is_dirty());
        assert!(fixture.branch().is_dirty());
        fixture
    }

    fn main(&self) -> &crate::DocState {
        self.session.docs.iter().find(|d| d.uid == self.main).unwrap()
    }

    fn branch(&self) -> &crate::DocState {
        self.session.docs.iter().find(|d| d.uid == self.branch).unwrap()
    }

    fn stored_branch(&self) -> &figcraft_format::Branch {
        self.main().branches.iter().find(|b| b.id == self.id).unwrap()
    }

    fn capture(&self) -> NativeBranchCapture {
        let snapshot = crate::cmd::file::FileSnapshot::of(self.main());
        let bytes = snapshot.encode().unwrap();
        NativeBranchCapture { snapshot, bytes, branch: self.stored_branch().clone(), revision: self.main().revision }
    }

    fn disk_branch(&self) -> figcraft_format::Branch {
        let mut reopened = Session::empty();
        reopened.execute("file.open", &json!({"path":self.path})).unwrap();
        reopened.ds().unwrap().branches.iter().find(|b| b.id == self.id).unwrap().clone()
    }

    fn switch_to_main(&mut self) {
        let index = self.session.docs.iter().position(|d| d.uid == self.main).unwrap();
        self.session.execute("doc.switch", &json!({"index":index})).unwrap();
    }
}

impl Drop for NativeSnapshotCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn native_snapshot_branch_edit_case(sync: bool) {
    let mut f = NativeSnapshotBranch::new(if sync { "synced" } else { "unsynced" });
    let capture = f.capture();
    rect(&mut f.session, 50.0);
    let live = f.branch().doc.clone();
    if sync {
        f.session.execute("branch.list", &json!({})).unwrap();
    }
    assert_eq!(f.main().revision, capture.revision, "a branch edit does not revise the main document");
    assert!(capture.snapshot.save(&mut f.session, &capture.bytes).unwrap());
    let disk = f.disk_branch();
    assert_eq!(disk.head, capture.branch.head, "the actual archive is the completed A snapshot");
    assert_ne!(disk.head, live, "new branch B was not in the bytes written");
    assert_eq!(f.stored_branch().head, live, "native completion must synchronize an unsynced B too");
    assert!(f.main().is_dirty(), "newer branch metadata must remain unsaved");
    assert!(f.branch().is_dirty(), "an A acknowledgment must not mark branch B saved");
}

#[test]
fn native_snapshot_synced_newer_branch_edit_stays_dirty() {
    native_snapshot_branch_edit_case(true);
}

#[test]
fn native_snapshot_unsynced_newer_branch_edit_stays_dirty() {
    // The final public edit is not followed by branch.list; this is reachable before the next
    // GUI command, when the worker completion invokes FileSnapshot::save directly.
    native_snapshot_branch_edit_case(false);
}

// Native history loads version snapshots lazily. Compare the ordered authored metadata and
// materialized content, rather than the private Doc/Stored representation of a Version.
fn native_snapshot_assert_version_readback(actual: &[figcraft_format::Version], expected: &[figcraft_format::Version]) {
    assert_eq!(actual.len(), expected.len(), "the actual archive preserves the exact version count");
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(
            (actual.id.as_str(), actual.name.as_str(), actual.description.as_str(), actual.time, actual.auto),
            (expected.id.as_str(), expected.name.as_str(), expected.description.as_str(), expected.time, expected.auto),
            "the actual archive preserves ordered version metadata"
        );
        assert_eq!(actual.images(), expected.images(), "the actual archive preserves version image data");
        assert_eq!(actual.document().unwrap(), expected.document().unwrap(), "the actual archive preserves each version document");
    }
}

fn native_snapshot_branch_version_case(sync: bool) {
    let mut f = NativeSnapshotBranch::new(if sync { "version-synced" } else { "version-unsynced" });
    let capture = f.capture();
    f.session.execute("file.saveVersion", &json!({"name":"After capture"})).unwrap();
    let versions = f.branch().versions.clone();
    assert_eq!(versions.len(), capture.branch.versions.len() + 1);
    assert_eq!(f.branch().doc, capture.branch.head, "the branch's geometry did not change");
    if sync {
        f.session.execute("branch.list", &json!({})).unwrap();
    }
    assert_eq!(f.main().revision, capture.revision);
    assert!(capture.snapshot.save(&mut f.session, &capture.bytes).unwrap());
    let disk = f.disk_branch();
    assert_eq!(disk.head, capture.branch.head);
    native_snapshot_assert_version_readback(&disk.versions, &capture.branch.versions);
    assert_eq!(f.stored_branch().versions, versions, "new version metadata remains in the live main");
    assert!(f.main().is_dirty());
    assert!(f.branch().is_dirty(), "a metadata-stale capture cannot acknowledge the branch");
}

#[test]
fn native_snapshot_synced_newer_branch_version_stays_dirty() {
    native_snapshot_branch_version_case(true);
}

#[test]
fn native_snapshot_unsynced_newer_branch_version_stays_dirty() {
    native_snapshot_branch_version_case(false);
}

#[test]
fn native_snapshot_branch_version_aba_keeps_generation_dirty() {
    let mut f = NativeSnapshotBranch::new("version-aba");
    let capture = f.capture();
    let index = f.branch().versions.len();
    f.session.execute("file.saveVersion", &json!({"name":"Transient version"})).unwrap();
    f.session.execute("version.delete", &json!({"index":index})).unwrap();
    assert_eq!(f.branch().doc, capture.branch.head);
    assert_eq!(f.branch().versions, capture.branch.versions, "values returned to A but their generation changed");
    assert_eq!(f.main().revision, capture.revision);
    assert!(capture.snapshot.save(&mut f.session, &capture.bytes).unwrap());
    let disk = f.disk_branch();
    assert_eq!(disk.head, capture.branch.head);
    native_snapshot_assert_version_readback(&disk.versions, &capture.branch.versions);
    assert_eq!(f.stored_branch().versions, f.branch().versions, "completion synchronizes the metadata back to A");
    assert!(f.main().is_dirty(), "metadata ABA must retain the conservative generation guard");
    assert!(f.branch().is_dirty());
}

#[test]
fn native_snapshot_branch_geometry_aba_keeps_generation_dirty() {
    let mut f = NativeSnapshotBranch::new("geometry-aba");
    let capture = f.capture();
    rect(&mut f.session, 50.0);
    assert_eq!(f.session.execute("edit.undo", &json!({})).unwrap()["undone"], true);
    assert_eq!(f.branch().doc, capture.branch.head, "undo restored the exact captured A contents");
    assert_eq!(f.main().revision, capture.revision);
    assert!(capture.snapshot.save(&mut f.session, &capture.bytes).unwrap());
    assert_eq!(f.disk_branch().head, capture.branch.head);
    assert_eq!(f.stored_branch().head, f.branch().doc, "completion synchronizes the head back to A");
    assert!(f.main().is_dirty(), "sync observed B and then A after the capture");
    assert!(f.branch().is_dirty());
}

#[test]
fn native_snapshot_newer_branch_only_save_supersedes_old_callback() {
    let mut f = NativeSnapshotBranch::new("newer-save");
    let capture = f.capture();
    let main_doc = f.main().doc.clone();
    rect(&mut f.session, 50.0);
    f.session.execute("file.save", &json!({})).unwrap();
    let live = f.branch().doc.clone();
    let before = std::fs::read(&f.path).unwrap();
    let parent_revision = f.main().revision;
    assert!(std::sync::Arc::ptr_eq(&f.main().doc, &main_doc), "the main authored document stayed unchanged");
    assert!(!f.main().is_dirty() && !f.branch().is_dirty());
    assert!(!capture.snapshot.save(&mut f.session, &capture.bytes).unwrap(), "newer save B supersedes the old A writer");
    assert_eq!(std::fs::read(&f.path).unwrap(), before, "refused callback writes no bytes");
    assert_eq!(f.main().revision, parent_revision, "refused callback changes no save state");
    assert_eq!(f.disk_branch().head, live);
    assert!(!f.main().is_dirty() && !f.branch().is_dirty());
}

#[test]
fn native_snapshot_identical_newer_save_also_supersedes_old_callback() {
    let mut f = NativeSnapshotBranch::new("identical-save");
    let capture = f.capture();
    f.session.execute("file.save", &json!({})).unwrap();
    let before = std::fs::read(&f.path).unwrap();
    let revision = f.main().revision;
    assert!(!capture.snapshot.save(&mut f.session, &capture.bytes).unwrap());
    assert_eq!(std::fs::read(&f.path).unwrap(), before);
    assert_eq!(f.main().revision, revision);
    assert!(!f.main().is_dirty() && !f.branch().is_dirty());
}

#[test]
fn native_snapshot_matching_metadata_still_saves_after_harmless_queries() {
    let mut f = NativeSnapshotBranch::new("matching");
    let capture = f.capture();
    f.session.execute("version.list", &json!({})).unwrap();
    assert_eq!(f.main().revision, capture.revision);
    assert!(capture.snapshot.save(&mut f.session, &capture.bytes).unwrap());
    let disk = f.disk_branch();
    assert_eq!(disk.head, f.branch().doc);
    native_snapshot_assert_version_readback(&disk.versions, &f.branch().versions);
    assert!(!f.main().is_dirty());
    assert!(!f.branch().is_dirty());
}

#[test]
fn native_snapshot_later_authored_edit_keeps_existing_snapshot_write_contract() {
    let mut f = NativeSnapshotBranch::new("authored");
    let capture = f.capture();
    f.switch_to_main();
    rect(&mut f.session, 100.0);
    assert!(f.main().revision > capture.revision);
    assert!(capture.snapshot.save(&mut f.session, &capture.bytes).unwrap(), "older authored snapshots may still write");
    assert!(f.main().is_dirty(), "the newer authored edit remains unsaved");
    assert!(!f.branch().is_dirty(), "the unchanged branch payload was included in the bytes");
    let mut reopened = Session::empty();
    reopened.execute("file.open", &json!({"path":f.path})).unwrap();
    let ds = reopened.ds().unwrap();
    assert_eq!(ds.doc.page_content_bounds(ds.page).unwrap().x1, 10.0);
}

#[test]
fn native_snapshot_changed_path_refuses_old_callback_without_writing() {
    let mut f = NativeSnapshotBranch::new("path");
    let capture = f.capture();
    let before = std::fs::read(&f.path).unwrap();
    let other = f.directory.join("other.figcraft").to_string_lossy().into_owned();
    f.session.execute("file.saveAs", &json!({"path":other})).unwrap();
    rect(&mut f.session, 70.0);
    let stored = f.stored_branch().head.clone();
    let live = f.branch().doc.clone();
    let dirty = f.main().is_dirty();
    let revision = f.main().revision;
    assert!(!capture.snapshot.save(&mut f.session, &capture.bytes).unwrap());
    assert_eq!(std::fs::read(&f.path).unwrap(), before);
    assert_eq!(f.main().revision, revision);
    assert_eq!(f.main().path.as_deref(), Some(other.as_str()));
    assert_eq!(f.stored_branch().head, stored, "refused stale writer must not synchronize other state");
    assert!(std::sync::Arc::ptr_eq(&f.branch().doc, &live));
    assert_eq!(f.main().is_dirty(), dirty);
    assert!(f.branch().is_dirty());
}

#[test]
fn native_snapshot_replaced_uid_refuses_old_callback_without_writing() {
    let mut f = NativeSnapshotBranch::new("uid");
    let capture = f.capture();
    let before = std::fs::read(&f.path).unwrap();
    f.switch_to_main();
    f.session.execute("file.revert", &json!({})).unwrap();
    let uid = f.session.ds().unwrap().uid;
    let revision = f.session.ds().unwrap().revision;
    assert_ne!(uid, f.main);
    assert!(!capture.snapshot.save(&mut f.session, &capture.bytes).unwrap());
    assert_eq!(std::fs::read(&f.path).unwrap(), before);
    assert_eq!((f.session.ds().unwrap().uid, f.session.ds().unwrap().revision), (uid, revision));
}

#[test]
fn native_snapshot_failed_history_rollback_keeps_capture_identity_usable() {
    let mut f = NativeSnapshotBranch::new("rollback");
    f.switch_to_main();
    let capture = f.capture();
    let before = f.main().clone();
    struct ResetPanic(bool);
    impl Drop for ResetPanic {
        fn drop(&mut self) {
            crate::derived::PANIC.with(|flag| flag.set(self.0));
        }
    }
    let failed = {
        let previous = crate::derived::PANIC.with(|flag| flag.replace(true));
        let _reset = ResetPanic(previous);
        f.session.execute("edit.undo", &json!({}))
    };
    assert!(failed.is_err());
    assert!(std::sync::Arc::ptr_eq(&f.main().doc, &before.doc));
    assert!(std::sync::Arc::ptr_eq(&f.main().saved_doc, &before.saved_doc));
    assert_eq!((f.main().history.undo.len(), f.main().history.redo.len()), (before.history.undo.len(), before.history.redo.len()));
    assert_eq!((f.main().uid, f.main().revision, &f.main().path), (before.uid, before.revision, &before.path));
    assert!(capture.snapshot.save(&mut f.session, &capture.bytes).unwrap(), "failed history restored the captured save identity");
    assert_eq!(f.disk_branch().head, f.branch().doc);
    assert!(!f.main().is_dirty() && !f.branch().is_dirty());
}

#[test]
fn native_snapshot_clean_geometry_newer_version_keeps_parent_dirty() {
    let mut f = NativeSnapshotBranch::new("clean-version");
    f.session.execute("file.save", &json!({})).unwrap();
    assert!(!f.main().is_dirty() && !f.branch().is_dirty());
    let capture = f.capture();
    let main_saved = f.main().saved_doc.clone();
    let branch_saved = f.branch().saved_doc.clone();
    f.session.execute("file.saveVersion", &json!({"name":"Metadata after clean capture"})).unwrap();
    f.session.execute("branch.list", &json!({})).unwrap();
    let versions = f.branch().versions.clone();
    assert_eq!(versions.len(), capture.branch.versions.len() + 1);
    assert_eq!(f.branch().doc, capture.branch.head, "the saved branch geometry stayed at A");
    assert_eq!(f.main().revision, capture.revision, "version-only branch metadata does not revise the main document");
    assert!(f.main().is_dirty());
    // sync folds branch version metadata into the parent and may clear the tab's metadata flag.
    // The parent's dirty flag is authoritative for the file; completion must preserve that policy.
    let branch_dirty = f.branch().is_dirty();
    assert!(std::sync::Arc::ptr_eq(&f.main().saved_doc, &main_saved));
    assert!(std::sync::Arc::ptr_eq(&f.branch().saved_doc, &branch_saved));
    assert!(capture.snapshot.save(&mut f.session, &capture.bytes).unwrap());
    let disk = f.disk_branch();
    assert_eq!(disk.head, capture.branch.head);
    native_snapshot_assert_version_readback(&disk.versions, &capture.branch.versions);
    assert_eq!(f.branch().versions, versions, "the branch keeps the newer live version history");
    assert_eq!(f.stored_branch().versions, versions, "the main keeps the newer live version history");
    assert!(f.main().is_dirty(), "the newer version was not written in captured A");
    assert!(std::sync::Arc::ptr_eq(&f.main().saved_doc, &main_saved), "unchanged authored geometry retains its saved checkpoint");
    assert!(std::sync::Arc::ptr_eq(&f.branch().saved_doc, &branch_saved), "stale metadata must not replace the branch checkpoint");
    assert_eq!(f.branch().is_dirty(), branch_dirty, "the existing branch sync policy is preserved");
}
