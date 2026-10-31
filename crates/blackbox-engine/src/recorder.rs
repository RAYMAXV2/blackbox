//! The recorder: owns the collectors and every in-memory series. Runs on the engine thread.

use crate::engine::{Cmd, EngineConfig, EngineSink, Notification, OverlayText, Shared, TrayState};
use crate::play::{PendingSecond, PlaySession};
use crate::store::{self, GameStore, Store};
use crate::sysinfo;
use blackbox_core::frames::{steady_presenter, FrameTracker, HitchRule, GAME_SPAN_US};
use blackbox_core::i18n;
use blackbox_core::kb::{self, Kb};
use blackbox_core::model::*;
use blackbox_core::params;
use blackbox_core::series::*;
use blackbox_core::session::TickSample;
use blackbox_sys::cputemp::{CpuTemp, Unavailable as CpuTempUnavailable};
use blackbox_sys::drivers::DriverRegistry;
use blackbox_sys::etw::kernel::{self as kernel_etw, DriverStat, KernelData, KernelSink, RawProcEvent};
use blackbox_sys::etw::present::{self as present_etw, PresentData, PresentEvent, PresentSink};
use blackbox_sys::etw::Session;
use blackbox_sys::foreground::{is_window_hung, Foreground};
use blackbox_sys::input::{InputProbe, InputSample};
use blackbox_sys::nvml::{GpuReading, Nvml};
use blackbox_sys::snapshot::{CoreTimes, ProcRaw, Snapshotter};
use blackbox_sys::system::{self, CpuFreq, MemoryInfo, SelfUsage};
use blackbox_sys::util::{enable_privilege, is_elevated, Clock};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Driver activity kept for the live view (seconds).
const DRIVER_WINDOW_S: usize = 10;
/// A game session lasts while the game runs: it only ends after this long out of the game.
/// (Alt-tabs don't split a play session; the time away isn't counted in its statistics.)
const GAME_AWAY_US: Micros = 300_000_000;
/// Shorter sessions (launchers, crashes on start) are not saved.
const MIN_SESSION_SEC: u64 = 60;
/// Presents newer than this stay buffered one more tick: late ETW buffers can still arrive.
const FRAME_HOLD_US: Micros = 250_000;
/// GPU polling when no game runs and the dashboard is hidden.
const GPU_IDLE_POLL_US: Micros = 5_000_000;
/// Window for Blackbox's own CPU readout (GetProcessTimes is quantized to ~15.6 ms).
const SELF_WINDOW: usize = 10;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProcPrev {
    cpu_100ns: i64,
    io_bytes: u64,
    hard_faults: u32,
    tag: Option<ProcTag>,
}

/// One process over the last second.
#[derive(Debug, Clone)]
pub(crate) struct ProcNow {
    pub pid: u32,
    pub name: Arc<str>,
    pub tag: Option<ProcTag>,
    pub cpu_pct: f32,
    pub io_bps: f64,
    pub working_set_mb: f32,
    pub hard_faults: u32,
}

impl ProcNow {
    pub fn to_live(&self) -> ProcLive {
        ProcLive {
            pid: self.pid,
            name: self.name.to_string(),
            cpu_pct: (self.cpu_pct * 10.0).round() / 10.0,
            io_bps: self.io_bps.round(),
            working_set_mb: self.working_set_mb.round(),
            tag: self.tag,
        }
    }

    pub fn to_use(&self, gpu_pct: Option<f32>) -> ProcUse {
        ProcUse {
            pid: self.pid,
            name: self.name.clone(),
            tag: self.tag,
            cpu_pct: self.cpu_pct,
            io_bps: self.io_bps,
            hard_faults: self.hard_faults,
            ram_mb: self.working_set_mb,
            gpu_pct,
        }
    }
}

pub(crate) struct GameSession {
    pub pid: u32,
    /// The game's window, followed while it is hung (Windows then shows a ghost window).
    pub hwnd: isize,
    pub name: Arc<str>,
    pub tracker: FrameTracker,
    pub started_us: Micros,
    pub away_since: Option<Micros>,
    pub fullscreen: bool,
    pub held: Vec<Incident>,
    pub play: PlaySession,
}

/// An incident waiting for the data around it (ETW buffers arrive up to ~1 s late).
#[derive(Debug, Clone)]
pub(crate) struct Pending {
    pub kind: IncidentKind,
    pub severity: Severity,
    pub marker_us: Micros,
    pub focus_from: Micros,
    pub focus_to: Micros,
    /// Timeline window stored with the incident.
    pub tl_from: Micros,
    pub tl_to: Micros,
    pub excess_ms: f32,
    pub duration_s: f32,
    pub magnitude: Magnitude,
    pub app: Option<(u32, Arc<str>)>,
    pub ready_at: Micros,
}

pub(crate) struct HangState {
    pub pid: u32,
    pub hwnd: isize,
    pub since: Micros,
    pub reported: bool,
}

pub(crate) struct Recorder {
    pub clock: Clock,
    pub kb: Arc<Kb>,
    pub settings: Settings,
    pub data_dir: PathBuf,
    pub version: String,
    pub elevated: bool,
    pub ncpu: usize,
    pub own_pid: u32,
    pub start_us: Micros,
    pub machine: blackbox_sys::sysinfo::MachineInfo,
    pub cpu_name: String,

    // collectors
    kernel: Option<Session>,
    frames: Option<Session>,
    pub kernel_err: Option<u32>,
    pub frames_err: Option<u32>,
    kdata: Arc<Mutex<KernelData>>,
    pdata: Arc<Mutex<PresentData>>,
    pub registry: Arc<DriverRegistry>,
    pub fg: Arc<Foreground>,
    snap: Snapshotter,
    raw: Vec<ProcRaw>,
    prev: HashMap<(u32, i64), ProcPrev>,
    prev_next: HashMap<(u32, i64), ProcPrev>,
    /// Blackbox and its WebView2 processes: never blamed, never a "game".
    pub own_family: HashSet<u32>,
    pub procs: Vec<ProcNow>,
    pub names: HashMap<u32, (Arc<str>, Micros)>,
    pub proc_count: u32,
    pub thread_count: u32,
    cores_prev: Vec<CoreTimes>,
    cores_now: Vec<CoreTimes>,
    pub per_core: Vec<f32>,
    pub cpu_total: f32,
    pub cpu_kernel: f32,
    pub dpc_pct: f32,
    pub isr_pct: f32,
    pub mem: MemoryInfo,
    freq: CpuFreq,
    pub freq_limit: f32,
    nvml: Option<Nvml>,
    last_gpu_us: Micros,
    pub gpu: Option<GpuSample>,
    pub gpu_name: Option<String>,
    /// GPU usage per process (NVML), highest first.
    gpu_procs: Vec<(u32, f32)>,
    cpu_temp: Option<CpuTemp>,
    pub cpu_temp_err: Option<CpuTempUnavailable>,
    pub cpu_temp_c: Option<f32>,
    net_prev: Option<(u64, u64)>,
    pub net_rate: (f64, f64),
    self_hist: VecDeque<(Micros, i64)>,
    pub self_cpu_pct: f32,
    pub self_usage: SelfUsage,
    pub timer_res_ms: f32,

    // series
    pub kring: KernelRing,
    pub seconds: Ring<SecondSample>,
    pub events: Ring<ProcEvent>,
    bucket_scratch: Vec<KernelBucket>,
    frame_scratch: Vec<PresentEvent>,
    frame_hold: Vec<PresentEvent>,
    pub drv_prev: Vec<DriverStat>,
    pub drv_window: VecDeque<Vec<(u16, DriverStat)>>,
    pub drv_session_max: Vec<(u32, u32)>,
    pub modules_available: bool,
    bad_timestamps_logged: u64,

    // detection
    pub game: Option<GameSession>,
    /// Present timestamps of the foreground app while deciding whether it is a game.
    candidate_pid: u32,
    candidate_frames: VecDeque<Micros>,
    pub pending: Vec<Pending>,
    pub last_by_key: HashMap<String, Micros>,
    pub last_hitch: Option<(Micros, Severity)>,
    pub hang: Option<HangState>,
    pub throttle_since: Option<Micros>,
    pub pressure_since: Option<Micros>,
    pub recent: VecDeque<(Micros, Severity)>,
    pub incidents_session: u32,
    next_id: u32,
    pub store: Arc<Mutex<Store>>,
    pub games: Arc<Mutex<GameStore>>,
    input: InputProbe,

    // bookkeeping
    last_tick_us: Micros,
    events_prev_total: u64,
    pub events_rate: u32,
    pub events_lost: u64,
    lost_reported: u64,
    pub lost_recent: u64,
    last_stats_us: Micros,
    last_restart_us: Micros,
    last_tray: Option<TrayState>,
    tick_count: u64,
    pub power_plan: Option<system::PowerPlan>,
    pub battery: Option<u8>,
}

impl Recorder {
    pub fn new(
        cfg: &EngineConfig,
        settings: Settings,
        kb: Arc<Kb>,
        store: Arc<Mutex<Store>>,
        games: Arc<Mutex<GameStore>>,
        registry: Arc<DriverRegistry>,
    ) -> Recorder {
        let clock = Clock::new();
        let elevated = is_elevated();
        let machine = blackbox_sys::sysinfo::machine();
        let ncpu = (machine.threads as usize).max(1);
        let nvml = Nvml::load();
        let gpu_name = nvml.as_ref().map(|n| n.name.clone());
        let now = clock.now_us();
        let own_pid = std::process::id();
        let mut r = Recorder {
            clock,
            kb,
            settings,
            data_dir: cfg.data_dir.clone(),
            version: cfg.version.clone(),
            elevated,
            ncpu,
            own_pid,
            start_us: now,
            cpu_name: machine.cpu.clone(),
            machine,
            kernel: None,
            frames: None,
            kernel_err: None,
            frames_err: None,
            kdata: Arc::new(Mutex::new(KernelData::default())),
            pdata: Arc::new(Mutex::new(PresentData::default())),
            registry,
            fg: Foreground::start(),
            snap: Snapshotter::new(),
            raw: Vec::with_capacity(512),
            prev: HashMap::with_capacity(512),
            prev_next: HashMap::with_capacity(512),
            own_family: HashSet::from([own_pid]),
            procs: Vec::with_capacity(512),
            names: HashMap::with_capacity(512),
            proc_count: 0,
            thread_count: 0,
            cores_prev: Vec::new(),
            cores_now: Vec::new(),
            per_core: Vec::new(),
            cpu_total: 0.0,
            cpu_kernel: 0.0,
            dpc_pct: 0.0,
            isr_pct: 0.0,
            mem: MemoryInfo::default(),
            freq: CpuFreq::default(),
            freq_limit: 100.0,
            nvml,
            last_gpu_us: i64::MIN / 2,
            gpu: None,
            gpu_name,
            gpu_procs: Vec::new(),
            cpu_temp: None,
            cpu_temp_err: None,
            cpu_temp_c: None,
            net_prev: None,
            net_rate: (0.0, 0.0),
            self_hist: VecDeque::with_capacity(SELF_WINDOW + 1),
            self_cpu_pct: 0.0,
            self_usage: SelfUsage::default(),
            timer_res_ms: 0.0,
            kring: KernelRing::new(KERNEL_RING_LEN),
            seconds: Ring::new(SECOND_RING_LEN),
            events: Ring::new(512),
            bucket_scratch: Vec::with_capacity(256),
            frame_scratch: Vec::with_capacity(1024),
            frame_hold: Vec::with_capacity(1024),
            drv_prev: Vec::new(),
            drv_window: VecDeque::with_capacity(DRIVER_WINDOW_S + 1),
            drv_session_max: Vec::new(),
            modules_available: false,
            bad_timestamps_logged: 0,
            game: None,
            candidate_pid: 0,
            candidate_frames: VecDeque::with_capacity(1024),
            pending: Vec::new(),
            last_by_key: HashMap::new(),
            last_hitch: None,
            hang: None,
            throttle_since: None,
            pressure_since: None,
            recent: VecDeque::new(),
            incidents_session: 0,
            next_id: 0,
            store,
            games,
            input: InputProbe::new(),
            last_tick_us: now,
            events_prev_total: 0,
            events_rate: 0,
            events_lost: 0,
            lost_reported: 0,
            lost_recent: 0,
            last_stats_us: now,
            last_restart_us: now,
            last_tray: None,
            tick_count: 0,
            power_plan: system::active_power_plan(),
            battery: system::on_battery(),
        };
        r.rebaseline(now);
        r
    }

    pub fn now_us(&self) -> Micros {
        self.clock.now_us()
    }

    pub fn mode(&self) -> Mode {
        if self.settings.paused {
            Mode::Paused
        } else if self.kernel.is_some() {
            Mode::Full
        } else {
            Mode::Limited
        }
    }

    pub fn has_cpu_temp(&self) -> bool {
        self.cpu_temp.is_some()
    }

    pub fn system_info(&self) -> SystemInfo {
        sysinfo::build(self)
    }

    /// Fresh baselines for every delta-based reading (start, resume after pause): the first
    /// tick must not divide minutes of accumulated counters by one second.
    fn rebaseline(&mut self, now: Micros) {
        let _ = self.snap.cores(&mut self.cores_prev);
        self.prev.clear();
        self.collect_processes(now, 1.0);
        self.net_prev = None;
        self.self_hist.clear();
        self.throttle_since = None;
        self.pressure_since = None;
        self.hang = None;
        self.candidate_frames.clear();
        self.last_tick_us = now;
    }

    // ───────────────────────────── tracing lifecycle

    pub fn start_tracing(&mut self) {
        if self.settings.paused || self.kernel.is_some() {
            return;
        }
        // Opened first so that, unelevated, the System page can say why there's no temperature.
        if self.cpu_temp.is_none() {
            match CpuTemp::open(&self.cpu_name, self.elevated) {
                Ok(mut t) => {
                    self.cpu_temp_c = t.read();
                    self.cpu_temp = Some(t);
                    self.cpu_temp_err = None;
                    log::info!("CPU temperature: PawnIO");
                }
                Err(e) => {
                    self.cpu_temp_err = Some(e);
                    log::info!("no CPU temperature: {e:?}");
                }
            }
        }
        if !self.elevated {
            self.kernel_err = Some(blackbox_sys::etw::ERROR_ACCESS_DENIED);
            self.frames_err = Some(blackbox_sys::etw::ERROR_ACCESS_DENIED);
            log::info!("not elevated: limited mode (snapshots only)");
            return;
        }
        // Kernel module addresses (DPC attribution) need SeDebugPrivilege since Windows 11 24H2.
        enable_privilege("SeDebugPrivilege");
        enable_privilege("SeSystemProfilePrivilege");
        let sink = KernelSink::new(self.clock, self.registry.clone(), self.kdata.clone());
        match Session::start(&kernel_etw::spec(), Box::new(sink)) {
            Ok(s) => {
                self.kernel = Some(s);
                self.kernel_err = None;
                log::info!("kernel session started");
            }
            Err(e) => {
                self.kernel_err = Some(e);
                log::warn!("kernel session failed: {e}");
            }
        }
        let sink = PresentSink::new(self.clock, self.fg.clone(), self.pdata.clone());
        match Session::start(&present_etw::spec(), Box::new(sink)) {
            Ok(s) => {
                self.frames = Some(s);
                self.frames_err = None;
                log::info!("frame session started");
            }
            Err(e) => {
                self.frames_err = Some(e);
                log::warn!("frame session failed: {e}");
            }
        }
    }

    pub fn stop_tracing(&mut self) {
        if let Some(mut s) = self.kernel.take() {
            s.stop();
        }
        if let Some(mut s) = self.frames.take() {
            s.stop();
        }
        // Pausing or quitting must not lose the play session in progress.
        let now = self.clock.now_us();
        self.end_game_session(now, None);
        self.pending.clear();
        self.frame_hold.clear();
    }

    /// Sessions can be stopped from outside (another tool, `logman stop`): notice and restart.
    fn check_sessions(&mut self, now: Micros) {
        let dead =
            self.kernel.as_ref().is_some_and(|s| !s.is_alive()) || self.frames.as_ref().is_some_and(|s| !s.is_alive());
        if dead && now - self.last_restart_us > 60_000_000 {
            self.last_restart_us = now;
            log::warn!("an ETW session ended unexpectedly, restarting tracing");
            self.stop_tracing();
            self.start_tracing();
        }
    }

    // ───────────────────────────── commands

    pub fn handle(&mut self, cmd: Cmd, shared: &Shared, sink: &dyn EngineSink) {
        match cmd {
            Cmd::Settings(s) => {
                let was_paused = self.settings.paused;
                self.settings = s;
                store::save_settings(&self.data_dir, &self.settings);
                self.store.lock().unwrap_or_else(|p| p.into_inner()).set_retention(self.settings.retention);
                if self.settings.paused && !was_paused {
                    self.stop_tracing();
                } else if !self.settings.paused && was_paused {
                    self.rebaseline(self.clock.now_us());
                    self.start_tracing();
                }
                shared.public().system = self.system_info();
                sink.settings(&self.settings);
                self.push_tray(sink, true);
            }
            Cmd::MarkRead(id) => {
                self.store.lock().unwrap_or_else(|p| p.into_inner()).mark_read(id.as_deref());
                self.push_tray(sink, true);
            }
            Cmd::Delete(id) => {
                self.store.lock().unwrap_or_else(|p| p.into_inner()).delete(&id);
                self.push_tray(sink, true);
            }
            Cmd::Clear => {
                self.store.lock().unwrap_or_else(|p| p.into_inner()).clear();
                self.push_tray(sink, true);
            }
            Cmd::TestNotification => {
                let (title, body) = i18n::test_notification(self.settings.lang);
                let ts = self.clock.now_unix_ms();
                sink.notify(&Notification {
                    toast: Toast {
                        id: format!("test-{ts}"),
                        ts,
                        severity: Severity::Major,
                        incident_id: None,
                        title: Reason::bare("toast.test"),
                        cause: None,
                    },
                    in_app: true,
                    overlay: (self.settings.notify.overlay != OverlayMode::Off).then_some(OverlayText {
                        title,
                        body,
                        severity: Severity::Major,
                    }),
                });
            }
            Cmd::Shutdown => {}
        }
    }

    // ───────────────────────────── the 1 Hz tick

    pub fn tick(&mut self, shared: &Shared, sink: &dyn EngineSink) {
        self.clock.rebase_unix();
        let now = self.clock.now_us();
        let dt = ((now - self.last_tick_us) as f32 / 1e6).clamp(0.2, 30.0);
        self.last_tick_us = now;
        self.tick_count += 1;
        let ui_active = shared.ui_active.load(std::sync::atomic::Ordering::Relaxed);

        if self.settings.paused {
            if ui_active {
                let status = self.build_status(now);
                sink.status(&status);
                shared.public().status = Some(status);
            }
            self.push_tray(sink, false);
            return;
        }

        self.collect_processes(now, dt);
        self.collect_cores();
        self.mem = system::memory();
        self.cpu_temp_c = self.cpu_temp.as_mut().and_then(CpuTemp::read).map(|t| (t * 10.0).round() / 10.0);
        self.freq_limit = self.freq.limit_pct(self.ncpu).unwrap_or(100.0);
        // NVML wakes the GPU driver: full rate only while it matters.
        if self.game.is_some() || ui_active || now - self.last_gpu_us >= GPU_IDLE_POLL_US {
            self.gpu = self.nvml.as_ref().map(|n| gpu_sample(n.read()));
            self.gpu_procs = self.nvml.as_ref().map(|n| n.process_utilization()).unwrap_or_default();
            self.last_gpu_us = now;
        }
        if ui_active {
            self.collect_net(dt); // only displayed, never used by the analysis
        } else {
            self.net_prev = None;
        }
        self.timer_res_ms = blackbox_sys::sysinfo::timer_resolution_ms().map(|t| t.0).unwrap_or(0.0);
        if self.tick_count % 30 == 0 {
            self.power_plan = system::active_power_plan();
            self.battery = system::on_battery();
        }

        let fresh = self.drain_kernel(now);
        let hitches = self.drain_frames(now, sink);
        for h in hitches {
            if let Some(g) = self.game.as_mut() {
                g.play.push_hitch(h);
            }
            self.on_hitch(h);
        }
        // After the hitches: a spike that caused a long frame is reported once, by the hitch.
        self.detect_latency(&fresh);
        self.bucket_scratch = fresh;
        self.fix_event_names(now);

        let sample = self.second_sample(now);
        self.seconds.push(sample);
        self.sample_session(now);

        self.detect_hang(now);
        self.detect_sustained(now);
        self.finalize_ready(now, sink);

        self.collect_self(now);
        self.collect_etw_stats(now, dt);
        if self.tick_count % 5 == 0 {
            self.check_sessions(now);
        }
        self.recent.retain(|(t, _)| now - t < 300_000_000);

        if ui_active {
            let status = self.build_status(now);
            sink.status(&status);
            let mut p = shared.public();
            p.status = Some(status);
            p.procs.clone_from(&self.procs);
            p.drivers.clone_from(&self.drv_prev);
            p.drivers_max.clone_from(&self.drv_session_max);
            p.live_session = self.game.as_ref().and_then(|g| self.session_stats(g)).map(|mut s| {
                s.id = "live".into();
                s
            });
        }
        if self.tick_count % 60 == 0 {
            shared.public().system = self.system_info();
        }
        self.push_tray(sink, false);
    }

    fn collect_processes(&mut self, now: Micros, dt: f32) {
        if self.snap.processes(&mut self.raw).is_err() {
            return;
        }
        let denom = dt * 1e7 * self.ncpu as f32;
        let game_pid = self.game.as_ref().map(|g| g.pid);
        // Our own process tree (the dashboard's WebView2 processes are our descendants).
        self.own_family.clear();
        self.own_family.insert(self.own_pid);
        for _ in 0..3 {
            for p in &self.raw {
                if self.own_family.contains(&p.ppid) && p.pid != 0 {
                    self.own_family.insert(p.pid);
                }
            }
        }
        self.procs.clear();
        self.prev_next.clear();
        let mut threads = 0u32;
        for p in &self.raw {
            threads += p.threads;
            let key = (p.pid, p.create_time);
            let (cpu_pct, io_bps, hard_faults, tag) = match self.prev.get(&key) {
                Some(q) => (
                    (p.cpu_100ns - q.cpu_100ns).max(0) as f32 / denom * 100.0,
                    p.io_bytes.saturating_sub(q.io_bytes) as f64 / dt as f64,
                    p.hard_faults.saturating_sub(q.hard_faults),
                    q.tag,
                ),
                None => (0.0, 0.0, 0, kb::process_tag(&p.name)),
            };
            self.prev_next.insert(
                key,
                ProcPrev { cpu_100ns: p.cpu_100ns, io_bytes: p.io_bytes, hard_faults: p.hard_faults, tag },
            );
            if p.pid == 0 {
                continue;
            }
            self.names.insert(p.pid, (p.name.clone(), now));
            let tag = if self.own_family.contains(&p.pid) {
                Some(ProcTag::Blackbox)
            } else if Some(p.pid) == game_pid {
                Some(ProcTag::Game)
            } else {
                tag
            };
            self.procs.push(ProcNow {
                pid: p.pid,
                name: p.name.clone(),
                tag,
                cpu_pct,
                io_bps,
                working_set_mb: (p.working_set >> 20) as f32,
                hard_faults,
            });
        }
        std::mem::swap(&mut self.prev, &mut self.prev_next);
        self.proc_count = self.raw.len() as u32;
        self.thread_count = threads;
        if self.tick_count % 30 == 0 {
            self.names.retain(|_, (_, seen)| now - *seen < 180_000_000);
        }
    }

    fn collect_cores(&mut self) {
        if self.snap.cores(&mut self.cores_now).is_err() {
            return;
        }
        self.per_core.clear();
        let (mut busy, mut kern, mut dpc, mut isr, mut total) = (0f64, 0f64, 0f64, 0f64, 0f64);
        for (i, c) in self.cores_now.iter().enumerate() {
            let p = self.cores_prev.get(i).copied().unwrap_or(*c);
            let d_total = ((c.kernel - p.kernel) + (c.user - p.user)).max(1) as f64;
            let d_idle = (c.idle - p.idle).max(0) as f64;
            let d_busy = (d_total - d_idle).max(0.0);
            self.per_core.push(((d_busy / d_total) * 100.0).clamp(0.0, 100.0).round() as f32);
            busy += d_busy;
            kern += ((c.kernel - p.kernel) as f64 - d_idle).max(0.0);
            dpc += (c.dpc - p.dpc).max(0) as f64;
            isr += (c.interrupt - p.interrupt).max(0) as f64;
            total += d_total;
        }
        std::mem::swap(&mut self.cores_prev, &mut self.cores_now);
        if total > 0.0 {
            self.cpu_total = (busy / total * 100.0) as f32;
            self.cpu_kernel = (kern / total * 100.0) as f32;
            self.dpc_pct = (dpc / total * 100.0) as f32;
            self.isr_pct = (isr / total * 100.0) as f32;
        }
    }

    fn collect_net(&mut self, dt: f32) {
        if let Some((rx, tx)) = system::net_octets() {
            if let Some((prx, ptx)) = self.net_prev {
                self.net_rate = (rx.saturating_sub(prx) as f64 / dt as f64, tx.saturating_sub(ptx) as f64 / dt as f64);
            }
            self.net_prev = Some((rx, tx));
        }
    }

    fn collect_self(&mut self, now: Micros) {
        let u = system::self_usage();
        if self.self_hist.len() == SELF_WINDOW {
            self.self_hist.pop_front();
        }
        self.self_hist.push_back((now, u.cpu_100ns));
        if let (Some(&(t0, c0)), Some(&(t1, c1))) = (self.self_hist.front(), self.self_hist.back()) {
            let span = (t1 - t0) as f32 / 1e6;
            if span > 0.0 {
                self.self_cpu_pct = (c1 - c0).max(0) as f32 / (span * 1e7 * self.ncpu as f32) * 100.0;
            }
        }
        self.self_usage = u;
    }

    fn collect_etw_stats(&mut self, now: Micros, dt: f32) {
        let total: u64 =
            self.kernel.as_ref().map_or(0, Session::events) + self.frames.as_ref().map_or(0, Session::events);
        self.events_rate = (total.saturating_sub(self.events_prev_total) as f32 / dt).round() as u32;
        self.events_prev_total = total;
        if now - self.last_stats_us >= 5_000_000 {
            self.last_stats_us = now;
            let lost: u64 = [&self.kernel, &self.frames]
                .iter()
                .filter_map(|s| s.as_ref())
                .map(|s| {
                    let st = s.stats();
                    st.events_lost + st.buffers_lost
                })
                .sum();
            self.lost_recent = lost.saturating_sub(self.lost_reported);
            self.lost_reported = lost;
            self.events_lost = lost;
        }
    }

    /// Moves what the kernel consumer aggregated into the ring. Returns the raw buckets drained
    /// this tick (for spike detection).
    fn drain_kernel(&mut self, now: Micros) -> Vec<KernelBucket> {
        let mut fresh = std::mem::take(&mut self.bucket_scratch);
        fresh.clear();
        let (procs, drivers, wmax, mods, bad_ts) = {
            let Ok(mut k) = self.kdata.lock() else {
                return fresh;
            };
            std::mem::swap(&mut k.buckets, &mut fresh);
            let procs: Vec<RawProcEvent> = std::mem::take(&mut k.procs);
            let drivers = k.drivers.clone();
            let wmax: Vec<(u32, u32)> = k.window_max.iter_mut().map(std::mem::take).collect();
            (procs, drivers, wmax, k.modules_available, k.bad_timestamps)
        };
        self.modules_available = mods;
        if bad_ts > self.bad_timestamps_logged && self.tick_count % 60 == 0 {
            // DPC/ISR events whose InitialTime didn't match the session clock: should stay ~0.
            log::warn!("kernel: {bad_ts} DPC/ISR events with inconsistent timestamps so far");
            self.bad_timestamps_logged = bad_ts;
        }
        for b in &fresh {
            self.kring.merge(b);
        }
        for e in procs {
            self.on_proc_event(e, now);
        }
        // Driver activity over the last second.
        let mut delta = Vec::new();
        if self.drv_session_max.len() < drivers.len() {
            self.drv_session_max.resize(drivers.len(), (0, 0));
        }
        for (i, cur) in drivers.iter().enumerate() {
            let prev = self.drv_prev.get(i).copied().unwrap_or_default();
            let mut d = cur.since(&prev);
            if let Some(&(dm, im)) = wmax.get(i) {
                d.dpc_max_us = dm;
                d.isr_max_us = im;
                let s = &mut self.drv_session_max[i];
                s.0 = s.0.max(dm);
                s.1 = s.1.max(im);
            }
            if d.dpc_count + d.isr_count > 0 {
                delta.push((i as u16, d));
            }
        }
        self.drv_prev = drivers;
        if self.drv_window.len() == DRIVER_WINDOW_S {
            self.drv_window.pop_front();
        }
        self.drv_window.push_back(delta);
        fresh
    }

    fn on_proc_event(&mut self, e: RawProcEvent, now: Micros) {
        let name: Arc<str> = if !e.image.is_empty() {
            Arc::from(e.image.as_str())
        } else if let Some((n, _)) = self.names.get(&e.pid) {
            n.clone()
        } else {
            Arc::from(format!("pid {}", e.pid))
        };
        if e.start {
            self.names.insert(e.pid, (name.clone(), now));
        }
        let parent_name = self.names.get(&e.ppid).map(|(n, _)| n.clone());
        let tag = if self.own_family.contains(&e.pid) { Some(ProcTag::Blackbox) } else { kb::process_tag(&name) };
        self.events.push(ProcEvent { t_us: e.t_us, pid: e.pid, ppid: e.ppid, start: e.start, name, parent_name, tag });
    }

    /// Kernel process events may carry a truncated image name (or none): once the process
    /// shows up in a snapshot, use its full name.
    fn fix_event_names(&mut self, now: Micros) {
        for ev in self.events.iter_mut().rev() {
            if now - ev.t_us > 15_000_000 {
                break;
            }
            let Some((full, _)) = self.names.get(&ev.pid) else {
                continue;
            };
            let truncated = ev.name.starts_with("pid ")
                || (!ev.name.to_ascii_lowercase().ends_with(".exe") && full.len() > ev.name.len());
            if truncated && **full != *ev.name && (ev.name.starts_with("pid ") || full.starts_with(&*ev.name)) {
                ev.name = full.clone();
                ev.tag = kb::process_tag(&ev.name);
            }
        }
    }

    /// Feeds the foreground app's presents to its frame tracker; manages game sessions.
    fn drain_frames(&mut self, now: Micros, sink: &dyn EngineSink) -> Vec<blackbox_core::frames::Hitch> {
        let mut frames = std::mem::take(&mut self.frame_scratch);
        frames.clear();
        if let Ok(mut p) = self.pdata.lock() {
            std::mem::swap(&mut p.frames, &mut frames);
        }
        frames.sort_unstable_by_key(|f| f.t_us);
        let fg_pid = self.fg.pid();
        // A frozen game is covered by a "ghost" window that belongs to another process:
        // the player is still in the game.
        let game_hung = self.game.as_ref().is_some_and(|g| is_window_hung(g.hwnd));

        // Focus moves: the gap while away is not a frame.
        let mut ended = false;
        if let Some(g) = self.game.as_mut() {
            if g.pid == fg_pid || game_hung {
                if g.away_since.take().is_some() {
                    g.tracker.reset_cadence(now);
                    self.frame_hold.clear();
                }
            } else {
                if g.away_since.is_none() {
                    g.away_since = Some(now);
                    g.tracker.reset_cadence(now);
                    self.frame_hold.clear();
                }
                let away = g.away_since.unwrap_or(now);
                let alive = self.names.get(&g.pid).is_some_and(|(_, seen)| now - *seen < 3_000_000);
                ended = now - away >= GAME_AWAY_US || !alive;
            }
        }
        if ended {
            self.end_game_session(now, Some(sink));
        }

        // A game is an app that keeps presenting at a steady rate for a few seconds. Judged on
        // present timestamps: ETW hands frames over in irregular batches.
        if self.game.is_none() {
            let eligible = fg_pid != 0 && !self.own_family.contains(&fg_pid) && !self.fg.is_ghost();
            if !eligible || self.candidate_pid != fg_pid {
                self.candidate_frames.clear();
                self.candidate_pid = fg_pid;
            }
            if eligible {
                self.candidate_frames.extend(frames.iter().filter(|f| f.pid == fg_pid).map(|f| f.t_us));
                if let Some(&newest) = self.candidate_frames.back() {
                    while self.candidate_frames.front().is_some_and(|&t| t < newest - GAME_SPAN_US) {
                        self.candidate_frames.pop_front();
                    }
                }
                if steady_presenter(&self.candidate_frames) {
                    let name = self.names.get(&fg_pid).map(|(n, _)| n.clone()).unwrap_or_else(|| Arc::from("game"));
                    log::info!("game session: {name} (pid {fg_pid})");
                    let hint = self.games.lock().unwrap_or_else(|p| p.into_inner()).cursor_hint(&name);
                    let play = PlaySession::new(&name, self.clock.now_unix_ms(), self.mode() == Mode::Full, hint);
                    self.game = Some(GameSession {
                        pid: fg_pid,
                        hwnd: self.fg.hwnd() as isize,
                        name,
                        tracker: FrameTracker::new(fg_pid, now),
                        started_us: now,
                        away_since: None,
                        fullscreen: false,
                        held: Vec::new(),
                        play,
                    });
                    self.candidate_frames.clear();
                }
            }
        }

        let mut hitches = Vec::new();
        let rule = HitchRule::for_sensitivity(self.settings.sensitivity);
        if let Some(g) = self.game.as_mut().filter(|g| g.pid == fg_pid) {
            self.frame_hold.extend(frames.iter().filter(|f| f.pid == g.pid).copied());
            self.frame_hold.sort_unstable_by_key(|f| f.t_us);
            // Games recreate their window when switching display modes: follow the current one.
            g.hwnd = self.fg.hwnd() as isize;
            if self.tick_count % 60 == 0 {
                let s = g.tracker.stats(now);
                log::info!("game {}: {:.0} fps, p99 {:.1} ms, {} hitches", g.name, s.fps, s.p99_ms, g.tracker.hitches);
            }
            // Several swapchains (overlays, launchers): follow the busiest one.
            let mut chains: Vec<(u64, u32)> = Vec::new();
            for f in &self.frame_hold {
                match chains.iter_mut().find(|(c, _)| *c == f.swapchain) {
                    Some((_, n)) => *n += 1,
                    None => chains.push((f.swapchain, 1)),
                }
            }
            let newest = self.frame_hold.last().map_or(now, |f| f.t_us);
            let cut = self.frame_hold.partition_point(|f| f.t_us <= newest - FRAME_HOLD_US);
            if let Some(&(chain, _)) = chains.iter().max_by_key(|(_, n)| *n) {
                for f in self.frame_hold[..cut].iter().filter(|f| f.swapchain == chain) {
                    if let Some(h) = g.tracker.push(f.t_us, &rule) {
                        hitches.push(h);
                    }
                }
            }
            self.frame_hold.drain(..cut);
            g.fullscreen = self.fg.is_fullscreen();
        }
        self.frame_scratch = frames;
        hitches
    }

    /// Statistics of the session in progress, if it is worth keeping.
    fn session_stats(&self, g: &GameSession) -> Option<SessionStats> {
        (g.play.played_sec() >= MIN_SESSION_SEC)
            .then(|| g.play.stats(&format!("{:x}", g.play.start_ts()), self.clock.now_unix_ms()))
    }

    /// Ends the game session: saves its statistics and, in smart mode, sums up what was held.
    /// `sink` is None when Blackbox itself stops (pause, exit): the session is still saved.
    pub(crate) fn end_game_session(&mut self, now: Micros, sink: Option<&dyn EngineSink>) {
        let Some(g) = self.game.take() else {
            return;
        };
        self.frame_hold.clear();
        log::info!(
            "game session ended: {} ({} hitches, {} s in the game)",
            g.name,
            g.tracker.hitches,
            g.play.played_sec()
        );
        if let Some(stats) = self.session_stats(&g) {
            let record = self.games.lock().unwrap_or_else(|p| p.into_inner()).add(stats, g.play.cursor_game());
            if let Some(sink) = sink {
                sink.session(&record);
            }
        }
        let Some(sink) = sink else { return };
        if g.held.is_empty() {
            return;
        }
        let minutes = ((now - g.started_us) / 60_000_000).max(1) as u64;
        let (title, body) = i18n::session_summary(&g.name, g.tracker.hitches, minutes, self.settings.lang);
        let ts = self.clock.now_unix_ms();
        let worst = g.held.iter().map(|i| i.severity).max().unwrap_or(Severity::Major);
        sink.notify(&Notification {
            toast: Toast {
                id: format!("session-{ts}"),
                ts,
                severity: worst,
                incident_id: g.held.first().map(|i| i.id.clone()),
                title: Reason::new(
                    "toast.session_summary",
                    params! {"app" => &*g.name, "hitches" => g.tracker.hitches, "minutes" => minutes},
                ),
                cause: None,
            },
            in_app: self.settings.notify.in_app,
            overlay: Some(OverlayText { title, body, severity: worst }),
        });
    }

    /// Samples this second of the game session (labelled later, once its frames are in).
    fn sample_session(&mut self, now: Micros) {
        let Some(g) = self.game.as_ref() else { return };
        let focused = g.away_since.is_none() || is_window_hung(g.hwnd);
        let (pid, hwnd) = (g.pid, g.hwnd);
        let input = if focused { self.input.sample(hwnd) } else { InputSample::default() };
        let (game_cpu, game_ram, game_io) = self
            .procs
            .iter()
            .find(|p| p.pid == pid)
            .map_or((0.0, 0.0, 0.0), |p| (p.cpu_pct, p.working_set_mb, (p.io_bps / 1e6) as f32));
        let horizon = self.kring.last_idx().map_or(now, |i| (i + 1) * BUCKET_US);
        let (mut dpc, mut isr, mut disk) = (0u32, 0u32, 0u32);
        for b in self.kring.range(horizon - 1_000_000, horizon) {
            dpc = dpc.max(b.dpc_max_us);
            isr = isr.max(b.isr_max_us);
            disk = disk.max(b.disk_lat_max_us());
        }
        let tick = TickSample {
            cpu_pct: self.cpu_total,
            game_cpu_pct: game_cpu,
            game_ram_mb: game_ram,
            freq_limit_pct: self.freq_limit,
            ram_used_mb: self.mem.total_mb.saturating_sub(self.mem.avail_mb) as f32,
            gpu: self.gpu.clone(),
            dpc_max_us: dpc,
            isr_max_us: isr,
            disk_lat_max_ms: disk as f32 / 1000.0,
            fullscreen: g.fullscreen,
            cpu_temp_c: self.cpu_temp_c,
            fps: None,
        };
        let Some(g) = self.game.as_mut() else { return };
        g.play.push_second(PendingSecond { t: now, focused, input, game_io_mbps: game_io, tick });
        g.play.classify(now, &g.tracker);
    }

    fn second_sample(&self, now: Micros) -> SecondSample {
        let mut by_cpu: Vec<&ProcNow> = self.procs.iter().filter(|p| p.cpu_pct >= 0.5).collect();
        by_cpu.sort_unstable_by(|a, b| b.cpu_pct.total_cmp(&a.cpu_pct));
        let mut by_io: Vec<&ProcNow> = self.procs.iter().filter(|p| p.io_bps >= 1e5).collect();
        by_io.sort_unstable_by(|a, b| b.io_bps.total_cmp(&a.io_bps));
        let mut by_mem: Vec<&ProcNow> = self.procs.iter().filter(|p| p.working_set_mb >= 50.0).collect();
        by_mem.sort_unstable_by(|a, b| b.working_set_mb.total_cmp(&a.working_set_mb));
        let gpu_of = |pid: u32| self.gpu_procs.iter().find(|(p, _)| *p == pid).map(|&(_, u)| u);
        let use_of = |p: &&ProcNow| p.to_use(gpu_of(p.pid));
        let top_gpu: Vec<ProcUse> = self
            .gpu_procs
            .iter()
            .filter(|(_, u)| *u >= 1.0)
            .take(4)
            .filter_map(|&(pid, _)| self.procs.iter().find(|p| p.pid == pid))
            .map(|p| p.to_use(gpu_of(p.pid)))
            .collect();
        let hard_faults = if self.kernel.is_some() {
            self.kring.range(now - 2_000_000, now - 1_000_000).map(|b| b.hard_faults).sum()
        } else {
            self.procs.iter().map(|p| p.hard_faults).sum()
        };
        SecondSample {
            t_us: now,
            cpu_total: self.cpu_total,
            cpu_kernel: self.cpu_kernel,
            dpc_pct: self.dpc_pct,
            isr_pct: self.isr_pct,
            mem_total_mb: self.mem.total_mb,
            mem_avail_mb: self.mem.avail_mb,
            mem_load: self.mem.load_pct,
            commit_mb: self.mem.commit_mb,
            commit_limit_mb: self.mem.commit_limit_mb,
            hard_faults,
            freq_limit_pct: self.freq_limit,
            gpu: self.gpu.clone(),
            top_cpu: by_cpu.iter().take(6).map(use_of).collect(),
            top_io: by_io.iter().take(6).map(use_of).collect(),
            top_mem: by_mem.iter().take(6).map(use_of).collect(),
            top_gpu,
            cpu_temp_c: self.cpu_temp_c,
            fg_pid: self.fg.pid(),
        }
    }

    pub fn next_incident_id(&mut self, ts: i64) -> String {
        self.next_id = self.next_id.wrapping_add(1);
        format!("{ts:x}-{:04x}", self.next_id & 0xffff)
    }

    pub fn push_tray(&mut self, sink: &dyn EngineSink, force: bool) {
        let unread = self.store.lock().map(|s| s.unread()).unwrap_or(0);
        let health = if self.settings.paused {
            Health::Idle
        } else if self.recent.iter().any(|(_, s)| *s >= Severity::Major) {
            Health::Warn
        } else {
            Health::Ok
        };
        let state = TrayState { recording: !self.settings.paused, unread, health, lang: self.settings.lang };
        if force || self.last_tray.as_ref() != Some(&state) {
            sink.tray(&state);
            self.last_tray = Some(state);
        }
    }
}

pub(crate) fn gpu_sample(r: GpuReading) -> GpuSample {
    const BITS: [(u64, GpuThrottle); 5] = [
        (0x4, GpuThrottle::SwPowerCap),
        (0x8, GpuThrottle::HwSlowdown),
        (0x20, GpuThrottle::SwThermal),
        (0x40, GpuThrottle::HwThermal),
        (0x80, GpuThrottle::HwPowerBrake),
    ];
    GpuSample {
        util_pct: r.util_pct,
        mem_util_pct: r.mem_util_pct,
        temp_c: r.temp_c,
        power_w: r.power_w,
        power_limit_w: r.power_limit_w,
        core_mhz: r.core_mhz,
        mem_mhz: r.mem_mhz,
        fan_pct: r.fan_pct,
        vram_used_mb: r.vram_used_mb,
        vram_total_mb: r.vram_total_mb,
        throttle: BITS.iter().filter(|(b, _)| r.throttle_mask & b != 0).map(|&(_, t)| t).collect(),
    }
}
