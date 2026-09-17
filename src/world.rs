//! The Durable Object hosting one Pumpkin server.
//!
//! `connect` is a `#[wasm_bindgen(experimental_tokio)]` export: its future runs on the
//! thread's Tokio event loop, whose wait is the host event loop, so nothing
//! blocks or suspends and the export returns a Promise. The server lifetime is
//! one such future: the first `connect` after idle runs it on the world mounted
//! from the object's storage, and it completes after the final save is synced.
//! Later connections are routed to the running listener.

use crate::{config, host, memory};
use host::{js_error, method, property, then};
use pumpkin::{data::VanillaData, server::Server, PumpkinServer};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{atomic::Ordering, Arc},
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

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

/// State shared between the exports and the server future.
struct Shared {
    /// `ctx.storage`
    storage: JsValue,
    phase: Cell<Phase>,
    failure: RefCell<Option<String>>,
    connections: Cell<u32>,
    startup_ms: Cell<Option<f64>>,
    saved_at: RefCell<Option<String>>,
    /// Signals the server root that the last connection has closed.
    idle: Arc<tokio::sync::Notify>,
    /// Resolves at the next phase change; connections arriving mid-transition
    /// wait on it and retry.
    transition: RefCell<Option<(js_sys::Promise, js_sys::Function)>>,
}

// Closures run under catch_unwind; the object is only ever touched from this
// thread, so a poisoned borrow cannot be observed.
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
        host::mount_storage(&storage);
        Ok(MinecraftWorld {
            shared: Rc::new(Shared {
                storage,
                phase: Cell::new(Phase::Idle),
                failure: RefCell::new(None),
                connections: Cell::new(0),
                startup_ms: Cell::new(None),
                saved_at: RefCell::new(None),
                idle: Arc::new(tokio::sync::Notify::new()),
                transition: RefCell::new(None),
            }),
        })
    }

    /// Serves one inbound connection; resolves when it closes. The first call
    /// while idle also runs the server, resolving once the final save is synced.
    #[wasm_bindgen(experimental_tokio)]
    pub async fn connect(&self, socket: JsValue) -> Result<JsValue, JsValue> {
        let shared = self.shared.clone();
        shared.connect(socket).await
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
            "saved_at",
            shared.saved_at.borrow().as_deref().map_or(JsValue::NULL, JsValue::from),
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
    async fn connect(self: Rc<Self>, socket: JsValue) -> Result<JsValue, JsValue> {
        loop {
            match self.phase.get() {
                Phase::Running => return self.route(socket),
                Phase::Failed => {
                    return Err(js_error(self.failure.borrow().as_deref().unwrap_or("failed")))
                }
                Phase::Starting | Phase::Stopping => {
                    JsFuture::from(self.transition()?).await?;
                }
                Phase::Idle => break,
            }
        }
        self.set_phase(Phase::Starting);
        let started = js_sys::Date::now();
        // A task of its own so a panic arrives as a `JoinError` and fails the
        // object rather than escaping the export.
        let outcome = match tokio::task::spawn_local(self.clone().run(socket, started)).await {
            Ok(outcome) => outcome,
            Err(panic) => Err(js_error(format!("server task failed: {panic}"))),
        };
        if let Err(error) = &outcome {
            self.fail(error);
        }
        outcome
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
        let root = host::MOUNT_ROOT.with(|root| root.as_string()).unwrap_or_default();
        std::fs::create_dir_all(&root).map_err(js_error)?;
        std::env::set_current_dir(&root).map_err(js_error)?;
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
        // Writes issued through the mount become durable before the connection
        // is reported closed.
        let synced = method(&self.storage, "sync", &[])?;
        let shared = self.clone();
        let done = Closure::once_into_js(move |_: JsValue| {
            shared
                .saved_at
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
