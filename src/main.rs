mod config;
mod host;
mod memory;
mod persist;
mod world;

use host::{method, property, then};
use wasm_bindgen::prelude::*;

fn main() {}

fn stub(env: &JsValue) -> Result<JsValue, JsValue> {
    let name = property(env, "WORLD_NAME")?;
    let namespace = property(env, "WORLD")?;
    method(&namespace, "getByName", &[&name])
}

/// Pipes an inbound TCP connection to the world's object until either side closes.
#[wasm_bindgen]
pub fn connect(socket: JsValue, env: JsValue, _ctx: JsValue) -> Result<JsValue, JsValue> {
    let target = host::stub_connect(&stub(&env)?, &world::authority())?;
    // Read/write failures are observed by the pumps; a normal peer disconnect
    // must not surface as an unhandled rejection of the lifetime promise.
    let swallow = Closure::<dyn FnMut(JsValue)>::new(|_| ()).into_js_value();
    for side in [&socket, &target] {
        then(&property(side, "closed")?, None, Some(&swallow))?;
    }
    let abort = web_sys::AbortController::new()?;
    let pipe = |from: &JsValue, to: &JsValue| -> Result<JsValue, JsValue> {
        let readable: web_sys::ReadableStream = property(from, "readable")?.unchecked_into();
        let writable: web_sys::WritableStream = property(to, "writable")?.unchecked_into();
        let options = web_sys::StreamPipeOptions::new();
        options.set_signal(&abort.signal());
        let abort = abort.clone();
        let on_error = Closure::once_into_js(move |error: JsValue| -> Result<JsValue, JsValue> {
            abort.abort();
            Err(error)
        });
        then(&readable.pipe_to_with_options(&writable, &options), None, Some(&on_error))
    };
    let pumps = js_sys::Array::of2(&pipe(&socket, &target)?, &pipe(&target, &socket)?);
    let close_both = Closure::once_into_js(move |results: JsValue| -> Result<JsValue, JsValue> {
        for result in js_sys::Array::from(&results).iter() {
            if property(&result, "status")? != "rejected" {
                continue;
            }
            let reason = property(&result, "reason")?;
            let text = reason
                .as_string()
                .unwrap_or_else(|| js_sys::Error::from(reason.clone()).message().into())
                .to_lowercase();
            let expected = ["closed", "closing", "abort", "cancel", "reset", "network connection lost"];
            if !expected.iter().any(|e| text.contains(e)) {
                web_sys::console::error_2(&"TCP forwarding failed".into(), &reason);
            }
        }
        let closes = js_sys::Array::new();
        for side in [&socket, &target] {
            closes.push(&method(side, "close", &[])?);
        }
        Ok(js_sys::Promise::all_settled(&closes).into())
    });
    then(&js_sys::Promise::all_settled(&pumps).into(), Some(&close_both), None)
}

fn json_response(value: &JsValue, status: u16) -> Result<JsValue, JsValue> {
    let init = web_sys::ResponseInit::new();
    init.set_status(status);
    let headers = web_sys::Headers::new()?;
    headers.set("content-type", "application/json")?;
    init.set_headers(&headers);
    let body = js_sys::JSON::stringify(value)?;
    Ok(web_sys::Response::new_with_opt_str_and_init(body.as_string().as_deref(), &init)?.into())
}

#[wasm_bindgen]
pub fn fetch(request: web_sys::Request, env: JsValue, _ctx: JsValue) -> Result<JsValue, JsValue> {
    let url = web_sys::Url::new(&request.url())?;
    match url.pathname().as_str() {
        "/health" => {
            let body = js_sys::Object::new();
            js_sys::Reflect::set(&body, &"ready".into(), &JsValue::TRUE)?;
            json_response(&body, 200)
        }
        "/" if request.method() == "GET" => {
            let status = method(&stub(&env)?, "status", &[])?;
            let respond = Closure::once_into_js(|value: JsValue| json_response(&value, 200));
            then(&status, Some(&respond), None)
        }
        _ => json_response(&"Not found".into(), 404),
    }
}
