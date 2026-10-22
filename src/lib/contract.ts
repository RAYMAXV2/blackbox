// Data contract between the Rust engine (src-tauri) and the Vue UI.
// Rust mirrors these types with serde (rename_all = "camelCase").
// Any change here must be mirrored in crates/blackbox-core/src/model.rs.
//
// Units are always in the field name or documented: Ms = milliseconds,
// Us = microseconds, Bps = bytes per second, Mb = mebibytes, Pct = 0..100.
// Timestamps (`ts`, `t0`, ...) are unix epoch milliseconds.

export type Lang = 'en' | 'fr'
export type Theme = 'dark' | 'light' | 'system'
export type Health = 'ok' | 'warn' | 'bad' | 'idle'
/** full = kernel tracing active, limited = not elevated (snapshots only), paused = user paused */
export type Mode = 'full' | 'limited' | 'paused'

/** A translatable message: the UI owns the strings for every key (see KEYS below). */
export interface Reason {
  key: string
  params?: Record<string, string | number>
}

// ───────────────────────────── live status (event `bb:status`, 1 Hz, only while the UI is active)

export interface LiveStatus {
  ts: number
  mode: Mode
  health: Health
  healthReasons: Reason[] // keys: hr.*
  overhead: Overhead
  cpu: CpuLive
  memory: MemLive
  gpu: GpuLive | null
  disks: DiskLive[]
  net: NetLive
  kernel: KernelLive
  drivers: DriverLive[] // top 8 by DPC+ISR time over the last 10 s
  processes: ProcLive[] // top 12 by CPU over the last second
  game: GameLive | null // foreground app that is presenting frames
  counters: Counters
}

/** What Blackbox itself costs. Shown proudly in the UI. */
export interface Overhead {
  cpuPct: number // recorder process, % of the whole machine
  workingSetMb: number
  privateMb: number
  eventsPerSec: number // ETW events handled per second
  eventsLost: number // since start (should stay 0)
  uptimeSec: number
}

export interface CpuLive {
  name: string
  totalPct: number
  kernelPct: number
  perCore: number[] // one entry per logical processor, 0..100
  freqLimitPct: number // current frequency limit vs max (100 = no limit)
  throttled: boolean
  processes: number
  threads: number
}

export interface MemLive {
  totalMb: number
  usedMb: number
  availableMb: number
  loadPct: number
  commitMb: number
  commitLimitMb: number
  hardFaultsPerSec: number
}

export interface GpuLive {
  name: string
  utilPct: number
  memUtilPct: number
  tempC: number | null
  powerW: number | null
  powerLimitW: number | null
  coreMhz: number | null
  memMhz: number | null
  fanPct: number | null
  vramUsedMb: number
  vramTotalMb: number
  throttle: GpuThrottle[] // empty = running free
}

export type GpuThrottle =
  | 'gpu_idle'
  | 'app_clocks'
  | 'sw_power_cap'
  | 'hw_slowdown'
  | 'sync_boost'
  | 'sw_thermal'
  | 'hw_thermal'
  | 'hw_power_brake'
  | 'display_clocks'

export interface DiskLive {
  id: number // physical disk number
  label: string // e.g. "Disk 0"
  readBps: number
  writeBps: number
  iops: number
  latAvgMs: number
  latMaxMs: number
}

export interface NetLive {
  rxBps: number
  txBps: number
}

export interface KernelLive {
  dpcPerSec: number
  isrPerSec: number
  dpcMaxUs: number // worst single DPC over the last second
  isrMaxUs: number
  dpcTimePct: number // share of total CPU time spent in DPCs
  isrTimePct: number
  hardFaultsPerSec: number
  timerResolutionMs: number // current system timer resolution
}

export type DriverCategory =
  | 'gpu'
  | 'network'
  | 'storage'
  | 'usb'
  | 'audio'
  | 'acpi'
  | 'kernel'
  | 'filesystem'
  | 'security'
  | 'input'
  | 'other'

export interface DriverLive {
  name: string // file name, e.g. "nvlddmkm.sys"
  friendly: string | null // e.g. "NVIDIA graphics driver"
  category: DriverCategory
  dpcCount: number
  dpcTotalUs: number
  dpcMaxUs: number
  isrCount: number
  isrTotalUs: number
  isrMaxUs: number
}

/** Session-wide driver statistics (command get_drivers). */
export interface DriverInfo extends DriverLive {
  loaded: boolean
  /** latency histogram of DPC+ISR durations, buckets: <50, <100, <250, <500, <1000, <2000, >=2000 µs */
  histogram: number[]
}

export interface ProcLive {
  pid: number
  name: string
  cpuPct: number // % of the whole machine
  ioBps: number // read+write+other transfer, all devices
  workingSetMb: number
  tag: ProcTag | null // known noisy process
}

export type ProcTag =
  | 'defender'
  | 'windows_update'
  | 'telemetry'
  | 'search_indexer'
  | 'shader_precache'
  | 'cloud_sync'
  | 'wmi'
  | 'game'
  | 'blackbox'

export interface GameLive {
  pid: number
  name: string
  fps: number
  frameTimeMs: number // average over the last second
  p99Ms: number // 99th percentile frame time over the last 10 s
  low1Fps: number // 1% low FPS over the last 10 s
  hitches: number // hitches detected in this session
  sessionSec: number
  frames: number[] // last <= 240 frame times in ms, oldest first
}

export interface Counters {
  incidentsTotal: number
  incidentsSession: number // since Blackbox started
  unread: number
}

// ───────────────────────────── incidents (event `bb:incident` carries a summary without timeline)

export type IncidentKind =
  | 'hitch' // one frame took far longer than usual
  | 'freeze' // a frame took >= 500 ms
  | 'latency' // a driver blocked the CPU (DPC/ISR) long enough to cause audio crackle / input lag
  | 'hang' // foreground app stopped responding
  | 'throttle' // sustained CPU/GPU throttling
  | 'pressure' // sustained memory/disk pressure

export type Severity = 'minor' | 'major' | 'severe'

export interface Incident {
  id: string
  ts: number
  kind: IncidentKind
  severity: Severity
  magnitude: { value: number; unit: 'ms' | 'us' | 's' | 'pct' }
  app: { pid: number; name: string } | null
  causes: CauseMatch[] // sorted by confidence, highest first. causes[0] is the verdict.
  ruledOut: Reason[] // keys: ro.*
  read: boolean
  timeline?: Timeline // only present in get_incident(id)
}

export interface CauseMatch {
  id: string // KnowledgeEntry id, e.g. "proc.telemetry"
  confidence: number // 0..1
  params: Record<string, string | number> // fills {placeholders} in the KB texts
  evidence: Reason[] // keys: ev.*
}

export interface Timeline {
  t0: number
  t1: number
  markerTs: number // when the incident happened
  bucketMs: number // resolution of the per-bucket series (100)
  /** per frame: t = ms offset from t0, ms = frame time */
  frames: { t: number; ms: number }[]
  // per-bucket series, length = (t1 - t0) / bucketMs
  dpcMaxUs: number[]
  isrMaxUs: number[]
  diskMbps: number[]
  diskLatMs: number[]
  hardFaults: number[]
  // per-second series: t = ms offset from t0
  cpuPct: { t: number; v: number }[]
  gpuPct: { t: number; v: number }[]
  events: TimelineEvent[]
}

export interface TimelineEvent {
  t: number // ms offset from t0
  kind: 'process_start' | 'process_end' | 'driver' | 'foreground' | 'note'
  label: string // process / driver name, already human readable
  flagged: boolean // part of the verdict
}

// ───────────────────────────── knowledge base: cause -> consequence -> fix

export type Consequence = 'stutter' | 'freeze' | 'audio_crackle' | 'input_lag' | 'fps_drop' | 'slow_app'

export type KbCategory = 'driver' | 'disk' | 'memory' | 'process' | 'cpu' | 'gpu' | 'app' | 'system'

export interface Localized {
  en: string
  fr: string
}

export interface KnowledgeEntry {
  id: string
  category: KbCategory
  consequences: Consequence[]
  title: Localized // may contain {placeholders}, see CauseMatch.params
  explain: Localized
  fixes: Localized[]
}

// ───────────────────────────── system

export interface SystemInfo {
  version: string // Blackbox version
  os: string // e.g. "Windows 11 Pro 25H2"
  build: string // e.g. "26200.1234"
  cpu: string
  cores: number
  threads: number
  gpu: string | null
  ramMb: number
  elevated: boolean
  bootTs: number
  checks: HealthCheck[]
}

export interface HealthCheck {
  id: string // keys: chk.*
  state: 'ok' | 'warn' | 'info'
  params: Record<string, string | number>
}

export interface Settings {
  lang: Lang
  theme: Theme
  notify: {
    overlay: 'off' | 'smart' | 'always' // native mini overlay; smart = hold while a fullscreen game runs, then summarize
    inApp: boolean
    minSeverity: Severity
  }
  sensitivity: 'low' | 'normal' | 'high'
  autostart: boolean
  retention: number // incidents kept on disk
  paused: boolean
}

/** Returned once when the UI opens (command get_bootstrap). */
export interface Bootstrap {
  status: LiveStatus | null
  system: SystemInfo
  settings: Settings
  incidents: Incident[] // newest first, without timeline
  kb: KnowledgeEntry[]
}

export interface Toast {
  id: string
  ts: number
  severity: Severity
  incidentId: string | null
  title: Reason // keys: toast.*
  cause: CauseMatch | null // when set, the UI renders the KB title of cause.id as the body
}

// ───────────────────────────── per-game play sessions (Games tab)

/**
 * Statistics of one play session (or the average of several, see GameRecord.last3).
 * A session lasts while the game runs; time spent out of the game (alt-tab) is excluded.
 * GPU fields are null without NVML (non-NVIDIA). CPU temperature is not measured in v0.1.
 */
export interface SessionStats {
  id: string // session id, or "avg3" for an average
  game: string // exe name, e.g. "Overwatch.exe"
  sessions: number // 1, or how many sessions were averaged
  startTs: number
  endTs: number
  durationSec: number // time actually spent in the game
  fullTracing: boolean // false = limited mode (no DPC/ISR/disk latency)

  // frames
  frames: number
  fpsAvg: number
  fpsMin1s: number // worst 1-second average FPS
  fpsMax1s: number
  frameTimeAvgMs: number
  frameTimeP50Ms: number
  frameTimeP95Ms: number
  frameTimeP99Ms: number
  low1Fps: number // 1% low: average FPS of the slowest 1% of frames
  low01Fps: number // 0.1% low

  // stability
  hitches: number // all confirmed hitches (incl. minor)
  hitchesMinor: number
  hitchesMajor: number
  hitchesSevere: number
  freezes: number // frames >= 500 ms
  hitchesPerHour: number
  incidents: number // incidents recorded for this game during the session(s)
  topCauses: { id: string; count: number }[] // KB ids, most frequent first (max 5)

  // CPU and memory
  cpuAvgPct: number
  cpuMaxPct: number
  gameCpuAvgPct: number // the game process, % of the whole machine
  cpuFreqLimitMinPct: number // 100 = never throttled
  ramUsedAvgMb: number
  ramUsedMaxMb: number
  gameRamMaxMb: number // game working set peak

  // GPU (NVIDIA / NVML)
  gpuUtilAvgPct: number | null
  gpuUtilMaxPct: number | null
  gpuTempAvgC: number | null
  gpuTempMaxC: number | null
  gpuPowerAvgW: number | null
  gpuPowerMaxW: number | null
  gpuClockAvgMhz: number | null
  gpuFanAvgPct: number | null
  vramAvgMb: number | null
  vramMaxMb: number | null
  gpuThrottlePct: number | null // share of seconds with a power/thermal limit active

  // system latency (full tracing only, else 0)
  dpcMaxUs: number
  isrMaxUs: number
  diskLatMaxMs: number
  fullscreenPct: number
}

export interface GameRecord {
  name: string // exe name
  sessions: SessionStats[] // newest first, up to 20
  last3: SessionStats | null // duration-weighted average of the newest 3 sessions (fewer if fewer)
  live: SessionStats | null // the session in progress, if this game is being played right now
  totalSessions: number
  totalPlaySec: number
  lastPlayedTs: number
}

// ───────────────────────────── IPC surface

export const EVENTS = {
  status: 'bb:status', // LiveStatus
  incident: 'bb:incident', // Incident (no timeline)
  toast: 'bb:toast', // Toast
  settings: 'bb:settings', // Settings
  session: 'bb:session', // GameRecord: a play session was saved for this game
} as const

/** Tauri command names (invoke) and their signatures. */
export interface Commands {
  get_bootstrap: () => Bootstrap
  get_incident: (args: { id: string }) => Incident | null
  mark_read: (args: { id: string | null }) => void // null = all
  delete_incident: (args: { id: string }) => void
  clear_incidents: () => void
  export_incident: (args: { id: string }) => string // path of the written JSON
  get_drivers: () => DriverInfo[]
  get_processes: () => ProcLive[]
  get_settings: () => Settings
  set_settings: (args: { settings: Settings }) => Settings
  set_ui_active: (args: { active: boolean }) => void // UI hidden/minimized => engine stops streaming
  restart_elevated: () => void
  test_notification: () => void
  open_data_folder: () => void
  get_games: () => GameRecord[] // most recently played first
  delete_game: (args: { name: string }) => void // forget a game's session history
}

// ───────────────────────────── translation keys the UI must provide (EN + FR)

export const KEYS = {
  /** health reasons */
  hr: [
    'hr.limited', // {}
    'hr.paused', // {}
    'hr.incidents_recent', // {count}
    'hr.dpc_high', // {driver, us}
    'hr.mem_high', // {pct}
    'hr.disk_slow', // {ms}
    'hr.gpu_throttle', // {reason}  reason = GpuThrottle id
    'hr.cpu_throttle', // {pct}
    'hr.cpu_saturated', // {pct}
    'hr.events_lost', // {count}
  ],
  /** evidence supporting a cause */
  ev: [
    'ev.dpc_spike', // {driver, us}
    'ev.isr_spike', // {driver, us}
    'ev.disk_busy', // {mbps, latMs}
    'ev.disk_top_process', // {process, mb}
    'ev.proc_started', // {process, sec}  sec = seconds before the incident
    'ev.proc_cpu', // {process, pct}
    'ev.cpu_total', // {pct}
    'ev.game_cpu', // {pct}
    'ev.hard_faults', // {count}
    'ev.mem_available', // {mb, pct}
    'ev.gpu_throttle', // {reason}
    'ev.gpu_temp', // {c}
    'ev.vram', // {usedMb, totalMb}
    'ev.cpu_freq_limit', // {pct}
    'ev.game_io', // {mb}
    'ev.not_responding', // {sec}
    'ev.no_external', // {}
  ],
  /** checks that came back clean for an incident */
  ro: [
    'ro.dpc', // {us}
    'ro.disk', // {latMs}
    'ro.cpu', // {pct}
    'ro.memory', // {mb}
    'ro.gpu_throttle', // {}
    'ro.cpu_throttle', // {}
    'ro.vram', // {pct}
    'ro.processes', // {}
  ],
  /** system health checks (SystemInfo.checks) */
  chk: [
    'chk.elevation', // {}  state warn when not elevated
    'chk.etw_kernel', // {error?}
    'chk.etw_presents', // {error?}
    'chk.power_plan', // {name}
    'chk.on_battery', // {pct}
    'chk.game_mode', // {enabled: 'yes'|'no'}
    'chk.hags', // {enabled: 'yes'|'no'|'unknown'}
    'chk.vbs', // {hvci: 'yes'|'no'}
    'chk.timer_resolution', // {currentMs, minMs, maxMs}
    'chk.nvml', // {available: 'yes'|'no'}
  ],
  /** toast titles/bodies */
  toast: [
    'toast.incident', // {kind, value, unit, app}  body: cause title is passed as CauseMatch, UI renders KB title
    'toast.session_summary', // {app, hitches, minutes}
    'toast.test', // {}
    'toast.limited', // {}
    'toast.autostart_admin', // {}  UI-side: set_settings rejected with "autostart_needs_admin"
  ],
} as const
