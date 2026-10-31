//! Public engine handle: thread lifecycle, commands and queries.

use crate::recorder::{ProcNow, Recorder};
use crate::store::{self, GameStore, Store};
use blackbox_core::kb::{self, Kb};
use blackbox_core::model::*;
use blackbox_sys::drivers::DriverRegistry;
use blackbox_sys::etw::kernel::DriverStat;
use blackbox_sys::util::background_current_thread;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub data_dir: PathBuf,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct OverlayText {
    pub title: String,
    pub body: String,
    pub severity: Severity,
}

/// A notification the engine decided to deliver, and on which channels.
#[derive(Debug, Clone)]
pub struct Notification {
    pub toast: Toast,
    pub in_app: bool,
    pub overlay: Option<OverlayText>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrayState {
    pub recording: bool,
    pub unread: u32,
    pub health: Health,
    pub lang: Lang,
}

/// Where the engine pushes what happens. Called from the engine thread: keep it cheap.
pub trait EngineSink: Send + Sync + 'static {
    fn status(&self, status: &LiveStatus);
    fn incident(&self, incident: &Incident);
    fn notify(&self, notification: &Notification);
    fn tray(&self, state: &TrayState);
    fn settings(&self, _settings: &Settings) {}
    /// A play session was saved: the game's updated record.
    fn session(&self, _record: &GameRecord) {}
}

pub(crate) enum Cmd {
    Settings(Settings),
    MarkRead(Option<String>),
    Delete(String),
    Clear,
    TestNotification,
    Shutdown,
}

/// State readable by queries without involving the engine thread.
pub(crate) struct Public {
    pub status: Option<LiveStatus>,
    pub system: SystemInfo,
    pub settings: Settings,
    pub procs: Vec<ProcNow>,
    pub drivers: Vec<DriverStat>,
    pub drivers_max: Vec<(u32, u32)>,
    /// The play session in progress (published while the UI is visible).
    pub live_session: Option<SessionStats>,
}

pub(crate) struct Shared {
    pub kb: Arc<Kb>,
    pub store: Arc<Mutex<Store>>,
    pub games: Arc<Mutex<GameStore>>,
    pub public: Mutex<Public>,
    pub ui_active: AtomicBool,
    pub registry: Arc<DriverRegistry>,
    pub data_dir: PathBuf,
}

impl Shared {
    pub fn public(&self) -> MutexGuard<'_, Public> {
        self.public.lock().unwrap_or_else(|p| p.into_inner())
    }
    pub fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }
}

pub struct Engine {
    shared: Arc<Shared>,
    tx: Sender<Cmd>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    pub fn start(cfg: EngineConfig, sink: Arc<dyn EngineSink>) -> Engine {
        let settings = store::load_settings(&cfg.data_dir);
        let kb = Arc::new(Kb::load());
        let store = Arc::new(Mutex::new(Store::open(&cfg.data_dir, settings.retention)));
        let games = Arc::new(Mutex::new(GameStore::open(&cfg.data_dir)));
        let registry = DriverRegistry::new();
        let rec = Recorder::new(&cfg, settings.clone(), kb.clone(), store.clone(), games.clone(), registry.clone());
        let shared = Arc::new(Shared {
            kb,
            store,
            games,
            public: Mutex::new(Public {
                status: None,
                system: rec.system_info(),
                settings,
                procs: Vec::new(),
                drivers: Vec::new(),
                drivers_max: Vec::new(),
                live_session: None,
            }),
            ui_active: AtomicBool::new(false),
            registry,
            data_dir: cfg.data_dir.clone(),
        });
        let (tx, rx) = mpsc::channel();
        let thread_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("bb-engine".into())
            .spawn(move || run(rec, rx, thread_shared, sink))
            .expect("spawn engine thread");
        Engine { shared, tx, thread: Mutex::new(Some(thread)) }
    }

    pub fn bootstrap(&self) -> Bootstrap {
        let (status, system, settings) = {
            let p = self.shared.public();
            (p.status.clone(), p.system.clone(), p.settings.clone())
        };
        Bootstrap {
            status,
            system,
            settings,
            incidents: self.shared.store().summaries().to_vec(),
            kb: self.shared.kb.entries().to_vec(),
        }
    }

    pub fn incident(&self, id: &str) -> Option<Incident> {
        self.shared.store().get(id)
    }

    pub fn incidents(&self) -> Vec<Incident> {
        self.shared.store().summaries().to_vec()
    }

    pub fn drivers(&self) -> Vec<DriverInfo> {
        let (cum, max) = {
            let p = self.shared.public();
            (p.drivers.clone(), p.drivers_max.clone())
        };
        let mut out: Vec<DriverInfo> = self
            .shared
            .registry
            .snapshot()
            .into_iter()
            .enumerate()
            .map(|(i, (name, loaded))| {
                let s = cum.get(i).copied().unwrap_or_default();
                let (dmax, imax) = max.get(i).copied().unwrap_or((0, 0));
                let (friendly, category) = kb::driver_info(&name);
                DriverInfo {
                    live: DriverLive {
                        name,
                        friendly: friendly.map(str::to_owned),
                        category,
                        dpc_count: s.dpc_count,
                        dpc_total_us: s.dpc_total_us,
                        dpc_max_us: dmax,
                        isr_count: s.isr_count,
                        isr_total_us: s.isr_total_us,
                        isr_max_us: imax,
                    },
                    loaded,
                    histogram: s.hist,
                }
            })
            .collect();
        out.sort_by(|a, b| {
            (b.live.dpc_total_us + b.live.isr_total_us)
                .cmp(&(a.live.dpc_total_us + a.live.isr_total_us))
                .then_with(|| a.live.name.cmp(&b.live.name))
        });
        out
    }

    pub fn processes(&self) -> Vec<ProcLive> {
        let p = self.shared.public();
        let mut v: Vec<ProcLive> = p.procs.iter().map(ProcNow::to_live).collect();
        v.sort_by(|a, b| b.cpu_pct.total_cmp(&a.cpu_pct).then_with(|| b.io_bps.total_cmp(&a.io_bps)));
        v
    }

    /// Every game with its sessions, most recently played first.
    pub fn games(&self) -> Vec<GameRecord> {
        let live = self.shared.public().live_session.clone();
        self.shared.games.lock().unwrap_or_else(|p| p.into_inner()).records(live.as_ref())
    }

    pub fn delete_game(&self, name: &str) {
        self.shared.games.lock().unwrap_or_else(|p| p.into_inner()).delete(name);
    }

    pub fn settings(&self) -> Settings {
        self.shared.public().settings.clone()
    }

    pub fn set_settings(&self, s: Settings) -> Settings {
        let s = s.sanitized();
        self.shared.public().settings = s.clone();
        let _ = self.tx.send(Cmd::Settings(s.clone()));
        s
    }

    /// The UI is visible: stream live status. Hidden: the engine stops building it.
    pub fn set_ui_active(&self, active: bool) {
        self.shared.ui_active.store(active, Ordering::Relaxed);
    }

    pub fn mark_read(&self, id: Option<String>) {
        let _ = self.tx.send(Cmd::MarkRead(id));
    }

    pub fn delete(&self, id: String) {
        let _ = self.tx.send(Cmd::Delete(id));
    }

    pub fn clear(&self) {
        let _ = self.tx.send(Cmd::Clear);
    }

    pub fn export(&self, id: &str, to_dir: &std::path::Path) -> std::io::Result<PathBuf> {
        self.shared.store().export(id, to_dir)
    }

    pub fn test_notification(&self) {
        let _ = self.tx.send(Cmd::TestNotification);
    }

    pub fn data_dir(&self) -> PathBuf {
        self.shared.data_dir.clone()
    }

    pub fn kb(&self) -> Arc<Kb> {
        self.shared.kb.clone()
    }

    /// Stops ETW sessions and joins the engine thread. Idempotent.
    pub fn shutdown(&self) {
        let _ = self.tx.send(Cmd::Shutdown);
        let handle = self.thread.lock().ok().and_then(|mut t| t.take());
        if let Some(h) = handle {
            let _ = h.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(mut rec: Recorder, rx: mpsc::Receiver<Cmd>, shared: Arc<Shared>, sink: Arc<dyn EngineSink>) {
    background_current_thread();
    rec.start_tracing();
    shared.public().system = rec.system_info();
    let mut next = rec.now_us() + 1_000_000;
    loop {
        let now = rec.now_us();
        let wait = Duration::from_micros((next - now).max(0) as u64);
        match rx.recv_timeout(wait) {
            Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(cmd) => {
                if let Err(e) = catch_unwind(AssertUnwindSafe(|| rec.handle(cmd, &shared, &*sink))) {
                    log::error!("engine command panicked: {e:?}");
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Err(e) = catch_unwind(AssertUnwindSafe(|| rec.tick(&shared, &*sink))) {
                    log::error!("engine tick panicked: {e:?}");
                }
                next += 1_000_000;
                let now = rec.now_us();
                if next <= now {
                    next = now + 1_000_000; // we fell behind (system was busy or asleep)
                }
            }
        }
    }
    rec.stop_tracing();
}
