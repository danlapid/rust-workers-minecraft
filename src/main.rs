mod config;
mod memory;

use pumpkin::{data::VanillaData, PumpkinServer};
use std::{cell::RefCell, sync::atomic::Ordering, sync::Arc};
use wasm_bindgen::prelude::*;
use worker::emscripten::{future_to_promise, js_error, socket_from_value};

fn main() {}

thread_local! {
    static FAILURE: RefCell<Option<String>> = const { RefCell::new(None) };
    static SERVER: RefCell<Option<Arc<PumpkinServer>>> = const { RefCell::new(None) };
    static NEXT_CLIENT: RefCell<u64> = const { RefCell::new(0) };
}

fn server() -> Result<Arc<PumpkinServer>, JsValue> {
    if let Some(error) = FAILURE.with(|failure| failure.borrow().clone()) {
        return Err(js_error(error));
    }
    SERVER
        .with(|server| server.borrow().clone())
        .ok_or_else(|| js_error("Server is not started"))
}

#[wasm_bindgen]
pub fn pumpkin_start(settings: String) -> js_sys::Promise {
    future_to_promise(async move {
        if SERVER.with(|server| server.borrow().is_some()) {
            return Ok(JsValue::UNDEFINED);
        }
        std::panic::set_hook(Box::new(|info| {
            FAILURE.with(|failure| {
                failure.borrow_mut().get_or_insert_with(|| info.to_string());
            });
            eprintln!("RUST PANIC: {info}");
        }));
        let (basic, advanced) = config::configuration(&settings).map_err(js_error)?;
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .use_current_thread()
            .build_global()
            .map_err(js_error)?;
        pumpkin::init_logger(&advanced);
        let server = Arc::new(PumpkinServer::new(basic, advanced, VanillaData::load()).await);
        server.init_plugins().await;
        SERVER.with(|slot| *slot.borrow_mut() = Some(server));
        Ok(JsValue::UNDEFINED)
    })
}

#[wasm_bindgen]
pub fn pumpkin_connect(raw: JsValue) -> js_sys::Promise {
    future_to_promise(async move {
        let server = server()?;
        let socket = socket_from_value(raw)?;
        let id = NEXT_CLIENT.with(|slot| {
            let mut id = slot.borrow_mut();
            *id += 1;
            *id
        });
        // The SDK's socket is the injected Tokio byte stream. Its adapter handles
        // backpressure, EOF and JS stream errors; no emulated TCP listener is used.
        server
            .serve_connection(socket, ([127, 0, 0, 1], 0).into(), id)
            .await;
        Ok(JsValue::UNDEFINED)
    })
}

#[wasm_bindgen]
pub fn pumpkin_status() -> Result<JsValue, JsValue> {
    let server = server()?;
    let result = js_sys::Object::new();
    js_sys::Reflect::set(
        &result,
        &"players".into(),
        &(server.server.get_all_players().len() as f64).into(),
    )?;
    js_sys::Reflect::set(
        &result,
        &"ticks".into(),
        &(server.server.tick_count.load(Ordering::Relaxed) as f64).into(),
    )?;
    js_sys::Reflect::set(
        &result,
        &"wasm_memory_bytes".into(),
        &((core::arch::wasm32::memory_size::<0>() * 65536) as f64).into(),
    )?;
    js_sys::Reflect::set(
        &result,
        &"mean_tick_ms".into(),
        &server.server.get_mspt().into(),
    )?;
    for (name, bytes) in memory::heap_usage() {
        js_sys::Reflect::set(&result, &name.into(), &(bytes as f64).into())?;
    }
    Ok(result.into())
}

#[wasm_bindgen]
pub fn pumpkin_save() -> js_sys::Promise {
    future_to_promise(async {
        let server = server()?;
        if !server.server.get_all_players().is_empty() {
            return Err(js_error(
                "Disconnect clients before checkpointing the world",
            ));
        }
        server.server.save_all().await.map_err(js_error)?;
        for world in server.server.worlds.load_full().iter() {
            world.level.checkpoint().await.map_err(js_error)?;
        }
        // Background generation may catch a panic while the writer is draining.
        crate::server()?;
        Ok(JsValue::UNDEFINED)
    })
}

#[wasm_bindgen]
pub fn pumpkin_shutdown() -> js_sys::Promise {
    future_to_promise(async {
        let server = server()?;
        if !server.server.get_all_players().is_empty() {
            return Err(js_error(
                "Disconnect clients before shutting down the world",
            ));
        }
        pumpkin::stop_server();
        server.server.save_all().await.map_err(js_error)?;
        server.server.shutdown().await;
        SERVER.with(|slot| slot.borrow_mut().take());
        Ok(JsValue::UNDEFINED)
    })
}
