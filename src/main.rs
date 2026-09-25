mod config;
mod handshake;
mod host;
mod memory;
mod settings;
mod world;

use host::method;
use wasm_bindgen::prelude::*;
use worker::{event, Context, Env, Request, Response, Socket};

fn main() {}

/// The world's Durable Object stub, from the declared `MinecraftWorld` export.
fn world(env: &Env) -> worker::Result<worker::worker_sys::DurableObject> {
    let name = env.var("WORLD_NAME")?.to_string();
    let namespace = host::EXPORTS.with(|exports| host::property(exports, "MinecraftWorld"))?;
    Ok(method(&namespace, "getByName", &[&JsValue::from(name)])?.unchecked_into())
}

/// Serves an inbound TCP connection. Server-list pings are answered from the
/// object's metadata without starting a server; a login handshake is piped to
/// the world's object until either side closes.
#[event(connect)]
async fn connect(mut socket: Socket, env: Env, _ctx: Context) -> worker::Result<()> {
    let world = world(&env)?;
    let rpc: JsValue = world.clone().into();
    let server_list = |protocol: u32| async move {
        let promise =
            method(&rpc, "serverList", &[&JsValue::from(protocol)]).map_err(js_message)?;
        let json = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
            .await
            .map_err(js_message)?;
        json.as_string()
            .ok_or_else(|| "serverList returned no text".to_string())
    };
    let consumed = match handshake::accept(&mut socket, server_list).await {
        Ok(handshake::Handshake::Login { consumed }) => consumed,
        // Invalid or interrupted pre-login traffic never starts a game runtime.
        Ok(handshake::Handshake::Status) => return Ok(()),
        Err(error) => {
            worker::console_debug!("Minecraft connection ended: {error}");
            return Ok(());
        }
    };
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"allowHalfOpen".into(), &JsValue::TRUE)?;
    let mut upstream = Socket::from(world.connect(&world::authority(), options.into())?);
    // Either side closing ends the connection; a disconnect is not an error
    // of the handler (which the `connect` event treats as fatal).
    if let Err(error) = tokio::io::AsyncWriteExt::write_all(&mut upstream, &consumed).await {
        worker::console_debug!("Minecraft connection ended: {error}");
        return Ok(());
    }
    let (mut client_read, mut client_write) = tokio::io::split(socket);
    let (mut server_read, mut server_write) = tokio::io::split(upstream);
    let _ = tokio::select! {
        r = tokio::io::copy(&mut client_read, &mut server_write) => r,
        r = tokio::io::copy(&mut server_read, &mut client_write) => r,
    };
    Ok(())
}

fn js_message(error: JsValue) -> String {
    js_sys::Error::from(error).message().into()
}

#[event(fetch)]
async fn fetch(request: Request, env: Env, _ctx: Context) -> worker::Result<Response> {
    match request.path().as_str() {
        "/health" => Response::ok(r#"{"ready":true}"#).map(json),
        "/" if request.method() == worker::Method::Get => {
            let rpc: JsValue = world(&env)?.into();
            let status = method(&rpc, "status", &[])?;
            let status =
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(status)).await?;
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
