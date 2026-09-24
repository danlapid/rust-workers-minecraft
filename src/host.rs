//! Platform calls, each returning the platform's own Promise or value.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/js/mount.js")]
extern "C" {
    /// Mounts the object's SQLite storage as the world's filesystem and routes
    /// NODERAWFS through it; `ROOT` is the mount path.
    #[wasm_bindgen(js_name = mountStorage)]
    pub fn mount_storage(storage: &worker::worker_sys::DurableObjectStorage);
    #[wasm_bindgen(js_name = ROOT, thread_local_v2)]
    pub static MOUNT_ROOT: JsValue;
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
pub fn js_error(error: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&error.to_string()).into()
}
