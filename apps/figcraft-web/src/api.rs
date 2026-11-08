//! `window.figcraft`: the control channel for the page, as promises.
//!
//! The same JSON methods as the desktop control server (`docs/control-protocol.md`), so an agent
//! driving FigCraft in a browser tab (or an `<iframe>` in another app) uses the same commands as
//! the CLI and the MCP server:
//!
//! ```js
//! await figcraft.execute("node.create", {kind: "frame", name: "Home"})  // engine command → its result
//! await figcraft.request("document.inspect")                            // control method → its result
//! await figcraft.commands()                                             // command list
//! const {data} = await figcraft.screenshot()                            // the canvas as PNG (base64)
//! await figcraft.openFile("home.fig", bytes)                            // `file.open` with the bytes
//! figcraft.captureDownloads(true)                                       // saves stay in the page…
//! figcraft.files(); await figcraft.readFile("home.figcraft")            // …to be read back
//! ```
//!
//! Results resolve with the method's `result`; errors reject with an `Error` carrying `code`,
//! `started`, `mayHaveApplied` and `rejected`. `request`/`execute` take optional third options
//! `{timeoutMs, deadlineEpochMs}`; `openFile` takes them third too, `inspect`/`commands` first,
//! and `screenshot` second. The default wait is 30 seconds, at most ten minutes. Queued work is
//! cancelled under its admission gate; already-started work is uncertain and never retried.
//! A surviving same-origin embedding page (or opener) settles waits when this document ends.
//! Local deadlines remain live if an opener is lost; document destruction needs a surviving owner.
//! Page restore admits only new requests; old identities and cancelled work are never replayed.
//! `capabilities` describes the exact bounds. Direct `openFile` keeps the 64 MiB native limit;
//! encoded requests are bounded to 96 MiB, 32 pending calls and 128 MiB pending bytes in total.

use std::cell::{Cell, RefCell};

use base64::Engine as _;
use figcraft_ui_egui::ControlRequest;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::request_lifecycle::{self, Completion, Failure, MAX_OPEN_BYTES, MAX_REQUEST_BYTES, Requests};
use crate::saved_files::SavedFiles;

// Inline imports remain in the normal one-level wasm-bindgen snippets allowlist.
#[wasm_bindgen(inline_js = r#"
// Private WASM import. Native callbacks belong to their scheduler's realm; no eval is used.
export function createRequestOwner(poll, ended, resumed) {
    const win = window, doc = win.document, links = [];
    const parentOwner = (start, track) => {
        let current = start;
        for (;;) {
            try {
                const parent = current.parent;
                if (parent === current || parent.location.origin !== win.location.origin) break;
                const frame = current.frameElement;
                if (!frame) break;
                void parent.document;
                if (track) links.push({frame, doc: current.document});
                current = parent;
            } catch { break; }
        }
        return current;
    };
    let owner = parentOwner(win, true);
    if (owner === win) {
        try {
            const opener = win.opener;
            if (opener && !opener.closed && opener.location.origin === win.location.origin) {
                void opener.document;
                owner = parentOwner(opener, false);
            }
        } catch { /* A cross-origin or unavailable opener is not an owner. */ }
    }
    const sameDocument = (realm, expected) => {
        try { return !realm.closed && realm.document === expected; } catch { return false; }
    };
    let ownerDoc;
    try { ownerDoc = owner.document; } catch { owner = win; ownerDoc = doc; }
    const bridge = (realm, callback) => realm.Function.prototype.call.bind(callback, undefined);
    let ownerTimer = null, localTimer = null, observer = null, closed = false, disposed = false;
    const owned = () => {
        try { return !win.closed && win.document === doc && links.every(link => link.frame.isConnected && link.frame.contentDocument === link.doc); }
        catch { return false; }
    };
    const stop = () => {
        // WindowProxy methods may now target a replacement document with colliding numeric IDs.
        if (ownerTimer !== null) { if (sameDocument(owner, ownerDoc)) { try { owner.clearInterval(ownerTimer); } catch {} } ownerTimer = null; }
        if (localTimer !== null) { if (sameDocument(win, doc)) { try { win.clearInterval(localTimer); } catch {} } localTimer = null; }
        if (observer) { try { observer.disconnect(); } catch {} observer = null; }
    };
    const end = () => {
        if (closed || disposed) return;
        closed = true;
        stop();
        // Directly settle Rust's JS callbacks; this schedules no job in the ended document.
        ended();
    };
    const tick = () => { if (!owned()) end(); else poll(); };
    const localTick = bridge(win, tick);
    let check = null, ownerTick = null;
    try {
        check = bridge(owner, () => { if (!owned()) end(); });
        ownerTick = bridge(owner, tick);
    } catch { owner = win; ownerDoc = doc; }
    const hidden = bridge(win, end);
    const shown = bridge(win, event => {
        if (!disposed && event.persisted && owned()) { closed = false; resumed(); }
    });
    const dispose = () => {
        if (disposed) return;
        disposed = true;
        stop();
        try { win.removeEventListener('pagehide', hidden); } catch {}
        try { win.removeEventListener('pageshow', shown); } catch {}
    };
    try {
        win.addEventListener('pagehide', hidden);
        win.addEventListener('pageshow', shown);
    } catch (error) { dispose(); throw error; }
    return {
        start() {
            if (closed || disposed) return false;
            if (!owned()) { end(); return false; }
            if (ownerTimer !== null || localTimer !== null) return true;
            try {
                // The child-native fallback remains live if a popup's opener closes or navigates.
                localTimer = win.setInterval(localTick, 25);
                if (owner !== win && sameDocument(owner, ownerDoc)) {
                    try {
                        observer = new owner.MutationObserver(check);
                        observer.observe(ownerDoc, {childList: true, subtree: true});
                        ownerTimer = owner.setInterval(ownerTick, 25);
                    } catch { // Keep local deadlines available when the survivor was already lost.
                        if (observer) { try { observer.disconnect(); } catch {} observer = null; }
                    }
                }
            } catch (error) { stop(); throw error; }
            return true;
        },
        stop,
        dispose,
    };
}
export function startRequestOwner(owner) { return owner.start(); }
export function stopRequestOwner(owner) { owner.stop(); }
export function disposeRequestOwner(owner) { owner.dispose(); }
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = createRequestOwner)]
    fn create_request_owner(poll: &js_sys::Function, ended: &js_sys::Function, resumed: &js_sys::Function) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = startRequestOwner)]
    fn start_request_owner(owner: &JsValue) -> Result<bool, JsValue>;
    #[wasm_bindgen(catch, js_name = stopRequestOwner)]
    fn stop_request_owner(owner: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = disposeRequestOwner)]
    fn dispose_request_owner(owner: &JsValue) -> Result<(), JsValue>;
}

struct Callbacks {
    resolve: js_sys::Function,
    reject: js_sys::Function,
}

thread_local! {
    static REQUESTS: RefCell<Requests<Callbacks>> = RefCell::new(Requests::default());
    // At most one survivor timer and one local fallback, only while calls wait.
    static REQUEST_OWNER: RefCell<Option<JsValue>> = const { RefCell::new(None) };
    static CTX: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
    static CAPTURE: Cell<bool> = const { Cell::new(false) };
    static SAVED: RefCell<SavedFiles> = RefCell::new(SavedFiles::default());
}

pub fn connect(ctx: egui::Context) {
    CTX.with(|c| *c.borrow_mut() = Some(ctx));
}

/// The page's requests since the last frame, for the app's control queue.
pub fn take_requests() -> Vec<ControlRequest> {
    let (requests, completed) = REQUESTS.with(|requests| requests.borrow_mut().take_for_frame(js_sys::Date::now()));
    settle(completed);
    requests
}

/// Keep a save instead of downloading it, when the page asked for that. True when kept.
pub fn capture(name: &str, bytes: &[u8]) -> bool {
    if !CAPTURE.with(Cell::get) {
        return false;
    }
    SAVED.with(|s| s.borrow_mut().keep(name, bytes));
    true
}

fn to_js(v: &Value) -> JsValue {
    js_sys::JSON::parse(&v.to_string()).unwrap_or(JsValue::NULL)
}

fn from_js(v: &JsValue, limit: usize) -> Result<Value, Failure> {
    if v.is_undefined() {
        return Ok(json!({}));
    }
    let invalid = || Failure::admission("CONTROL_INVALID_REQUEST", "Parameters and options must be serializable JSON objects.");
    let encoded = js_sys::JSON::stringify(v).map_err(|_| invalid())?;
    if encoded.length() as usize > limit {
        return Err(Failure::admission("CONTROL_PAYLOAD_TOO_LARGE", "The page request exceeds its encoded byte limit."));
    }
    let encoded = encoded.as_string().ok_or_else(invalid)?;
    if encoded.len() > limit {
        return Err(Failure::admission("CONTROL_PAYLOAD_TOO_LARGE", "The page request exceeds its encoded byte limit."));
    }
    serde_json::from_str(&encoded).map_err(|_| invalid())
}

fn failure_js(failure: Failure) -> JsValue {
    let error = js_sys::Error::new(&failure.message);
    for (key, value) in [
        ("code", JsValue::from_str(failure.code)),
        ("started", JsValue::from_bool(failure.started)),
        ("mayHaveApplied", JsValue::from_bool(failure.may_have_applied)),
        ("rejected", JsValue::from_bool(failure.rejected)),
    ] {
        let _ = js_sys::Reflect::set(&error, &key.into(), &value);
    }
    error.into()
}

fn stop_idle_timer() {
    if !REQUESTS.with(|requests| requests.borrow().has_pending()) {
        REQUEST_OWNER.with(|slot| {
            if let Some(owner) = slot.borrow().as_ref() {
                let _ = stop_request_owner(owner);
            }
        });
    }
}

fn ensure_timer() -> Result<(), Failure> {
    REQUEST_OWNER.with(|slot| {
        let owner = slot.borrow();
        let owner = owner.as_ref().ok_or_else(|| Failure::admission("CONTROL_TRANSPORT_UNAVAILABLE", "The page request owner is unavailable."))?;
        match start_request_owner(owner) {
            Ok(true) => Ok(()),
            Ok(false) => Err(Failure::owner_closed()),
            Err(_) => Err(Failure::admission("CONTROL_TRANSPORT_UNAVAILABLE", "The browser could not schedule the owned request deadline.")),
        }
    })
}

fn owner_ended() {
    let completed = REQUESTS.with(|requests| requests.borrow_mut().close());
    // This is a direct synchronous JS rejection, not a child-realm Rust future/microtask.
    settle(completed);
}

fn owner_resumed() {
    REQUESTS.with(|requests| requests.borrow_mut().reopen());
    CTX.with(|ctx| {
        if let Some(ctx) = ctx.borrow().as_ref() {
            ctx.request_repaint();
        }
    });
}

fn dispose_owner() {
    let owner = REQUEST_OWNER.with(|slot| slot.borrow_mut().take());
    if let Some(owner) = owner {
        let _ = dispose_request_owner(&owner);
    }
}

fn install_owner() -> Result<(), JsValue> {
    let poll = Closure::<dyn FnMut()>::new(poll_replies).into_js_value();
    let ended = Closure::<dyn FnMut()>::new(owner_ended).into_js_value();
    let resumed = Closure::<dyn FnMut()>::new(owner_resumed).into_js_value();
    let owner = create_request_owner(poll.unchecked_ref(), ended.unchecked_ref(), resumed.unchecked_ref())?;
    REQUEST_OWNER.with(|slot| *slot.borrow_mut() = Some(owner));
    Ok(())
}

/// Settle answered or expired requests, from both animation frames and the bounded wait timer.
pub fn poll_replies() {
    let completed = REQUESTS.with(|requests| requests.borrow_mut().poll(js_sys::Date::now()));
    settle(completed);
}

fn settle(completed: Vec<Completion<Callbacks>>) {
    // JavaScript callbacks can synchronously call this API again. Release the queue borrow first.
    for completion in completed {
        match completion.result {
            Ok(value) => {
                let _ = completion.context.resolve.call1(&JsValue::NULL, &to_js(&value));
            }
            Err(error) => {
                let _ = completion.context.reject.call1(&JsValue::NULL, &failure_js(error));
            }
        }
    }
    stop_idle_timer();
}

fn request(method: &str, params: Value, options: Value) -> js_sys::Promise {
    if let Err(error) = REQUESTS.with(|requests| requests.borrow().ensure_open()) {
        return js_sys::Promise::reject(&failure_js(error));
    }
    if let Err(error) = ensure_timer() {
        return js_sys::Promise::reject(&failure_js(error));
    }
    let mut owned = Some(params);
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let Some(params) = owned.take() else {
            let _ = reject.call1(&JsValue::NULL, &failure_js(Failure::admission("CONTROL_INVALID_REQUEST", "A request can be queued only once.")));
            return;
        };
        let result = REQUESTS.with(|requests| {
            requests.borrow_mut().enqueue(method, params, &options, js_sys::Date::now(), Callbacks { resolve, reject: reject.clone() })
        });
        if let Err(error) = result {
            let _ = reject.call1(&JsValue::NULL, &failure_js(error));
        }
    });
    stop_idle_timer();
    CTX.with(|c| {
        if let Some(ctx) = c.borrow().as_ref() {
            ctx.request_repaint();
        }
    });
    promise
}

fn request_js(method: &str, params: &JsValue, options: &JsValue) -> js_sys::Promise {
    match from_js(params, MAX_REQUEST_BYTES).and_then(|params| from_js(options, 4096).map(|options| (params, options))) {
        Ok((params, options)) => request(method, params, options),
        Err(error) => js_sys::Promise::reject(&failure_js(error)),
    }
}

fn set_fn(obj: &js_sys::Object, name: &str, f: JsValue) -> Result<(), JsValue> {
    js_sys::Reflect::set(obj, &name.into(), &f).map(|_| ())
}

/// Install `window.figcraft`.
pub fn install() -> Result<(), JsValue> {
    let obj = js_sys::Object::new();
    set_fn(
        &obj,
        "request",
        Closure::<dyn Fn(String, JsValue, JsValue) -> js_sys::Promise>::new(|method: String, params: JsValue, options: JsValue| {
            request_js(&method, &params, &options)
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "execute",
        Closure::<dyn Fn(String, JsValue, JsValue) -> js_sys::Promise>::new(|command: String, params: JsValue, options: JsValue| {
            match from_js(&params, MAX_REQUEST_BYTES).and_then(|params| from_js(&options, 4096).map(|options| (params, options))) {
                Ok((params, options)) if params.is_object() => request("engine.execute", json!({"command": command, "params": params}), options),
                Ok(_) => js_sys::Promise::reject(&failure_js(Failure::admission("CONTROL_INVALID_REQUEST", "Command parameters must be an object."))),
                Err(error) => js_sys::Promise::reject(&failure_js(error)),
            }
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "commands",
        Closure::<dyn Fn(JsValue) -> js_sys::Promise>::new(|options: JsValue| request_js("engine.commands", &JsValue::UNDEFINED, &options))
            .into_js_value(),
    )?;
    set_fn(
        &obj,
        "inspect",
        Closure::<dyn Fn(JsValue) -> js_sys::Promise>::new(|options: JsValue| request_js("document.inspect", &JsValue::UNDEFINED, &options))
            .into_js_value(),
    )?;
    set_fn(
        &obj,
        "screenshot",
        Closure::<dyn Fn(JsValue, JsValue) -> js_sys::Promise>::new(|params: JsValue, options: JsValue| {
            request_js("ui.screenshot", &params, &options)
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "openFile",
        Closure::<dyn Fn(String, js_sys::Uint8Array, JsValue) -> js_sys::Promise>::new(
            |name: String, bytes: js_sys::Uint8Array, options: JsValue| {
                // Refuse before copying/encoding an oversized file. The encoded envelope is also
                // checked against the request and aggregate budgets by the common queue.
                if bytes.length() as usize > MAX_OPEN_BYTES {
                    return js_sys::Promise::reject(&failure_js(Failure::admission(
                        "CONTROL_PAYLOAD_TOO_LARGE",
                        "Native file opening is limited to 64 MiB before base64 encoding.",
                    )));
                }
                let options =
                    match from_js(&options, 4096).and_then(|options| request_lifecycle::timeout(&options, js_sys::Date::now()).map(|_| options)) {
                        Ok(options) => options,
                        Err(error) => return js_sys::Promise::reject(&failure_js(error)),
                    };
                let data = base64::engine::general_purpose::STANDARD.encode(bytes.to_vec());
                request("engine.execute", json!({"command": "file.open", "params": {"name": name, "data": data}}), options)
            },
        )
        .into_js_value(),
    )?;
    js_sys::Reflect::set(&obj, &"capabilities".into(), &to_js(&request_lifecycle::capabilities()))?;
    set_fn(&obj, "captureDownloads", Closure::<dyn Fn(bool)>::new(|on: bool| CAPTURE.with(|c| c.set(on))).into_js_value())?;
    set_fn(
        &obj,
        "files",
        Closure::<dyn Fn() -> JsValue>::new(|| {
            SAVED.with(|s| to_js(&Value::Array(s.borrow().list().map(|(n, size)| json!({"path": n, "size": size})).collect())))
        })
        .into_js_value(),
    )?;
    set_fn(
        &obj,
        "readFile",
        Closure::<dyn Fn(String) -> Result<JsValue, JsValue>>::new(|name: String| {
            SAVED.with(|s| {
                s.borrow()
                    .get(&name)
                    .map(|b| js_sys::Uint8Array::from(b).into())
                    .ok_or_else(|| js_sys::Error::new(&format!("{name}: not found")).into())
            })
        })
        .into_js_value(),
    )?;
    let w = web_sys::window().ok_or("no window")?;
    install_owner()?;
    match js_sys::Reflect::set(&w, &"figcraft".into(), &obj) {
        Ok(true) => Ok(()),
        result => {
            // A failing setter may have observed the object and even queued a call.
            owner_ended();
            dispose_owner();
            match result {
                Err(error) => Err(error),
                Ok(_) => Err(js_sys::Error::new("The browser refused to publish the FigCraft API.").into()),
            }
        }
    }
}
