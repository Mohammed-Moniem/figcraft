use super::export::export_fig;
use super::load::{ext, file_stem, write};
use super::open::remember;
use super::*;

/// Thumbnail PNG of the chosen frame, or the first page's content (file browsers / home screen).
pub fn thumbnail(doc: &Document) -> Option<Vec<u8>> {
    let render = |page, b: figcraft_geom::Rect, solo| {
        let scale = (400.0 / b.width().max(b.height()).max(1.0)).min(2.0);
        let view = figcraft_render::View {
            origin: (b.x0, b.y0),
            scale,
            width: (b.width() * scale).ceil().max(1.0) as u32,
            height: (b.height() * scale).ceil().max(1.0) as u32,
        };
        let opts = figcraft_render::Options { solo, ..Default::default() };
        let pix = figcraft_render::render_view(doc, page, &view, &opts)?;
        figcraft_render::encode_png(&pix).ok()
    };
    doc.ids()
        .find_map(|id| {
            let n = doc.node(id)?;
            if !n.kind.is_frame_like() || n.plugin_data.get("figcraft.thumbnail").map(String::as_str) != Some("true") {
                return None;
            }
            // Hidden library-store pages are not document pages.
            let page = doc.page_of(id).filter(|p| doc.pages.contains(p))?;
            let b = figcraft_render::export_bounds(doc, id)?;
            if ![b.x0, b.y0, b.x1, b.y1, b.width(), b.height()].into_iter().all(f64::is_finite) || b.width() <= 0.0 || b.height() <= 0.0 {
                return None;
            }
            render(page, b, Some(id))
        })
        .or_else(|| {
            let page = doc.first_page()?;
            render(page, doc.page_content_bounds(page)?, None)
        })
}

/// The `.figcraft` file for a document: content, thumbnail and version history.
pub fn file_bytes(ds: &DocState) -> Result<Vec<u8>> {
    FileSnapshot::of(ds).encode()
}

/// Native bytes for a chosen output path (including CLI `run --out`).
pub fn file_bytes_for(ds: &DocState, destination: &str) -> Result<Vec<u8>> {
    FileSnapshot::of(ds).encode_for(Some(destination))
}

/// A document's file as it is at one moment, so the slow part of saving it (the thumbnail and the
/// encoding) can run on another thread while editing goes on: [`encode`](Self::encode) there, then
/// [`save`](Self::save) the bytes back on the session's thread.
pub struct FileSnapshot {
    uid: u64,
    path: Option<String>,
    library_origin: Option<String>,
    revision: u64,
    doc: Arc<Document>,
    /// The document as last saved when the snapshot was taken: another save since makes it stale.
    saved: Arc<Document>,
    save_identity: Arc<()>,
    metadata_generation: Option<u64>,
    page: NodeId,
    versions: Vec<Version>,
    branches: Vec<figcraft_format::Branch>,
}

impl FileSnapshot {
    pub fn of(ds: &DocState) -> Self {
        Self {
            uid: ds.uid,
            path: ds.path.clone(),
            library_origin: ds.library_origin.clone(),
            revision: ds.revision,
            doc: ds.doc.clone(),
            saved: ds.saved_doc.clone(),
            save_identity: ds.save_identity.clone(),
            metadata_generation: ds.metadata_generation(),
            page: ds.page,
            versions: ds.versions.clone(),
            branches: ds.branches.clone(),
        }
    }

    /// The file's bytes: content, thumbnail and version history.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.encode_for(self.path.as_deref())
    }

    /// Encode for a native destination without changing the tab's in-memory reference origin.
    pub fn encode_for(&self, destination: Option<&str>) -> Result<Vec<u8>> {
        let mut doc = (*self.doc).clone();
        figcraft_text::appearance::refresh_fonts(&mut doc, None);
        doc.current_page = doc.opening_page.or(Some(self.page));
        let thumb = thumbnail(&doc);
        let bytes = match destination {
            Some(destination) => {
                let origin = self.library_origin.clone().or_else(|| crate::library_paths::document_origin(Some(destination)));
                figcraft_format::save_bytes_with_history_paths(&doc, thumb.as_deref(), &self.versions, &self.branches, &|path| {
                    crate::library_paths::for_destination(path, origin.as_deref(), destination)
                })
            }
            None => figcraft_format::save_bytes_with_history(&doc, thumb.as_deref(), &self.versions, &self.branches),
        };
        bytes.map_err(|e| EngineError::Other(e.to_string()))
    }

    /// Write `bytes` (this snapshot's [`encode`](Self::encode)) to the document's file: it is saved
    /// as of the snapshot, and edits since stay unsaved. False (nothing written) when the document
    /// was closed, saved, reverted or given another path since: the bytes are out of date.
    pub fn save(self, s: &mut Session, bytes: &[u8]) -> Result<bool> {
        // A stale/closed target is a no-op. For a valid writer, flush branch edits which happened
        // during encoding before judging metadata: the main tab's revision need not change.
        if !s.docs.iter().any(|d| self.matches_saved_state(d)) {
            return Ok(false);
        }
        crate::branches::sync(s);
        let Some(ds) = s.docs.iter_mut().find(|d| self.matches_saved_state(d)) else {
            return Ok(false);
        };
        let path = self.path.as_deref().ok_or(EngineError::NoDocument)?;
        refuse_dropping_images(ds, path)?;
        refuse_overwriting_changes(ds, path)?;
        write(path, bytes)?;
        ds.note_on_disk();
        let exact_metadata = ds.mark_saved_snapshot(self.doc, self.revision, self.metadata_generation);
        ds.revision += 1;
        if exact_metadata {
            branch_tabs_saved_capture(s, self.uid, &self.branches);
        }
        Ok(true)
    }

    /// A newer branch-only or identical save also supersedes this writer; saved_doc alone
    /// cannot distinguish those saves because the main document may keep exactly the same Arc.
    fn matches_saved_state(&self, ds: &DocState) -> bool {
        ds.uid == self.uid && ds.path == self.path && Arc::ptr_eq(&ds.saved_doc, &self.saved) && Arc::ptr_eq(&ds.save_identity, &self.save_identity)
    }
}

impl Session {
    /// Pending host snapshots for still-open files, for a transport to retire closed, evicted
    /// or superseded tickets without treating a posted message as a successful save.
    pub fn pending_file_saves(&self) -> Vec<(u64, u64)> {
        self.pending_file_saves
            .iter()
            .filter(|snapshot| self.docs.iter().any(|d| d.uid == snapshot.uid))
            .map(|snapshot| (snapshot.uid, snapshot.revision))
            .collect()
    }
}

fn refuse_dropping_images(ds: &DocState, path: &str) -> Result<()> {
    match ds.images_lost_saving_to(path) {
        Some(count) => Err(EngineError::Other(format!(
            "{} wasn't saved: {count} of its images weren't loaded (together they pass what FigCraft reads from one file), and saving over it would delete them. Save it under another name; this file keeps them.",
            file_stem(path)
        ))),
        None => Ok(()),
    }
}

/// Saving over the tab's own file would lose what another app wrote to it since (#1330).
fn refuse_overwriting_changes(ds: &DocState, path: &str) -> Result<()> {
    match (&ds.on_disk, crate::library::stat(path)) {
        (Some(was), Ok(now)) if now != *was => Err(EngineError::Other(format!(
            "{} wasn't saved: the file changed on disk since it was opened or saved here (another app or figcraft-cli wrote to it), and saving would replace those changes. Revert to saved to load them, or Save as to keep this version as well.",
            file_stem(path)
        ))),
        (Some(_), Err(_)) => Err(EngineError::Other(
            "The saved file is missing or unreadable. Save a separate copy instead of replacing an unverified destination.".into(),
        )),
        _ => Ok(()),
    }
}

/// The branch tabs of the file in tab `uid` were saved with it.
fn branch_tabs_saved(s: &mut Session, uid: u64) {
    for tab in s.docs.iter_mut().filter(|d| d.branch_of.as_ref().is_some_and(|(m, _)| *m == uid)) {
        tab.mark_saved();
    }
}

/// Only branch payloads actually held by the completed capture may become clean, after its
/// parent metadata generation matched. Synchronous saves acknowledge their current branch set.
fn branch_tabs_saved_capture(s: &mut Session, uid: u64, captured: &[figcraft_format::Branch]) {
    for tab in s.docs.iter_mut().filter(|d| d.branch_of.as_ref().is_some_and(|(main, _)| *main == uid)) {
        let Some((_, id)) = tab.branch_of.as_ref() else { continue };
        if captured.iter().find(|b| &b.id == id).is_some_and(|b| Arc::ptr_eq(tab.document_itself(), &b.head) && tab.versions == b.versions) {
            tab.mark_saved();
        }
    }
}

pub(super) fn color_profile(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "file.colorProfile";
    let Some(name) = string(p, "profile") else {
        s.request_ui("dialog.colorProfile", json!({}));
        return Ok(json!({"profile": s.doc()?.color_profile.id(), "changed": false}));
    };
    let profile = figcraft_color::ColorProfile::parse(&name).ok_or_else(|| bad(cmd, "profile: srgb or displayP3"))?;
    let convert = boolean(p, "convert").unwrap_or(true);
    let changed = s.doc()?.color_profile != profile && s.doc_mut()?.set_color_profile(profile, convert);
    Ok(json!({"profile": profile.id(), "changed": changed}))
}

/// Native saves act on the current document; a `.fig` copy exports the version shown, and a
/// pending Save As choice must leave it shown until its destination is known.
pub(crate) fn save_ends_preview(s: &Session, cmd: &str, p: &Value) -> bool {
    let path =
        string(p, "path").or_else(|| if cmd == "file.save" { s.docs.get(crate::branches::file_tab(s)).and_then(|ds| ds.path.clone()) } else { None });
    path.is_some_and(|path| ext(&path) != "fig") || (cmd == "file.saveAs" && wants_data(p))
}

/// `file.saveAs` asked for the file's bytes instead of a path to write (the browser's download).
fn wants_data(p: &Value) -> bool {
    p.get("data").and_then(Value::as_bool) == Some(true)
}

/// Save the file of the active tab (the main file for a branch tab, with its branches), or export
/// the active document as an experimental `.fig` copy without changing its native save state.
/// Without a path the file comes back as base64 `data` for the caller to deliver (#969).
fn save_to(s: &mut Session, path: Option<String>, overwrite: bool) -> Result<Value> {
    if let Some(path) = path.as_deref().filter(|p| ext(p) == "fig") {
        return export_fig(s, &json!({"path": path}));
    }
    let i = crate::branches::file_tab(s);
    if let Some(path) = &path
        && s.docs.iter().enumerate().any(|(j, ds)| j != i && ds.path.as_deref().is_some_and(|p| crate::file_access::same_file(p, path)))
    {
        return Err(EngineError::Other(format!("{} is already open in another tab; choose another destination", file_stem(path))));
    }
    let ds = s.docs.get_mut(i).ok_or(EngineError::NoDocument)?;
    // A download (no path) writes over no file.
    if let Some(path) = path.as_deref() {
        refuse_dropping_images(ds, path)?;
        if !overwrite && ds.path.as_deref().is_some_and(|p| crate::file_access::same_file(p, path)) {
            refuse_overwriting_changes(ds, path)?;
        }
    }
    let mut snapshot = FileSnapshot::of(ds);
    let follows_path = ds.path.as_deref().map_or(ds.doc.name == "Untitled", |old| ds.doc.name == file_stem(old));
    if let Some(path) = path.as_deref()
        && follows_path
        && ds.path.as_deref() != Some(path)
    {
        Arc::make_mut(&mut snapshot.doc).name = file_stem(path);
    }
    let bytes = snapshot.encode_for(path.as_deref())?;
    let Some(path) = path else {
        // The download can still be refused: the caller marks the file saved once it was delivered.
        let name = format!("{}.figcraft", ds.doc.name);
        // Tab revision is not an immutable save identity: queries advance it, branches advance
        // another tab's, and a failed command group may roll it back. This session sequence is
        // never rolled back or reused; `revision` on a data export names its exact snapshot.
        s.file_save_revision = s.file_save_revision.checked_add(1).ok_or_else(|| EngineError::Other("save snapshot sequence exhausted".into()))?;
        snapshot.revision = s.file_save_revision;
        let (document, revision) = (snapshot.uid, snapshot.revision);
        s.pending_file_saves.retain(|p| s.docs.iter().any(|d| d.uid == p.uid));
        if s.pending_file_saves.len() >= 32 {
            s.pending_file_saves.remove(0);
        }
        s.pending_file_saves.push(snapshot);
        return Ok(
            json!({"name": name, "bytes": bytes.len(), "data": base64::engine::general_purpose::STANDARD.encode(&bytes), "document": document, "revision": revision}),
        );
    };
    write(&path, &bytes)?;
    ds.doc = snapshot.doc;
    if ds.library_origin.is_none() {
        ds.library_origin = crate::library_paths::document_origin(Some(&path));
    }
    ds.path = Some(path.clone());
    ds.note_on_disk();
    mark_saved(s, &json!({}))?;
    remember(s, &path);
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

/// The file of the active tab (with its branch tabs) now matches what was written or delivered.
pub(super) fn mark_saved(s: &mut Session, p: &Value) -> Result<Value> {
    if p.get("document").is_some() || p.get("revision").is_some() {
        let (Some(uid), Some(revision)) = (p.get("document").and_then(Value::as_u64), p.get("revision").and_then(Value::as_u64)) else {
            return Err(bad("file.markSaved", "document and revision must both be unsigned integers"));
        };
        let Some(i) = s.pending_file_saves.iter().position(|p| p.uid == uid && p.revision == revision) else {
            return Ok(json!({"saved": false, "document": uid, "revision": revision}));
        };
        let snapshot = s.pending_file_saves.remove(i);
        let Some(ds) = s.docs.iter_mut().find(|d| d.uid == uid) else {
            return Ok(json!({"saved": false, "document": uid, "revision": revision}));
        };
        // Another Save/revert supersedes this one; content/history/branch edits after the
        // export stay dirty. Harmless queries and normalization do not invalidate the snapshot.
        let unchanged = ds.path == snapshot.path
            && Arc::ptr_eq(&ds.saved_doc, &snapshot.saved)
            && *ds.doc == *snapshot.doc
            && ds.versions == snapshot.versions
            && ds.branches == snapshot.branches;
        if unchanged {
            ds.mark_saved();
            ds.revision += 1;
            branch_tabs_saved(s, uid);
            s.pending_file_saves.retain(|snapshot| snapshot.uid != uid || snapshot.revision > revision);
        }
        return Ok(json!({"saved": unchanged, "document": uid, "revision": revision}));
    }
    let i = crate::branches::file_tab(s);
    let ds = s.docs.get_mut(i).ok_or(EngineError::NoDocument)?;
    ds.mark_saved();
    ds.revision += 1;
    let uid = ds.uid;
    branch_tabs_saved(s, uid);
    Ok(json!({"saved": true}))
}

pub(super) fn save(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(path) = string(p, "path") {
        return save_to(s, Some(path), p.get("overwrite").and_then(Value::as_bool) == Some(true));
    }
    let ds = s.docs.get(crate::branches::file_tab(s)).ok_or(EngineError::NoDocument)?;
    match ds.path.clone() {
        // Earlier builds wrote native data under a .fig name: never overwrite that file with an export.
        Some(path) if ext(&path) == "fig" => {
            s.request_ui("dialog.saveAs", json!({}));
            Err(EngineError::Disabled("file.save".into(), "the saved path ends in .fig: choose Save as or pass `path`".into()))
        }
        Some(path) => {
            if p.get("overwrite").and_then(Value::as_bool) != Some(true) {
                refuse_overwriting_changes(ds, &path)?;
            }
            save_to(s, Some(path), p.get("overwrite").and_then(Value::as_bool) == Some(true))
        }
        None => {
            s.request_ui("dialog.saveAs", json!({}));
            Err(EngineError::Disabled("file.save".into(), "the file has no path yet: pass `path`".into()))
        }
    }
}

pub(super) fn save_as(s: &mut Session, p: &Value) -> Result<Value> {
    match string(p, "path") {
        Some(mut path) => {
            if ext(&path).is_empty() {
                path.push_str(".figcraft");
            }
            save_to(s, Some(path), false)
        }
        None if wants_data(p) => save_to(s, None, false),
        None => {
            s.request_ui("dialog.saveAs", json!({}));
            Err(EngineError::Disabled("file.saveAs".into(), "pass `path`".into()))
        }
    }
}
