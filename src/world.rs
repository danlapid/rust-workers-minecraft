//! The Durable Object hosting one Pumpkin server.
//!
//! The object owns an `EventLoopRuntime`: Tokio's current-thread scheduler and
//! drivers with the host event loop as its wait. Work is scheduled as roots
//! that complete by callback, so every export returns a Promise and nothing
//! blocks or suspends. The server lifetime is one such root: the first
//! `connect` after idle schedules it, and its completion (after the final save)
//! is the checkpoint. Later connections are routed to the running listener.

use crate::{config, host, memory, persist};
use host::{js_error, method, property, then};
use pumpkin::{data::VanillaData, server::Server, PumpkinServer};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{atomic::Ordering, Arc},
};
use tokio::runtime::EventLoopRuntime;
use wasm_bindgen::prelude::*;

pub const PORT: u16 = 25565;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Idle,
    Starting,
    Running,
    Stopping,
    Failed,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Starting => "starting",
            Phase::Running => "running",
            Phase::Stopping => "stopping",
            Phase::Failed => "failed",
        }
    }
}

thread_local! {
    static PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
    static SERVER: RefCell<Option<Arc<Server>>> = const { RefCell::new(None) };
    /// Pumpkin's logger is process-wide and installed once.
    static LOGGER: Cell<bool> = const { Cell::new(false) };
}

/// State shared between the exports and the scheduled roots.
struct Shared {
    /// `ctx.storage`
    storage: JsValue,
    runtime: EventLoopRuntime,
    phase: Cell<Phase>,
    failure: RefCell<Option<String>>,
    connections: Cell<u32>,
    startup_ms: Cell<Option<f64>>,
    checkpointed_at: RefCell<Option<String>>,
    /// Signals the server root that the last connection has closed.
    idle: Arc<tokio::sync::Notify>,
    /// Resolves at the next phase change; connections arriving mid-transition
    /// wait on it and retry.
    transition: RefCell<Option<(js_sys::Promise, js_sys::Function)>>,
}

// Closures run under catch_unwind; the object is only ever touched from this
// thread through the runtime's calls, so a poisoned borrow cannot be observed.
impl std::panic::RefUnwindSafe for Shared {}

#[wasm_bindgen]
pub struct MinecraftWorld {
    shared: Rc<Shared>,
}

#[wasm_bindgen]
impl MinecraftWorld {
    #[wasm_bindgen(constructor)]
    pub fn new(state: JsValue, _env: JsValue) -> Result<MinecraftWorld, JsValue> {
        std::panic::set_hook(Box::new(|info| {
            PANIC.with(|slot| {
                slot.borrow_mut().get_or_insert_with(|| info.to_string());
            });
            eprintln!("RUST PANIC: {info}");
        }));
        let storage = property(&state, "storage")?;
        persist::prepare(&storage)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build_event_loop_runtime()
            .map_err(js_error)?;
        Ok(MinecraftWorld {
            shared: Rc::new(Shared {
                storage,
                runtime,
                phase: Cell::new(Phase::Idle),
                failure: RefCell::new(None),
                connections: Cell::new(0),
                startup_ms: Cell::new(None),
                checkpointed_at: RefCell::new(None),
                idle: Arc::new(tokio::sync::Notify::new()),
                transition: RefCell::new(None),
            }),
        })
    }

    /// Serves one inbound connection; resolves when it closes. The first call
    /// while idle also runs the server, resolving once the world is checkpointed.
    pub fn connect(&self, socket: JsValue) -> Result<JsValue, JsValue> {
        self.shared.connect(socket)
    }

    pub fn status(&self) -> Result<JsValue, JsValue> {
        let shared = &self.shared;
        let result = js_sys::Object::new();
        let set = |key: &str, value: JsValue| js_sys::Reflect::set(&result, &key.into(), &value);
        set("phase", shared.phase.get().name().into())?;
        set("connections", (shared.connections.get() as f64).into())?;
        set(
            "failure",
            shared.failure.borrow().as_deref().map_or(JsValue::NULL, JsValue::from),
        )?;
        set("startup_ms", shared.startup_ms.get().map_or(JsValue::NULL, JsValue::from))?;
        set(
            "checkpointed_at",
            shared.checkpointed_at.borrow().as_deref().map_or(JsValue::NULL, JsValue::from),
        )?;
        let server = SERVER.with(|slot| slot.borrow().clone());
        set(
            "server",
            match (shared.phase.get(), server) {
                (Phase::Running, Some(server)) => server_status(&server)?,
                _ => JsValue::NULL,
            },
        )?;
        Ok(result.into())
    }
}

impl Shared {
    fn connect(self: &Rc<Self>, socket: JsValue) -> Result<JsValue, JsValue> {
        match self.phase.get() {
            Phase::Running => return self.route(socket),
            Phase::Failed => {
                return Err(js_error(self.failure.borrow().as_deref().unwrap_or("failed")))
            }
            Phase::Starting | Phase::Stopping => {
                let shared = self.clone();
                let retry = Closure::once_into_js(move |_: JsValue| shared.connect(socket));
                return then(&self.transition()?.into(), Some(&retry), None);
            }
            Phase::Idle => {}
        }
        self.set_phase(Phase::Starting);
        let started = js_sys::Date::now();
        let root = self.clone();
        self.promise(async move { root.run(socket, started).await })
    }

    /// The promise of the next phase change.
    fn transition(&self) -> Result<js_sys::Promise, JsValue> {
        let mut slot = self.transition.borrow_mut();
        if let Some((promise, _)) = slot.as_ref() {
            return Ok(promise.clone());
        }
        let resolvers = with_resolvers()?;
        let promise: js_sys::Promise = property(&resolvers, "promise")?.unchecked_into();
        let resolve: js_sys::Function = property(&resolvers, "resolve")?.unchecked_into();
        Ok(slot.insert((promise, resolve)).0.clone())
    }

    fn set_phase(&self, phase: Phase) {
        self.phase.set(phase);
        if let Some((_, resolve)) = self.transition.borrow_mut().take() {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
    }

    /// Schedules `future` as a root and returns the Promise of its outcome.
    fn promise(
        self: &Rc<Self>,
        future: impl std::future::Future<Output = Result<JsValue, JsValue>> + 'static,
    ) -> Result<JsValue, JsValue> {
        let resolvers = with_resolvers()?;
        let promise = property(&resolvers, "promise")?;
        let resolve: js_sys::Function = property(&resolvers, "resolve")?.unchecked_into();
        let reject: js_sys::Function = property(&resolvers, "reject")?.unchecked_into();
        let shared = self.clone();
        self.runtime.schedule(future, move |outcome| {
            let outcome =
                outcome.unwrap_or_else(|panic| Err(js_error(format!("server task failed: {panic}"))));
            match outcome {
                Ok(value) => {
                    let _ = resolve.call1(&JsValue::UNDEFINED, &value);
                }
                Err(error) => {
                    shared.fail(&error);
                    let _ = reject.call1(&JsValue::UNDEFINED, &error);
                }
            }
        });
        Ok(promise)
    }

    fn fail(&self, error: &JsValue) {
        if self.phase.get() != Phase::Failed {
            self.set_phase(Phase::Failed);
            let message = js_sys::Error::from(error.clone()).message();
            *self.failure.borrow_mut() = Some(String::from(message));
        }
    }

    /// Routes a socket to Pumpkin's listener and tracks it until it closes.
    fn route(self: &Rc<Self>, socket: JsValue) -> Result<JsValue, JsValue> {
        self.connections.set(self.connections.get() + 1);
        let promise = host::handle_as_node_connection(&socket)?;
        let shared = self.clone();
        let done = Closure::once_into_js(move |_: JsValue| {
            shared.connections.set(shared.connections.get() - 1);
            if shared.connections.get() == 0 {
                shared.idle.notify_one();
            }
        });
        then(&promise, Some(&done), Some(&done))
    }

    async fn run(self: Rc<Self>, first: JsValue, started: f64) -> Result<JsValue, JsValue> {
        let restored = persist::restore(&self.storage)?;
        std::env::set_current_dir(persist::ROOT).map_err(js_error)?;
        eprintln!("Restored {restored} world files");
        pumpkin::reset_stop();
        let (basic, advanced) = config::configuration();
        if !LOGGER.replace(true) {
            pumpkin::init_logger(&advanced);
        }
        let server = PumpkinServer::new(basic, advanced, VanillaData::load()).await;
        server.init_plugins().await;
        SERVER.with(|slot| *slot.borrow_mut() = Some(server.server.clone()));
        self.set_phase(Phase::Running);
        self.startup_ms.set(Some(js_sys::Date::now() - started));

        // The first connection's promise belongs to the platform; the server
        // root tracks it only through `connections`.
        drop(self.route(first)?);
        let idle = self.idle.clone();
        let shared = self.clone();
        tokio::task::spawn_local(async move {
            loop {
                idle.notified().await;
                // A connection may have been routed since the notification.
                if shared.connections.get() == 0 {
                    break;
                }
            }
            // Stopping before the listener closes, so no connection is routed
            // to a server that will not accept it.
            shared.set_phase(Phase::Stopping);
            pumpkin::stop_server();
        });
        // Returns after the final save once stopped.
        server.start().await;
        SERVER.with(|slot| slot.borrow_mut().take());
        if let Some(panic) = PANIC.with(|slot| slot.borrow_mut().take()) {
            return Err(js_error(panic));
        }
        let (written, removed) = persist::save(&self.storage)?;
        eprintln!("Checkpoint: {written} files written, {removed} removed");
        // Issued writes become durable before the connection is reported closed.
        let synced = method(&self.storage, "sync", &[])?;
        let shared = self.clone();
        let done = Closure::once_into_js(move |_: JsValue| {
            shared
                .checkpointed_at
                .replace(Some(String::from(js_sys::Date::new_0().to_iso_string())));
            shared.set_phase(Phase::Idle);
        });
        then(&synced, Some(&done), None)
    }
}

fn with_resolvers() -> Result<JsValue, JsValue> {
    method(&js_sys::Promise::resolve(&JsValue::UNDEFINED).constructor().into(), "withResolvers", &[])
}

fn server_status(server: &Server) -> Result<JsValue, JsValue> {
    let result = js_sys::Object::new();
    let set = |key: &str, value: f64| js_sys::Reflect::set(&result, &key.into(), &value.into());
    set("players", server.get_all_players().len() as f64)?;
    set("ticks", server.tick_count.load(Ordering::Relaxed) as f64)?;
    set(
        "wasm_memory_bytes",
        (core::arch::wasm32::memory_size::<0>() * 65536) as f64,
    )?;
    for (name, bytes) in memory::heap_usage() {
        set(name, bytes as f64)?;
    }
    Ok(result.into())
}

pub fn authority() -> String {
    format!("world:{PORT}")
}
