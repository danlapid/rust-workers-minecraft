//! Platform calls, each returning the platform's own Promise or value.

use wasm_bindgen::prelude::*;

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

/// `storage.sql.exec(query, ...bindings)` with the rows read out eagerly as
/// arrays of column values.
pub fn sql(storage: &JsValue, query: &str, bindings: &[JsValue]) -> Result<Vec<Vec<JsValue>>, JsValue> {
    let sql = property(storage, "sql")?;
    let exec: js_sys::Function = property(&sql, "exec")?.unchecked_into();
    let args = js_sys::Array::new();
    args.push(&query.into());
    for binding in bindings {
        args.push(binding);
    }
    let cursor = js_sys::Reflect::apply(&exec, &sql, &args)?;
    let raw = method(&cursor, "raw", &[])?;
    let rows = js_sys::Array::from(&method(&raw, "toArray", &[])?);
    Ok(rows.iter().map(|row| js_sys::Array::from(&row).to_vec()).collect())
}
