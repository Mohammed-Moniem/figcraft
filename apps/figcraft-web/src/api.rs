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

struct Callbacks {
    resolve: js_sys::Function,
    reject: js_sys::Function,
}

thread_local! {
    static REQUESTS: RefCell<Requests<Callbacks>> = RefCell::new(Requests::default());
    // One reusable callback and at most one interval, active only while requests wait. Completing
    // a request does not leave a timeout closure behind until its original deadline.
    static TIMER_CALLBACK: RefCell<Option<Closure<dyn FnMut()>>> = const { RefCell::new(None) };
    static TIMER_ID: Cell<Option<i32>> = const { Cell::new(None) };
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
        TIMER_ID.with(|timer| {
            if let (Some(id), Some(window)) = (timer.take(), web_sys::window()) {
                window.clear_interval_with_handle(id);
            }
        });
    }
}

fn ensure_timer() -> Result<(), Failure> {
    if TIMER_ID.with(Cell::get).is_some() {
        return Ok(());
    }
    let window = web_sys::window().ok_or_else(|| Failure::admission("CONTROL_TRANSPORT_UNAVAILABLE", "The browser window is unavailable."))?;
    TIMER_CALLBACK.with(|slot| {
        let mut callback = slot.borrow_mut();
        if callback.is_none() {
            *callback = Some(Closure::wrap(Box::new(poll_replies) as Box<dyn FnMut()>));
        }
        let Some(callback) = callback.as_ref() else {
            return Err(Failure::admission("CONTROL_TRANSPORT_UNAVAILABLE", "The request timer is unavailable."));
        };
        let id = window
            .set_interval_with_callback_and_timeout_and_arguments_0(callback.as_ref().unchecked_ref(), 25)
            .map_err(|_| Failure::admission("CONTROL_TRANSPORT_UNAVAILABLE", "The browser could not schedule the request deadline."))?;
        TIMER_ID.with(|timer| timer.set(Some(id)));
        Ok(())
    })
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
    js_sys::Reflect::set(&w, &"figcraft".into(), &obj)?;
    Ok(())
}
