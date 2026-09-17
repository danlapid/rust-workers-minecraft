//! Platform calls, each returning the platform's own Promise or value.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/js/mount.js")]
extern "C" {
    /// Mounts the object's SQLite storage as the world's filesystem and routes
    /// NODERAWFS through it; `ROOT` is the mount path.
    #[wasm_bindgen(js_name = mountStorage)]
    pub fn mount_storage(storage: &JsValue);
    #[wasm_bindgen(js_name = ROOT, thread_local_v2)]
    pub static MOUNT_ROOT: JsValue;
}

#[wasm_bindgen(module = "cloudflare:node")]
extern "C" {
    /// Routes an inbound socket to the `net.Server` listening on its local
    /// port within the current Durable Object's port table; resolves when the
    /// connection closes.
    #[wasm_bindgen(js_name = handleAsNodeConnection, catch)]
    pub fn handle_as_node_connection(socket: &JsValue) -> Result<JsValue, JsValue>;
}

pub fn property(target: &JsValue, name: &str) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(target, &name.into())
}

pub fn method(target: &JsValue, name: &str, args: &[&JsValue]) -> Result<JsValue, JsValue> {
    let function: js_sys::Function = property(target, name)?.unchecked_into();
    let array = js_sys::Array::new();
    for arg in args {
        array.push(arg);
    }
    js_sys::Reflect::apply(&function, target, &array)
}

/// `promise.then(on_fulfilled, on_rejected)` on any thenable; the callbacks
/// are one-shot closures already converted with `Closure::once_into_js`.
pub fn then(
    promise: &JsValue,
    on_fulfilled: Option<&JsValue>,
    on_rejected: Option<&JsValue>,
) -> Result<JsValue, JsValue> {
    let then: js_sys::Function = property(promise, "then")?.unchecked_into();
    then.call2(
        promise,
        on_fulfilled.unwrap_or(&JsValue::UNDEFINED),
        on_rejected.unwrap_or(&JsValue::UNDEFINED),
    )
}

/// `stub.connect(authority, { allowHalfOpen: true })`.
pub fn stub_connect(stub: &JsValue, authority: &str) -> Result<JsValue, JsValue> {
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"allowHalfOpen".into(), &JsValue::TRUE)?;
    method(stub, "connect", &[&authority.into(), &options])
}

pub fn js_error(error: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&error.to_string()).into()
}

