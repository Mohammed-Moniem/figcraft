//! The project library's file: kept and read back, a backup of the last good one, a damaged or
//! repaired file kept aside (never lost), one that can't be read never written over, and a first
//! run that starts from the recent design files.

use std::path::{Path, PathBuf};

use figcraft_engine::projects::{Favorite, Projects};
use figcraft_ui_egui::ProjectsNotice;

use crate::projects_store::{BACKUP, FILE, read, seeded, write};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("figcraft-projects-store-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn library() -> Projects {
    let mut p = Projects::default();
    let northwind = p.create_folder("NORTHWIND BUILDERS", None, 1).unwrap();
    let social = p.create_folder("Social Media", Some(&northwind), 2).unwrap();
    p.move_files(&["/w/post.figcraft".to_string()], Some(&social), 3).unwrap();
    p.set_favorite(&Favorite::Folder(northwind), true, 4).unwrap();
    p.unsaved = false;
    p
}

fn names(d: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn kept_and_read_back_whole() {
    let d = dir("roundtrip");
    assert!(read(&d).projects.is_none(), "none yet");
    let p = library();
    write(&d, &p).unwrap();
    let back = read(&d);
    assert_eq!(back.projects.as_ref(), Some(&p));
    assert!(back.notice.is_none() && back.blocked.is_none());
    // The second save keeps the first as the backup; no staging file is left behind.
    let mut q = p.clone();
    q.create_folder("LUMEN", None, 5).unwrap();
    write(&d, &q).unwrap();
    assert_eq!(names(&d), [FILE, BACKUP]);
    let (bak, _) = Projects::from_json(&std::fs::read_to_string(d.join(BACKUP)).unwrap()).unwrap();
    assert_eq!(bak, p);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_damaged_file_is_kept_aside_and_the_backup_comes_back() {
    let d = dir("damaged");
    let p = library();
    write(&d, &p).unwrap();
    write(&d, &p).unwrap();
    std::fs::write(d.join(FILE), b"{ \"folders\": [ truncated").unwrap();
    let back = read(&d);
    assert_eq!(back.projects.as_ref().map(|x| &x.folders), Some(&p.folders), "the last good copy");
    assert!(back.projects.as_ref().is_some_and(|x| x.unsaved), "kept again at once");
    let Some(ProjectsNotice::Restored { kept: Some(kept) }) = back.notice else { panic!("{:?}", back.notice) };
    assert_eq!(std::fs::read(&kept).unwrap(), b"{ \"folders\": [ truncated", "the damaged file is kept as it was");
    assert!(back.blocked.is_none());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn damaged_without_a_backup_starts_empty_and_keeps_the_file() {
    let d = dir("nobackup");
    std::fs::write(d.join(FILE), b"not json").unwrap();
    let back = read(&d);
    assert_eq!(back.projects, Some(Projects::default()));
    let Some(ProjectsNotice::Damaged { kept: Some(kept) }) = back.notice else { panic!("{:?}", back.notice) };
    assert_eq!(std::fs::read(kept).unwrap(), b"not json");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn broken_references_are_repaired_and_the_original_kept() {
    let d = dir("repaired");
    let text = r#"{"version":1,"next_id":3,"folders":[{"id":"f1","name":"A","parent":"f9"}],"files":[{"path":"/a.figcraft","folder":"f7"}],"favorites":[{"type":"folder","id":"f8"}]}"#;
    std::fs::write(d.join(FILE), text).unwrap();
    let back = read(&d);
    let p = back.projects.unwrap();
    assert_eq!(p.folder("f1").unwrap().parent, None);
    assert_eq!(p.file("/a.figcraft").unwrap().folder, None, "the file is kept, in no folder");
    assert!(p.favorites.is_empty());
    assert!(p.unsaved);
    let Some(ProjectsNotice::Repaired { fixes, kept: Some(kept) }) = back.notice else { panic!() };
    assert_eq!(fixes, 3);
    assert_eq!(std::fs::read_to_string(kept).unwrap(), text);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_file_that_cant_be_read_is_never_written_over() {
    let d = dir("unreadable");
    // A folder where the file should be: it can't be read as text.
    std::fs::create_dir_all(d.join(FILE)).unwrap();
    let back = read(&d);
    assert!(back.projects.is_none());
    assert!(back.blocked.is_some());
    assert!(matches!(back.notice, Some(ProjectsNotice::Unreadable { .. })));
    // The services refuse to save over it.
    let mut svc = crate::services::Desktop::load_from(Some(d.clone()), crate::recovery::Shared::default());
    assert!(figcraft_ui_egui::Services::save_projects(&mut svc, &library()).is_err());
    assert!(d.join(FILE).is_dir(), "left as it was");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_removed_file_comes_back_from_its_backup() {
    let d = dir("removed");
    write(&d, &library()).unwrap();
    write(&d, &library()).unwrap();
    std::fs::remove_file(d.join(FILE)).unwrap();
    let back = read(&d);
    assert_eq!(back.projects.map(|p| p.folders.len()), Some(2));
    assert!(matches!(back.notice, Some(ProjectsNotice::Restored { kept: None })));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_first_run_starts_from_the_recent_design_files() {
    let recents: Vec<String> = ["/w/new.figcraft", "/w/pic.png", "/w/old.fig", "/w/x.sketch"].iter().map(|s| s.to_string()).collect();
    let p = seeded(&recents);
    let paths: Vec<&str> = p.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["/w/x.sketch", "/w/old.fig", "/w/new.figcraft"], "oldest first, images left out");
    assert!(p.unsaved);
    assert!(!seeded(&[]).unsaved);
}

#[test]
fn the_services_keep_the_library_where_the_preferences_are() {
    let d = dir("services");
    let mut svc = crate::services::Desktop::load_from(Some(d.clone()), crate::recovery::Shared::default());
    assert!(svc.projects.is_none());
    figcraft_ui_egui::Services::save_projects(&mut svc, &library()).unwrap();
    let again = crate::services::Desktop::load_from(Some(d.clone()), crate::recovery::Shared::default());
    assert_eq!(again.projects, Some(library()));
    let _ = std::fs::remove_dir_all(d);
}

// PR1401-S3 bounded project metadata: synthetic references, production loader/store.
fn project_limits_corpus(folders: usize, files: usize) -> Projects {
    use figcraft_engine::projects::{Folder, ProjectFile};
    Projects {
        next_id: folders as u64 + 1,
        folders: (1..=folders).map(|i| Folder { id: format!("f{i}"), name: format!("F{i}"), parent: None, created: 0 }).collect(),
        files: (0..files).map(|i| ProjectFile { path: format!("/{i:05}.fig"), folder: None, added: 0, kind: None }).collect(),
        ..Projects::default()
    }
}

fn project_limits_measure<T>(name: &str, work: impl FnOnce() -> T) -> T {
    let started = std::time::Instant::now();
    let value = work();
    eprintln!("PROJECT_LIMIT_OPERATION {name} elapsed_ms={:.3}", started.elapsed().as_secs_f64() * 1000.0);
    value
}

#[test]
fn project_limits_count_boundary_roundtrips_and_one_over_mutations_do_not_change_state() {
    use figcraft_engine::projects::{MAX_FILES, MAX_FOLDERS};
    assert_eq!((MAX_FOLDERS, MAX_FILES), (5_000, 50_000));
    let d = dir("limits-count");
    let mut p = project_limits_corpus(MAX_FOLDERS, MAX_FILES);
    assert_eq!(project_limits_measure("native-count-list-folders", || p.children(None)).len(), MAX_FOLDERS);
    assert_eq!(project_limits_measure("native-count-list-files", || p.files_in(None).count()), MAX_FILES);
    let bytes = p.to_json().len();
    assert!(bytes <= 4 * 1024 * 1024, "count fixture must fit the independent byte bound: {bytes}");
    eprintln!("PROJECT_LIMIT_CORPUS native-count folders={} files={} bytes={bytes}", p.folders.len(), p.files.len());
    std::fs::write(d.join("design-sentinel.figcraft"), b"synthetic design sentinel: byte-preservation only").unwrap();
    let design = std::fs::read(d.join("design-sentinel.figcraft")).unwrap();
    project_limits_measure("native-count-write", || write(&d, &p)).unwrap();
    project_limits_measure("native-count-write-backup", || write(&d, &p)).unwrap();
    let metadata = std::fs::read(d.join(FILE)).unwrap();
    let backup = std::fs::read(d.join(BACKUP)).unwrap();
    let loaded = project_limits_measure("native-count-read-repair", || read(&d));
    assert_eq!(loaded.projects, Some(p.clone()));
    assert!(loaded.notice.is_none() && loaded.blocked.is_none());
    assert!(project_limits_measure("native-count-folder-one-over", || p.create_folder("Extra", None, 0)).is_err());
    assert_eq!(p, loaded.projects.unwrap());
    assert!(!project_limits_measure("native-count-file-one-over", || p.touch("/extra.fig", None, 0)));
    assert!(p.move_files(&["/extra.fig".to_string()], None, 0).is_err());
    assert_eq!(p.to_json().as_bytes(), metadata);
    assert_eq!(std::fs::read(d.join(FILE)).unwrap(), metadata);
    assert_eq!(std::fs::read(d.join(BACKUP)).unwrap(), backup);
    assert_eq!(std::fs::read(d.join("design-sentinel.figcraft")).unwrap(), design);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn project_limits_one_over_count_input_is_repaired_with_exact_original_kept() {
    use figcraft_engine::projects::{MAX_FILES, MAX_FOLDERS};
    let d = dir("limits-incoming-count");
    let good = library();
    write(&d, &good).unwrap();
    write(&d, &good).unwrap();
    let backup = std::fs::read(d.join(BACKUP)).unwrap();
    let incoming = project_limits_corpus(MAX_FOLDERS + 1, MAX_FILES + 1).to_json();
    assert!(incoming.len() <= 4 * 1024 * 1024);
    std::fs::write(d.join(FILE), &incoming).unwrap();
    let loaded = project_limits_measure("native-one-over-count-read-repair", || read(&d));
    let p = loaded.projects.unwrap();
    assert_eq!((p.folders.len(), p.files.len()), (MAX_FOLDERS, MAX_FILES));
    assert!(p.unsaved && loaded.blocked.is_none());
    let Some(ProjectsNotice::Repaired { fixes, kept: Some(kept) }) = loaded.notice else { panic!("expected retained original and repair notice") };
    assert_eq!(fixes, 2);
    assert_eq!(std::fs::read(kept).unwrap(), incoming.as_bytes());
    assert_eq!(std::fs::read(d.join(FILE)).unwrap(), incoming.as_bytes(), "loader does not silently replace incoming bytes");
    assert_eq!(std::fs::read(d.join(BACKUP)).unwrap(), backup);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn project_limits_depth_boundary_refuses_new_child_and_repairs_incoming_depth_33() {
    use figcraft_engine::projects::MAX_DEPTH;
    assert_eq!(MAX_DEPTH, 32);
    let d = dir("limits-depth");
    let mut p = Projects::default();
    let mut last = None;
    for i in 0..MAX_DEPTH {
        last = Some(p.create_folder(&format!("Depth {i}"), last.as_deref(), 0).unwrap());
    }
    p.unsaved = false;
    let id = last.unwrap();
    assert_eq!(p.depth(&id), MAX_DEPTH);
    let before = p.clone();
    assert!(p.create_folder("One too deep", Some(&id), 0).is_err());
    assert_eq!(p, before);
    write(&d, &p).unwrap();
    assert_eq!(project_limits_measure("native-depth-32-read", || read(&d)).projects, Some(p.clone()));
    let mut incoming = project_limits_corpus(MAX_DEPTH + 1, 0);
    for i in 1..incoming.folders.len() {
        incoming.folders[i].parent = Some(format!("f{i}"));
    }
    let text = incoming.to_json();
    std::fs::write(d.join(FILE), &text).unwrap();
    let loaded = project_limits_measure("native-depth-33-read-repair", || read(&d));
    let repaired = loaded.projects.unwrap();
    assert_eq!(repaired.folders.len(), MAX_DEPTH + 1);
    assert!(repaired.folders.iter().all(|f| repaired.depth(&f.id) <= MAX_DEPTH));
    let Some(ProjectsNotice::Repaired { fixes: 1, kept: Some(kept) }) = loaded.notice else { panic!("expected depth repair and exact original") };
    assert_eq!(std::fs::read(kept).unwrap(), text.as_bytes());
    std::fs::remove_dir_all(d).unwrap();
}

fn project_limits_exact_byte_corpus() -> Projects {
    let mut p = project_limits_corpus(0, 1024);
    for f in &mut p.files {
        f.path.push_str(&"x".repeat(4000 - f.path.len()));
    }
    let mut needed = (4 * 1024 * 1024usize).checked_sub(p.to_json().len()).expect("short-path corpus is below 4 MiB");
    for f in &mut p.files {
        let take = needed.min(4096 - f.path.len());
        f.path.push_str(&"x".repeat(take));
        needed -= take;
    }
    assert_eq!(needed, 0, "bounded synthetic paths can reach the exact byte limit");
    assert_eq!(p.to_json().len(), 4 * 1024 * 1024);
    assert!(p.files.iter().all(|f| f.path.len() <= 4096));
    p
}

#[test]
fn project_limits_exact_byte_write_read_and_one_over_preserve_current_and_distinct_backup() {
    let d = dir("limits-bytes");
    write(&d, &library()).unwrap();
    let mut p = project_limits_exact_byte_corpus();
    eprintln!("PROJECT_LIMIT_CORPUS native-bytes folders={} files={} bytes={}", p.folders.len(), p.files.len(), p.to_json().len());
    project_limits_measure("native-exact-4mib-write", || write(&d, &p)).unwrap();
    let metadata = std::fs::read(d.join(FILE)).unwrap();
    let backup = std::fs::read(d.join(BACKUP)).unwrap();
    assert_ne!(metadata, backup, "exercise preservation of a distinct last-good backup");
    let loaded = project_limits_measure("native-exact-4mib-read", || read(&d));
    assert_eq!(loaded.projects, Some(p.clone()));
    assert!(loaded.notice.is_none() && loaded.blocked.is_none());
    p.files.iter_mut().find(|f| f.path.len() < 4096).unwrap().path.push('x');
    assert_eq!(p.to_json().len(), 4 * 1024 * 1024 + 1);
    assert!(project_limits_measure("native-one-byte-over-write", || write(&d, &p)).is_err());
    assert_eq!(std::fs::read(d.join(FILE)).unwrap(), metadata);
    assert_eq!(std::fs::read(d.join(BACKUP)).unwrap(), backup, "a refused candidate must not replace a different last-good backup");
    std::fs::write(d.join(FILE), p.to_json()).unwrap();
    let over = std::fs::read(d.join(FILE)).unwrap();
    let blocked = project_limits_measure("native-one-byte-over-read", || read(&d));
    assert!(blocked.projects.is_none() && blocked.blocked.is_some());
    assert!(matches!(blocked.notice, Some(ProjectsNotice::Unreadable { .. })));
    assert_eq!(std::fs::read(d.join(FILE)).unwrap(), over);
    assert_eq!(std::fs::read(d.join(BACKUP)).unwrap(), backup);
    std::fs::remove_dir_all(d).unwrap();
}
