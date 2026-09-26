//! The Durable Object hosting one Pumpkin server.
//!
//! A `#[durable_object(connect)]`: its handlers run on the thread's ambient
//! Tokio event loop, whose wait is the host event loop, so nothing blocks or
//! suspends. The server lifetime is one such future: the first `connect` after
//! idle runs it on the world mounted from the object's storage, and it
//! completes after the final save is synced. Later connections are routed to
//! the running listener, and the handler outlives each routed connection.

use crate::{config, host, memory, settings::Settings};
use host::{js_error, method, property};
use pumpkin::{data::VanillaData, server::Server, PumpkinServer};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{atomic::Ordering, Arc},
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use worker::{durable_object, DurableObject, Env, Socket, State};

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
    storage: worker::durable::Storage,
    phase: Cell<Phase>,
    failure: RefCell<Option<String>>,
    connections: Cell<u32>,
    startup_ms: Cell<Option<f64>>,
    saved_at: RefCell<Option<String>>,
    /// Servers started in this object's lifetime.
    starts: Cell<u32>,
    /// While running with no players: the time the server will stop.
    idle_deadline: Cell<Option<f64>>,
    settings: Result<Settings, String>,
    /// Signals the server root that the last connection has closed.
    idle: Arc<tokio::sync::Notify>,
    /// Resolves at the next phase change; connections arriving mid-transition
    /// wait on it and retry.
    transition: RefCell<Option<(js_sys::Promise, js_sys::Function)>>,
}

// Closures run under catch_unwind; the object is only ever touched from this
// thread, so a poisoned borrow cannot be observed.
impl std::panic::RefUnwindSafe for Shared {}

#[durable_object(connect)]
pub struct MinecraftWorld {
    shared: Rc<Shared>,
}

impl DurableObject for MinecraftWorld {
    fn new(state: State, env: Env) -> Self {
        std::panic::set_hook(Box::new(|info| {
            PANIC.with(|slot| {
                slot.borrow_mut().get_or_insert_with(|| info.to_string());
            });
            eprintln!("RUST PANIC: {info}");
        }));
        let storage = state.storage();
        host::mount_storage(storage.as_raw());
        MinecraftWorld {
            shared: Rc::new(Shared {
                storage,
                phase: Cell::new(Phase::Idle),
                failure: RefCell::new(None),
                connections: Cell::new(0),
                startup_ms: Cell::new(None),
                saved_at: RefCell::new(None),
                starts: Cell::new(0),
                idle_deadline: Cell::new(None),
                settings: Settings::from_env(&env),
                idle: Arc::new(tokio::sync::Notify::new()),
                transition: RefCell::new(None),
            }),
        }
    }

    async fn fetch(&self, _req: worker::Request) -> worker::Result<worker::Response> {
        worker::Response::error("tcp only", 404)
    }

    /// Serves one inbound connection; resolves when it closes. The first call
    /// while idle also runs the server, resolving once the final save is synced.
    async fn connect(&self, socket: Socket) -> worker::Result<()> {
        let shared = self.shared.clone();
        // On a running server `connect` resolves to the promise of the routed
        // connection; this handler must outlive it, or the host closes the socket.
        let served = shared.connect(socket).await.map_err(worker::Error::from)?;
        if let Ok(promise) = served.dyn_into::<js_sys::Promise>() {
            JsFuture::from(promise).await.map_err(worker::Error::from)?;
        }
        Ok(())
    }
}

/// RPC methods for the Worker's status and server-list responders.
#[wasm_bindgen]
impl MinecraftWorld {
    pub fn status(&self) -> Result<JsValue, JsValue> {
        let shared = &self.shared;
        let result = js_sys::Object::new();
        let set = |key: &str, value: JsValue| js_sys::Reflect::set(&result, &key.into(), &value);
        set("phase", shared.phase.get().name().into())?;
        set("connections", (shared.connections.get() as f64).into())?;
        set(
            "failure",
            shared
                .failure
                .borrow()
                .as_deref()
                .map_or(JsValue::NULL, JsValue::from),
        )?;
        set(
            "startup_ms",
            shared.startup_ms.get().map_or(JsValue::NULL, JsValue::from),
        )?;
        set(
            "saved_at",
            shared
                .saved_at
                .borrow()
                .as_deref()
                .map_or(JsValue::NULL, JsValue::from),
        )?;
        set("runtime_starts", shared.starts.get().into())?;
        set(
            "idle_deadline",
            shared
                .idle_deadline
                .get()
                .map_or(JsValue::NULL, JsValue::from),
        )?;
        set(
            "settings",
            match &shared.settings {
                Ok(settings) => settings.to_js()?,
                Err(_) => JsValue::NULL,
            },
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

    /// Server-list metadata for the Worker's status responder, without
    /// starting a server.
    #[wasm_bindgen(js_name = serverList)]
    pub fn server_list(&self, protocol: f64) -> Result<JsValue, JsValue> {
        let shared = &self.shared;
        let settings = shared.settings.as_ref().map_err(|e| js_error(e.clone()))?;
        let online = SERVER.with(|slot| match (shared.phase.get(), slot.borrow().as_ref()) {
            (Phase::Running, Some(server)) => server.get_all_players().len(),
            _ => 0,
        });
        let text = if shared.phase.get() == Phase::Failed {
            "Server unavailable"
        } else {
            &settings.motd
        };
        let protocol = if (MIN_PROTOCOL..=MAX_PROTOCOL).contains(&protocol) {
            protocol
        } else {
            MIN_PROTOCOL
        };
        let json = serde_json::json!({
            "version": { "name": VERSION_NAME, "protocol": protocol },
            "players": { "max": settings.max_players, "online": online },
            "description": { "text": text },
            "enforcesSecureChat": false,
        });
        Ok(JsValue::from(json.to_string()))
    }
}

/// The Java versions the pinned Pumpkin accepts.
const MIN_PROTOCOL: f64 = 4.0;
const MAX_PROTOCOL: f64 = 776.0;
const VERSION_NAME: &str = "1.7.2-26.2";

impl Shared {
    async fn connect(self: Rc<Self>, socket: worker::Socket) -> Result<JsValue, JsValue> {
        loop {
            match self.phase.get() {
                Phase::Running => return self.route(socket),
                Phase::Failed => {
                    return Err(js_error(
                        self.failure.borrow().as_deref().unwrap_or("failed"),
                    ))
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
    fn route(self: &Rc<Self>, socket: worker::Socket) -> Result<JsValue, JsValue> {
        self.connections.set(self.connections.get() + 1);
        self.idle_deadline.set(None);
        let shared = self.clone();
        let served = async move {
            let outcome = socket.handle_as_node_connection().await;
            shared.connections.set(shared.connections.get() - 1);
            if shared.connections.get() == 0 {
                shared.idle.notify_one();
            }
            outcome
                .map(|()| JsValue::UNDEFINED)
                .map_err(|e| JsValue::from(e))
        };
        Ok(wasm_bindgen_futures::future_to_promise(served).into())
    }

    async fn run(self: Rc<Self>, first: worker::Socket, started: f64) -> Result<JsValue, JsValue> {
        let root = host::MOUNT_ROOT
            .with(|root| root.as_string())
            .unwrap_or_default();
        std::fs::create_dir_all(&root).map_err(js_error)?;
        std::env::set_current_dir(&root).map_err(js_error)?;
        pumpkin::reset_stop();
        let settings = self.settings.clone().map_err(js_error)?;
        let (basic, advanced) = config::configuration(&settings);
        if !LOGGER.replace(true) {
            pumpkin::init_logger(&advanced);
        }
        let server = PumpkinServer::new(basic, advanced, VanillaData::load()).await;
        server.init_plugins().await;
        SERVER.with(|slot| *slot.borrow_mut() = Some(server.server.clone()));
        self.set_phase(Phase::Running);
        self.startup_ms.set(Some(js_sys::Date::now() - started));
        self.starts.set(self.starts.get() + 1);

        // The first connection's promise belongs to the platform; the server
        // root tracks it only through `connections`.
        drop(self.route(first)?);
        let idle = self.idle.clone();
        let shared = self.clone();
        let timeout = std::time::Duration::from_secs(settings.idle_timeout_seconds.into());
        tokio::task::spawn_local(async move {
            loop {
                idle.notified().await;
                // A connection may have been routed since the notification.
                if shared.connections.get() != 0 {
                    continue;
                }
                // Players are saved on disconnect; save the world too, so the
                // mount holds everything before the idle period rather than
                // only at stop.
                if let Some(server) = SERVER.with(|slot| slot.borrow().clone()) {
                    save_world(&server).await;
                }
                if shared.connections.get() != 0 {
                    continue;
                }
                // Keep the world up briefly so a rejoining player finds it running.
                shared
                    .idle_deadline
                    .set(Some(js_sys::Date::now() + timeout.as_millis() as f64));
                tokio::time::sleep(timeout).await;
                shared.idle_deadline.set(None);
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
        self.storage.sync().await.map_err(JsValue::from)?;
        self.saved_at
            .replace(Some(String::from(js_sys::Date::new_0().to_iso_string())));
        self.set_phase(Phase::Idle);
        Ok(JsValue::UNDEFINED)
    }
}

/// Saves players, world data and dirty chunks, returning once the chunk writer
/// has handed them to the filesystem.
async fn save_world(server: &Server) {
    use pumpkin_world::chunk::io::FileIO;
    if let Err(error) = server.save_all().await {
        eprintln!("World save after last disconnect failed: {error}");
    }
    for world in server.worlds.load().iter() {
        let level = &world.level;
        // `save_all` flags the scheduler; the flag clears once it has queued the
        // dirty chunks for writing.
        while level.should_save.load(Ordering::Relaxed) {
            tokio::task::yield_now().await;
        }
        level.chunk_saver.block_and_await_ongoing_tasks().await;
    }
}

fn with_resolvers() -> Result<JsValue, JsValue> {
    method(
        &js_sys::Promise::resolve(&JsValue::UNDEFINED)
            .constructor()
            .into(),
        "withResolvers",
        &[],
    )
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
