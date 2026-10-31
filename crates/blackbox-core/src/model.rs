//! Serializable types shared with the UI. Mirrors `src/lib/contract.ts` one to one.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Params = BTreeMap<String, ParamValue>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Num(f64),
    Text(String),
}

impl From<&str> for ParamValue {
    fn from(v: &str) -> Self {
        ParamValue::Text(v.to_owned())
    }
}
impl From<String> for ParamValue {
    fn from(v: String) -> Self {
        ParamValue::Text(v)
    }
}
macro_rules! num_param {
    ($($t:ty),*) => {$(
        impl From<$t> for ParamValue {
            fn from(v: $t) -> Self { ParamValue::Num(v as f64) }
        }
    )*};
}
num_param!(f32, f64, u8, u16, u32, u64, i32, i64, usize);

/// Builds a `Params` map: `params!{"driver" => name, "us" => 1200}`.
#[macro_export]
macro_rules! params {
    () => { $crate::model::Params::new() };
    ($($k:expr => $v:expr),+ $(,)?) => {{
        let mut p = $crate::model::Params::new();
        $( p.insert(($k).to_string(), $crate::model::ParamValue::from($v)); )+
        p
    }};
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reason {
    pub key: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: Params,
}

impl Reason {
    pub fn new(key: &str, params: Params) -> Self {
        Reason { key: key.to_owned(), params }
    }
    pub fn bare(key: &str) -> Self {
        Reason { key: key.to_owned(), params: Params::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Ok,
    Warn,
    Bad,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Full,
    Limited,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveStatus {
    pub ts: i64,
    pub mode: Mode,
    pub health: Health,
    pub health_reasons: Vec<Reason>,
    pub overhead: Overhead,
    pub cpu: CpuLive,
    pub memory: MemLive,
    pub gpu: Option<GpuLive>,
    pub disks: Vec<DiskLive>,
    pub net: NetLive,
    pub kernel: KernelLive,
    pub drivers: Vec<DriverLive>,
    pub processes: Vec<ProcLive>,
    pub game: Option<GameLive>,
    pub counters: Counters,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overhead {
    pub cpu_pct: f32,
    pub working_set_mb: f32,
    pub private_mb: f32,
    pub events_per_sec: u32,
    pub events_lost: u64,
    pub uptime_sec: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuLive {
    pub name: String,
    pub total_pct: f32,
    pub kernel_pct: f32,
    pub per_core: Vec<f32>,
    pub freq_limit_pct: f32,
    pub throttled: bool,
    pub processes: u32,
    pub threads: u32,
    /// Tctl (AMD) or package (Intel) temperature; None without the PawnIO driver.
    pub temp_c: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemLive {
    pub total_mb: u32,
    pub used_mb: u32,
    pub available_mb: u32,
    pub load_pct: f32,
    pub commit_mb: u32,
    pub commit_limit_mb: u32,
    pub hard_faults_per_sec: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuThrottle {
    GpuIdle,
    AppClocks,
    SwPowerCap,
    HwSlowdown,
    SyncBoost,
    SwThermal,
    HwThermal,
    HwPowerBrake,
    DisplayClocks,
}

impl GpuThrottle {
    pub fn id(self) -> &'static str {
        match self {
            GpuThrottle::GpuIdle => "gpu_idle",
            GpuThrottle::AppClocks => "app_clocks",
            GpuThrottle::SwPowerCap => "sw_power_cap",
            GpuThrottle::HwSlowdown => "hw_slowdown",
            GpuThrottle::SyncBoost => "sync_boost",
            GpuThrottle::SwThermal => "sw_thermal",
            GpuThrottle::HwThermal => "hw_thermal",
            GpuThrottle::HwPowerBrake => "hw_power_brake",
            GpuThrottle::DisplayClocks => "display_clocks",
        }
    }
    pub fn is_thermal(self) -> bool {
        matches!(
            self,
            GpuThrottle::SwThermal | GpuThrottle::HwThermal | GpuThrottle::HwSlowdown | GpuThrottle::HwPowerBrake
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuLive {
    pub name: String,
    pub util_pct: f32,
    pub mem_util_pct: f32,
    pub temp_c: Option<f32>,
    pub power_w: Option<f32>,
    pub power_limit_w: Option<f32>,
    pub core_mhz: Option<u32>,
    pub mem_mhz: Option<u32>,
    pub fan_pct: Option<f32>,
    pub vram_used_mb: u32,
    pub vram_total_mb: u32,
    pub throttle: Vec<GpuThrottle>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskLive {
    pub id: u32,
    pub label: String,
    pub read_bps: f64,
    pub write_bps: f64,
    pub iops: u32,
    pub lat_avg_ms: f32,
    pub lat_max_ms: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetLive {
    pub rx_bps: f64,
    pub tx_bps: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelLive {
    pub dpc_per_sec: u32,
    pub isr_per_sec: u32,
    pub dpc_max_us: u32,
    pub isr_max_us: u32,
    pub dpc_time_pct: f32,
    pub isr_time_pct: f32,
    pub hard_faults_per_sec: u32,
    pub timer_resolution_ms: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriverCategory {
    Gpu,
    Network,
    Storage,
    Usb,
    Audio,
    Acpi,
    Kernel,
    Filesystem,
    Security,
    Input,
    Other,
}

impl DriverCategory {
    /// Knowledge-base cause id for a latency spike coming from a driver of this category.
    pub fn cause_id(self) -> &'static str {
        match self {
            DriverCategory::Gpu => "drv.gpu",
            DriverCategory::Network => "drv.network",
            DriverCategory::Storage => "drv.storage",
            DriverCategory::Usb => "drv.usb",
            DriverCategory::Audio => "drv.audio",
            DriverCategory::Acpi => "drv.acpi",
            _ => "drv.other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverLive {
    pub name: String,
    pub friendly: Option<String>,
    pub category: DriverCategory,
    pub dpc_count: u64,
    pub dpc_total_us: u64,
    pub dpc_max_us: u32,
    pub isr_count: u64,
    pub isr_total_us: u64,
    pub isr_max_us: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverInfo {
    #[serde(flatten)]
    pub live: DriverLive,
    pub loaded: bool,
    pub histogram: [u64; 7],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcTag {
    Defender,
    WindowsUpdate,
    Telemetry,
    SearchIndexer,
    ShaderPrecache,
    CloudSync,
    Wmi,
    Game,
    Blackbox,
}

impl ProcTag {
    /// Knowledge-base cause id for a process of this kind being active around an incident.
    pub fn cause_id(self) -> Option<&'static str> {
        Some(match self {
            ProcTag::Defender => "proc.defender",
            ProcTag::WindowsUpdate => "proc.windows_update",
            ProcTag::Telemetry => "proc.telemetry",
            ProcTag::SearchIndexer => "proc.search_indexer",
            ProcTag::ShaderPrecache => "proc.shader_precache",
            ProcTag::CloudSync => "proc.cloud_sync",
            ProcTag::Wmi => "proc.wmi",
            ProcTag::Game | ProcTag::Blackbox => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcLive {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub io_bps: f64,
    pub working_set_mb: f32,
    pub tag: Option<ProcTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLive {
    pub pid: u32,
    pub name: String,
    pub fps: f32,
    pub frame_time_ms: f32,
    pub p99_ms: f32,
    pub low1_fps: f32,
    pub hitches: u32,
    pub session_sec: u64,
    pub frames: Vec<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counters {
    pub incidents_total: u32,
    pub incidents_session: u32,
    pub unread: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncidentKind {
    Hitch,
    Freeze,
    Latency,
    Hang,
    Throttle,
    Pressure,
}

impl IncidentKind {
    pub fn id(self) -> &'static str {
        match self {
            IncidentKind::Hitch => "hitch",
            IncidentKind::Freeze => "freeze",
            IncidentKind::Latency => "latency",
            IncidentKind::Hang => "hang",
            IncidentKind::Throttle => "throttle",
            IncidentKind::Pressure => "pressure",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Minor,
    Major,
    Severe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Ms,
    Us,
    S,
    Pct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Magnitude {
    pub value: f64,
    pub unit: Unit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRef {
    pub pid: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Incident {
    pub id: String,
    pub ts: i64,
    pub kind: IncidentKind,
    pub severity: Severity,
    pub magnitude: Magnitude,
    pub app: Option<AppRef>,
    pub causes: Vec<CauseMatch>,
    pub ruled_out: Vec<Reason>,
    pub read: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<Timeline>,
    /// The machine at the moment of the incident (detail only, like the timeline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<IncidentContext>,
}

impl Incident {
    /// Copy without the timeline and context, as sent in lists and events.
    pub fn summary(&self) -> Incident {
        Incident { timeline: None, context: None, ..self.clone() }
    }
}

/// What was running when the incident happened.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncidentContext {
    pub cpu_temp_c: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub cpu_pct: f32,
    pub gpu_pct: Option<f32>,
    pub mem_available_mb: u32,
    pub mem_load_pct: f32,
    /// Biggest users of CPU, RAM, disk and GPU at that second (up to 12), by CPU first.
    pub processes: Vec<ProcContext>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcContext {
    pub pid: u32,
    pub name: String,
    pub tag: Option<ProcTag>,
    pub cpu_pct: f32,
    pub ram_mb: f32,
    pub io_bps: f64,
    pub gpu_pct: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CauseMatch {
    pub id: String,
    pub confidence: f32,
    pub params: Params,
    pub evidence: Vec<Reason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimePoint {
    pub t: i64,
    pub v: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FramePoint {
    pub t: i64,
    pub ms: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineEventKind {
    ProcessStart,
    ProcessEnd,
    Driver,
    Foreground,
    Note,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub t: i64,
    pub kind: TimelineEventKind,
    pub label: String,
    pub flagged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub t0: i64,
    pub t1: i64,
    pub marker_ts: i64,
    pub bucket_ms: u32,
    pub frames: Vec<FramePoint>,
    pub dpc_max_us: Vec<u32>,
    pub isr_max_us: Vec<u32>,
    pub disk_mbps: Vec<f32>,
    pub disk_lat_ms: Vec<f32>,
    pub hard_faults: Vec<u32>,
    pub cpu_pct: Vec<TimePoint>,
    pub gpu_pct: Vec<TimePoint>,
    #[serde(default)]
    pub cpu_temp_c: Vec<TimePoint>,
    #[serde(default)]
    pub gpu_temp_c: Vec<TimePoint>,
    pub events: Vec<TimelineEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Localized {
    pub en: String,
    pub fr: String,
}

impl Localized {
    pub fn get(&self, lang: Lang) -> &str {
        match lang {
            Lang::En => &self.en,
            Lang::Fr => &self.fr,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Consequence {
    Stutter,
    Freeze,
    AudioCrackle,
    InputLag,
    FpsDrop,
    SlowApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KbCategory {
    Driver,
    Disk,
    Memory,
    Process,
    Cpu,
    Gpu,
    App,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub id: String,
    pub category: KbCategory,
    pub consequences: Vec<Consequence>,
    pub title: Localized,
    pub explain: Localized,
    pub fixes: Vec<Localized>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub version: String,
    pub os: String,
    pub build: String,
    pub cpu: String,
    pub cores: u32,
    pub threads: u32,
    pub gpu: Option<String>,
    pub ram_mb: u32,
    pub elevated: bool,
    pub boot_ts: i64,
    pub checks: Vec<HealthCheck>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckState {
    Ok,
    Warn,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub id: String,
    pub state: CheckState,
    pub params: Params,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Fr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayMode {
    Off,
    Smart,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sensitivity {
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifySettings {
    pub overlay: OverlayMode,
    pub in_app: bool,
    pub min_severity: Severity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub lang: Lang,
    pub theme: Theme,
    pub notify: NotifySettings,
    pub sensitivity: Sensitivity,
    pub autostart: bool,
    pub retention: u32,
    pub paused: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            lang: Lang::En,
            theme: Theme::Dark,
            notify: NotifySettings { overlay: OverlayMode::Smart, in_app: true, min_severity: Severity::Major },
            sensitivity: Sensitivity::Normal,
            autostart: false,
            retention: 200,
            paused: false,
        }
    }
}

impl Settings {
    /// Clamp values coming from the UI or from disk to sane ranges.
    pub fn sanitized(mut self) -> Settings {
        self.retention = self.retention.clamp(10, 2000);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bootstrap {
    pub status: Option<LiveStatus>,
    pub system: SystemInfo,
    pub settings: Settings,
    pub incidents: Vec<Incident>,
    pub kb: Vec<KnowledgeEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CauseCount {
    pub id: String,
    pub count: u32,
}

/// Where the time of a session went (see `activity::PhaseClassifier`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhaseTotals {
    pub gameplay_sec: u32,
    pub menu_sec: u32,
    pub loading_sec: u32,
    pub away_sec: u32,
    /// Loading screens (runs of loading seconds).
    pub loadings: u32,
    pub loading_avg_sec: f32,
    pub loading_max_sec: u32,
}

/// One play session, or the average of several (`sessions > 1`). See contract.ts.
/// The top level covers gameplay only; `all` covers every second in the game (menus and
/// loading screens included).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionStats {
    pub id: String,
    pub game: String,
    pub sessions: u32,
    /// "gameplay" or "all".
    pub scope: String,
    pub phases: PhaseTotals,
    pub all: Option<Box<SessionStats>>,
    pub start_ts: i64,
    pub end_ts: i64,
    pub duration_sec: u64,
    pub full_tracing: bool,

    pub frames: u64,
    pub fps_avg: f32,
    pub fps_min1s: f32,
    pub fps_max1s: f32,
    pub frame_time_avg_ms: f32,
    pub frame_time_p50_ms: f32,
    pub frame_time_p95_ms: f32,
    pub frame_time_p99_ms: f32,
    pub low1_fps: f32,
    pub low01_fps: f32,

    pub hitches: u32,
    pub hitches_minor: u32,
    pub hitches_major: u32,
    pub hitches_severe: u32,
    pub freezes: u32,
    pub hitches_per_hour: f32,
    pub incidents: u32,
    pub top_causes: Vec<CauseCount>,

    pub cpu_avg_pct: f32,
    pub cpu_max_pct: f32,
    pub game_cpu_avg_pct: f32,
    pub cpu_freq_limit_min_pct: f32,
    pub ram_used_avg_mb: f32,
    pub ram_used_max_mb: f32,
    pub game_ram_max_mb: f32,
    pub cpu_temp_avg_c: Option<f32>,
    pub cpu_temp_max_c: Option<f32>,

    pub gpu_util_avg_pct: Option<f32>,
    pub gpu_util_max_pct: Option<f32>,
    pub gpu_temp_avg_c: Option<f32>,
    pub gpu_temp_max_c: Option<f32>,
    pub gpu_power_avg_w: Option<f32>,
    pub gpu_power_max_w: Option<f32>,
    pub gpu_clock_avg_mhz: Option<f32>,
    pub gpu_fan_avg_pct: Option<f32>,
    pub vram_avg_mb: Option<f32>,
    pub vram_max_mb: Option<f32>,
    pub gpu_throttle_pct: Option<f32>,

    pub dpc_max_us: u32,
    pub isr_max_us: u32,
    pub disk_lat_max_ms: f32,
    pub fullscreen_pct: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRecord {
    pub name: String,
    pub sessions: Vec<SessionStats>,
    pub last3: Option<SessionStats>,
    pub live: Option<SessionStats>,
    pub total_sessions: u32,
    pub total_play_sec: u64,
    pub last_played_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Toast {
    pub id: String,
    pub ts: i64,
    pub severity: Severity,
    pub incident_id: Option<String>,
    pub title: Reason,
    pub cause: Option<CauseMatch>,
}
