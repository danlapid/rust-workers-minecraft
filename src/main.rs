mod config;
mod host;
mod memory;
mod world;

use host::{method, property, then};
use wasm_bindgen::prelude::*;
use worker::{event, Context, Env, Request, Response};

fn main() {}

/// The world's Durable Object stub.
fn stub(env: &Env) -> worker::Result<JsValue> {
    let name = env.var("WORLD_NAME")?.to_string();
    Ok(env.durable_object("WORLD")?.get_by_name(&name)?.into_rpc())
}

/// Pipes an inbound TCP connection to the world's object until either side
/// closes. worker-build exports it as the entrypoint's `connect` handler.
#[wasm_bindgen]
pub fn connect(socket: JsValue, env: Env, _ctx: JsValue) -> Result<JsValue, JsValue> {
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

#[event(fetch)]
async fn fetch(request: Request, env: Env, _ctx: Context) -> worker::Result<Response> {
    match request.path().as_str() {
        "/health" => Response::ok(r#"{"ready":true}"#).map(json),
        "/" if request.method() == worker::Method::Get => {
            let status = method(&stub(&env)?, "status", &[])?;
            let status = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(status)).await?;
            let body = js_sys::JSON::stringify(&status)?;
            Response::ok(String::from(body)).map(json)
        }
        _ => Response::error("Not found", 404),
    }
}

fn json(response: Response) -> Response {
    let headers = worker::Headers::new();
    let _ = headers.set("content-type", "application/json");
    response.with_headers(headers)
}
