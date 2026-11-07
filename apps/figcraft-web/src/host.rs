//! Host modes: a page that runs FigCraft (as an `<iframe>`, or in its own document) opens
//! documents in it and stores what it saves instead of the browser downloading. The messages
//! and the two transports are `craft-web-host`'s (storytold/craft-libs): the same contract as
//! the other Craft apps' bridges. This module is FigCraft's part of it.
//!
//! - `?host=parent` (an `<iframe>`, such as a Nextcloud app) talks to the same-origin parent over
//!   `postMessage`; `data-host="page"` on the canvas (the page's own document) uses DOM
//!   `CustomEvent`s on the canvas instead. `?host=parent` wins when both are present.
//! - `figcraft:ready` (to the host) once the app is running and listening; send a file now.
//! - `figcraft:open` (from the host): `{ name, bytes }`. It opens exactly as a file picked with
//!   File › Open or dropped on the window would (the app's own open queue, with its "Opening as
//!   a new file" card). A payload over the browser's own open limit
//!   ([`figcraft_ui_egui::browser_can_read`]'s 512 MB) is refused before its bytes are copied,
//!   and anything malformed is ignored. FigCraft reads whole files, so it says `file: false` in
//!   `ready` and a host sends `bytes`.
//! - `figcraft:save` (to the host): `{ name, saveAs, bytes }` for every file the app hands its
//!   download service; in host mode nothing reaches the browser's downloads. `saveAs` says whether
//!   the file becomes the document's file ([`crate::hostmode::save_as`]):
//!   - A Save of the file the host opened, under its own name: `false`.
//!   - A document file (`.figcraft` or `.json`, the formats `file.save` and `file.saveAs` write)
//!     under a name the host didn't open — Save As, or a Save of a document the host never sent
//!     (an untitled one): `true`. It becomes the document's file; the next Save sends its name
//!     with `false`. Names compare without case.
//!   - Everything else is a copy or export that never becomes the document: `false` — rendered
//!     images, SVG and PDF, `.fig` copies, and the variables exports (a `.variables.json`
//!     sidecar's `.json` extension doesn't make it a document).
//! - `figcraft:dirty` whenever unsaved changes appear or go away, `figcraft:failed` when the app
//!   couldn't start.
//!
//! In host mode the app starts with no document (the home screen) and the host sends the file to
//! edit; `?author=` names the user for comments and board notes.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use craft_web_host::web::{FileData, Host, Message};
use figcraft_ui_egui::{DocumentSave, FigApp, SaveDelivery};

/// The app id in the host messages (`figcraft:ready`).
const APP: &str = "figcraft";

/// The longest user name taken from `?author=`.
const MAX_AUTHOR_CHARS: usize = 200;

/// Files the host sent, on their way to the app's own open queue ([`figcraft_ui_egui::pending_open`]):
/// `(name, bytes)`.
pub type OpenQueue = Rc<RefCell<Vec<(String, Vec<u8>)>>>;

#[derive(Clone)]
pub struct Bridge {
    host: Rc<Host>,
    /// The file the host opened, and after a `saveAs` hand-over the document's file: the name a
    /// Save under (`saveAs: false`) is decided against.
    host_file: Rc<RefCell<Option<String>>>,
    acknowledged: bool,
    pending: Rc<RefCell<BTreeMap<String, DocumentSave>>>,
    saved: Rc<RefCell<Vec<DocumentSave>>>,
}

impl Bridge {
    /// The bridge for this page, or `None` when FigCraft isn't hosted: no `?host=parent` and no
    /// `data-host="page"` on the canvas, an unframed `?host=parent` page (it stays the normal app
    /// with downloads), or a parent on another origin.
    pub fn connect(canvas: &web_sys::HtmlCanvasElement) -> Option<Self> {
        let acknowledged = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .and_then(|q| web_sys::UrlSearchParams::new_with_str(&q).ok())
            .and_then(|q| q.get("ack"))
            .is_some_and(|v| v == "1");
        Some(Self {
            host: Rc::new(Host::connect(APP, canvas)?),
            host_file: Rc::default(),
            acknowledged,
            pending: Rc::default(),
            saved: Rc::default(),
        })
    }

    /// Hear the files the host sends, and queue each one on `opens` for the app's own open path
    /// (the shell drains it into [`figcraft_ui_egui::pending_open`] every frame, so a host file
    /// opens like a dropped one).
    pub fn listen(&self, opens: OpenQueue, ctx: egui::Context) {
        let host_file = self.host_file.clone();
        let pending = self.pending.clone();
        let saved = self.saved.clone();
        self.host.listen(move |message| match message {
            Message::Open { name, data } => deliver_open(&opens, &ctx, &host_file, &name, &data),
            Message::Saved { id } => {
                if let Some(snapshot) = pending.borrow_mut().remove(&id) {
                    saved.borrow_mut().push(snapshot);
                    ctx.request_repaint();
                }
            }
            _ => {}
        });
    }

    /// `?author=`: the host's name for the user signs comments and board notes.
    pub fn set_author(app: &mut FigApp, author: &str) {
        let author: String = author.trim().chars().take(MAX_AUTHOR_CHARS).collect();
        if !author.is_empty() {
            app.session.prefs.user_name = author;
        }
    }

    pub fn post_ready(&self) {
        let _ = self.host.post_ready(env!("CARGO_PKG_VERSION"), false);
    }

    /// The app couldn't start (no WebGPU or WebGL2, for example): let the host say so.
    pub fn post_failed(&self, error: &str) {
        let _ = self.host.post_failed(error, None);
    }

    /// The document file the host opened or the last Save As made, if any.
    pub fn host_file(&self) -> Option<String> {
        // A single global filename cannot represent several documents. The acknowledged host
        // maps the explicit document UID; legacy hosts retain their old filename convention.
        if self.acknowledged {
            return None;
        }
        self.host_file.borrow().clone()
    }

    pub fn acknowledged(&self) -> bool {
        self.acknowledged
    }

    pub fn take_saved(&self) -> Vec<DocumentSave> {
        std::mem::take(&mut *self.saved.borrow_mut())
    }

    pub fn retain_pending(&self, pending: &[(u64, u64)]) {
        self.pending.borrow_mut().retain(|_, snapshot| pending.contains(&(snapshot.document, snapshot.revision)));
        self.saved.borrow_mut().retain(|snapshot| pending.contains(&(snapshot.document, snapshot.revision)));
    }

    /// Local candidate protocol: persist these exact bytes for this document, then acknowledge
    /// the opaque `id` with `figcraft:saved`. Posting a message alone never marks the file saved.
    pub fn post_document(&self, snapshot: DocumentSave, path: &str, bytes: &[u8]) -> SaveDelivery {
        if self.pending.borrow().len() + self.saved.borrow().len() >= 32 {
            return SaveDelivery::HostFailed;
        }
        let mut nonce = [0u8; 16];
        let random = web_sys::window().and_then(|w| w.crypto().ok()).is_some_and(|c| c.get_random_values_with_u8_array(&mut nonce).is_ok());
        if !random {
            return SaveDelivery::HostFailed;
        }
        let nonce: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
        let id = format!("fc-local-save-v1:{}:{}:{nonce}", snapshot.document, snapshot.revision);
        self.pending.borrow_mut().insert(id.clone(), snapshot);
        match self.host.post_save(&file_name(path), bytes, Some(&id), snapshot.save_as) {
            Ok(()) => SaveDelivery::Pending,
            Err(_) => {
                self.pending.borrow_mut().remove(&id);
                SaveDelivery::HostFailed
            }
        }
    }

    /// Hand a file the app would otherwise download to the host; `false` when nothing was
    /// delivered. The `saveAs` rule is this module's documentation's.
    pub fn post_save(&self, path: &str, bytes: &[u8]) -> bool {
        let name = file_name(path);
        let host_file = self.host_file.borrow().clone();
        let save_as = crate::hostmode::save_as(&name, host_file.as_deref());
        let sent = self.host.post_save(&name, bytes, None, save_as).is_ok();
        // After a Save As hand-over the host has the new file: the next Save of it sends its name
        // with `saveAs: false`. A failed send changes nothing, so a retry is still a Save As.
        if sent && save_as {
            *self.host_file.borrow_mut() = Some(name);
        }
        sent
    }

    /// Report unsaved changes when they appear or go away, not every frame (the crate does not
    /// repeat a state it already reported).
    pub fn set_dirty(&self, dirty: bool) {
        self.host.post_dirty(dirty);
    }
}

/// The file an open message carries, queued for the app's own open path, or nothing when the
/// payload isn't one (a `File` the app doesn't read, or over the open limit).
fn deliver_open(opens: &OpenQueue, ctx: &egui::Context, host_file: &RefCell<Option<String>>, name: &str, data: &FileData) {
    let name = crate::hostmode::open_name(name);
    let FileData::Bytes(array) = data else {
        log::warn!("host file {name} came as a File; FigCraft reads bytes only; not opened");
        return;
    };
    if figcraft_ui_egui::browser_can_read(&name, data.size()).is_err() {
        log::warn!("host file {name} is too large to open in the browser; not opened");
        return;
    }
    // The document's file is now the one the host sent: a Save of it under this name is
    // `saveAs: false`.
    *host_file.borrow_mut() = Some(name.clone());
    opens.borrow_mut().push((name, array.to_vec()));
    ctx.request_repaint();
}

/// The last component of `path`, or the whole of it when there is none.
fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}
