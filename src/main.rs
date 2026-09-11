// `#[wasm_bindgen(jspi)]` is experimental and warns as deprecated.
#![allow(deprecated)]

mod config;
mod memory;

use pumpkin::{data::VanillaData, server::Server, PumpkinServer};
use std::{cell::RefCell, sync::atomic::Ordering, sync::Arc};
use wasm_bindgen::prelude::*;

fn main() {}

thread_local! {
    static FAILURE: RefCell<Option<String>> = const { RefCell::new(None) };
    static SERVER: RefCell<Option<Arc<Server>>> = const { RefCell::new(None) };
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&error.to_string()).into()
}

fn server() -> Result<Arc<Server>, JsValue> {
    if let Some(error) = FAILURE.with(|failure| failure.borrow().clone()) {
        return Err(js_error(error));
    }
    SERVER
        .with(|server| server.borrow().clone())
        .ok_or_else(|| js_error("Server is not running"))
}

/// Runs the server on a Tokio runtime that parks by suspending this activation.
/// `ready` is called once the listener is bound. Resolves after `pumpkin_stop`
/// once the final save completes.
#[wasm_bindgen(jspi)]
pub fn pumpkin_run(ready: js_sys::Function) -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(|info| {
        FAILURE.with(|failure| {
            failure.borrow_mut().get_or_insert_with(|| info.to_string());
        });
        eprintln!("RUST PANIC: {info}");
    }));
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .use_current_thread()
        .build_global()
        .map_err(js_error)?;
    let (basic, advanced) = config::configuration();
    pumpkin::init_logger(&advanced);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(js_error)?;
    runtime.block_on(async {
        let server = PumpkinServer::new(basic, advanced, VanillaData::load()).await;
        server.init_plugins().await;
        SERVER.with(|slot| *slot.borrow_mut() = Some(server.server.clone()));
        ready.call0(&JsValue::UNDEFINED)?;
        server.start().await;
        SERVER.with(|slot| slot.borrow_mut().take());
        Ok(())
    })
}

/// Requests shutdown; `pumpkin_run` completes the save and returns.
#[wasm_bindgen]
pub fn pumpkin_stop() {
    pumpkin::stop_server();
}

#[wasm_bindgen]
pub fn pumpkin_status() -> Result<JsValue, JsValue> {
    let server = server()?;
    let result = js_sys::Object::new();
    js_sys::Reflect::set(
        &result,
        &"players".into(),
        &(server.get_all_players().len() as f64).into(),
    )?;
    js_sys::Reflect::set(
        &result,
        &"ticks".into(),
        &(server.tick_count.load(Ordering::Relaxed) as f64).into(),
    )?;
    js_sys::Reflect::set(
        &result,
        &"wasm_memory_bytes".into(),
        &((core::arch::wasm32::memory_size::<0>() * 65536) as f64).into(),
    )?;
    for (name, bytes) in memory::heap_usage() {
        js_sys::Reflect::set(&result, &name.into(), &(bytes as f64).into())?;
    }
    Ok(result.into())
}
