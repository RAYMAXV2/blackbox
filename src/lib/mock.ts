// Browser-only simulator of the Rust engine. Loaded lazily by ipc.ts when the
// UI does not run inside Tauri, so the whole interface can be developed and
// reviewed in a normal browser. It mimics the engine's behaviour: 1 Hz status
// only while the UI is active, incidents with full timelines, toasts, settings.

import kbJson from '../../crates/blackbox-core/kb/signatures.json'
import type { Backend } from './ipc'
import {
  EVENTS,
  type Bootstrap,
  type CauseMatch,
  type DriverCategory,
  type DriverInfo,
  type DriverLive,
  type GpuThrottle,
  type Incident,
  type IncidentKind,
  type KnowledgeEntry,
  type LiveStatus,
  type Mode,
  type ProcLive,
  type ProcTag,
  type Reason,
  type Settings,
  type Severity,
  type SystemInfo,
  type Timeline,
  type TimelineEvent,
  type Toast,
} from './contract'

const KB = kbJson as KnowledgeEntry[]

// ── tiny deterministic RNG ───────────────────────────────────────────────
let seed = 0x9e3779b9
function rnd(): number {
  seed ^= seed << 13
  seed ^= seed >>> 17
  seed ^= seed << 5
  return ((seed >>> 0) % 1_000_000) / 1_000_000
}
const rr = (a: number, b: number) => a + rnd() * (b - a)
const ri = (a: number, b: number) => Math.round(rr(a, b))
const pick = <T>(arr: readonly T[]): T => arr[Math.floor(rnd() * arr.length)]
const gauss = () => (rnd() + rnd() + rnd() + rnd() - 2) / 2
const round = (v: number, d = 0) => Math.round(v * 10 ** d) / 10 ** d

// ── event bus ────────────────────────────────────────────────────────────
type Handler = (payload: unknown) => void
const handlers = new Map<string, Set<Handler>>()
function emit(event: string, payload: unknown) {
  handlers.get(event)?.forEach((h) => h(payload))
}

// ── static world ─────────────────────────────────────────────────────────
const query = new URLSearchParams(location.search)
const START = Date.now()
const BOOT = START - (3 * 3600 + 1234) * 1000
const GAME = { pid: 18432, name: 'Cyberpunk2077.exe' }
const HAS_GAME = query.get('game') !== '0'
let mode: Mode = query.get('mode') === 'limited' ? 'limited' : 'full'
let uiActive = true

interface DriverSim {
  name: string
  friendly: string
  category: DriverCategory
  dpc: number // per second
  dpcUs: number // mean µs
  isr: number
  isrUs: number
}

const DRIVERS: DriverSim[] = [
  { name: 'nvlddmkm.sys', friendly: 'NVIDIA graphics driver', category: 'gpu', dpc: 1400, dpcUs: 9, isr: 1300, isrUs: 3 },
  { name: 'ntoskrnl.exe', friendly: 'Windows kernel', category: 'kernel', dpc: 1100, dpcUs: 3, isr: 0, isrUs: 0 },
  { name: 'HIDCLASS.SYS', friendly: 'HID input class (mouse, keyboard)', category: 'input', dpc: 1000, dpcUs: 2, isr: 0, isrUs: 0 },
  { name: 'USBXHCI.SYS', friendly: 'USB 3 host controller', category: 'usb', dpc: 900, dpcUs: 4, isr: 880, isrUs: 1.5 },
  { name: 'storport.sys', friendly: 'Storage port driver', category: 'storage', dpc: 420, dpcUs: 7, isr: 0, isrUs: 0 },
  { name: 'stornvme.sys', friendly: 'NVMe storage driver', category: 'storage', dpc: 0, dpcUs: 0, isr: 410, isrUs: 2 },
  { name: 'ndis.sys', friendly: 'Network driver interface (NDIS)', category: 'network', dpc: 380, dpcUs: 12, isr: 0, isrUs: 0 },
  { name: 'Wdf01000.sys', friendly: 'Kernel driver framework', category: 'kernel', dpc: 300, dpcUs: 5, isr: 0, isrUs: 0 },
  { name: 'dxgkrnl.sys', friendly: 'DirectX graphics kernel', category: 'gpu', dpc: 240, dpcUs: 4, isr: 0, isrUs: 0 },
  { name: 'tcpip.sys', friendly: 'TCP/IP stack', category: 'network', dpc: 220, dpcUs: 6, isr: 0, isrUs: 0 },
  { name: 'Netwtw14.sys', friendly: 'Intel Wi-Fi driver', category: 'network', dpc: 160, dpcUs: 28, isr: 150, isrUs: 4 },
  { name: 'HDAudBus.sys', friendly: 'HD Audio bus driver', category: 'audio', dpc: 95, dpcUs: 8, isr: 95, isrUs: 2 },
  { name: 'Ntfs.sys', friendly: 'NTFS file system', category: 'filesystem', dpc: 30, dpcUs: 6, isr: 0, isrUs: 0 },
  { name: 'ACPI.sys', friendly: 'ACPI power management', category: 'acpi', dpc: 12, dpcUs: 40, isr: 10, isrUs: 6 },
  { name: 'WdFilter.sys', friendly: 'Microsoft Defender filter', category: 'security', dpc: 5, dpcUs: 10, isr: 0, isrUs: 0 },
]

interface ProcSim {
  pid: number
  name: string
  cpu: number
  io: number
  ws: number
  tag: ProcTag | null
}

const PROCS: ProcSim[] = [
  { pid: GAME.pid, name: GAME.name, cpu: 17, io: 42e6, ws: 9800, tag: 'game' },
  { pid: 4120, name: 'dwm.exe', cpu: 1.6, io: 0, ws: 180, tag: null },
  { pid: 8812, name: 'chrome.exe', cpu: 1.1, io: 90e3, ws: 650, tag: null },
  { pid: 7724, name: 'Discord.exe', cpu: 0.9, io: 30e3, ws: 410, tag: null },
  { pid: 6012, name: 'steamwebhelper.exe', cpu: 0.6, io: 12e3, ws: 290, tag: null },
  { pid: 10324, name: 'Spotify.exe', cpu: 0.5, io: 40e3, ws: 240, tag: null },
  { pid: 5212, name: 'MsMpEng.exe', cpu: 0.4, io: 200e3, ws: 260, tag: 'defender' },
  { pid: 4, name: 'System', cpu: 0.4, io: 900e3, ws: 4, tag: null },
  { pid: 2280, name: 'audiodg.exe', cpu: 0.3, io: 0, ws: 34, tag: null },
  { pid: 1556, name: 'svchost.exe', cpu: 0.2, io: 10e3, ws: 48, tag: null },
  { pid: 1880, name: 'explorer.exe', cpu: 0.2, io: 6e3, ws: 220, tag: null },
  { pid: 7040, name: 'nvcontainer.exe', cpu: 0.15, io: 0, ws: 60, tag: null },
  { pid: 9932, name: 'OneDrive.exe', cpu: 0.1, io: 5e3, ws: 120, tag: 'cloud_sync' },
  { pid: 2916, name: 'WmiPrvSE.exe', cpu: 0.1, io: 0, ws: 30, tag: 'wmi' },
  { pid: 11240, name: 'blackbox.exe', cpu: 0.09, io: 0, ws: 10, tag: 'blackbox' },
  { pid: 900, name: 'csrss.exe', cpu: 0.08, io: 0, ws: 7, tag: null },
  { pid: 3368, name: 'SearchIndexer.exe', cpu: 0.05, io: 2e3, ws: 40, tag: 'search_indexer' },
  { pid: 6620, name: 'RuntimeBroker.exe', cpu: 0.02, io: 0, ws: 22, tag: null },
]

// ── settings ─────────────────────────────────────────────────────────────
const SETTINGS_KEY = 'blackbox.mock.settings'
const DEFAULTS: Settings = {
  lang: 'en',
  theme: 'dark',
  notify: { overlay: 'smart', inApp: true, minSeverity: 'major' },
  sensitivity: 'normal',
  autostart: false,
  retention: 200,
  paused: false,
}

function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY)
    if (raw) {
      const s = JSON.parse(raw) as Partial<Settings>
      return { ...DEFAULTS, ...s, notify: { ...DEFAULTS.notify, ...(s.notify ?? {}) } }
    }
  } catch {
    /* storage unavailable */
  }
  return { ...DEFAULTS, lang: query.get('lang') === 'fr' ? 'fr' : 'en' }
}

let settings = loadSettings()

function saveSettings() {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings))
  } catch {
    /* ignore */
  }
}

// ── driver statistics ────────────────────────────────────────────────────
interface DriverAgg {
  dpcCount: number
  dpcTotalUs: number
  dpcMaxUs: number
  isrCount: number
  isrTotalUs: number
  isrMaxUs: number
  histogram: number[]
}

const session = new Map<string, DriverAgg>()
const recent: Map<string, DriverAgg>[] = [] // last 10 ticks

function emptyAgg(): DriverAgg {
  return { dpcCount: 0, dpcTotalUs: 0, dpcMaxUs: 0, isrCount: 0, isrTotalUs: 0, isrMaxUs: 0, histogram: [0, 0, 0, 0, 0, 0, 0] }
}

const HIST_EDGES = [50, 100, 250, 500, 1000, 2000]
function bucketOf(us: number) {
  const i = HIST_EDGES.findIndex((e) => us < e)
  return i === -1 ? 6 : i
}

/** spikes[driverName] = µs forced this tick (incident injection) */
function tickDrivers(spikes: Record<string, number>) {
  const tick = new Map<string, DriverAgg>()
  for (const d of DRIVERS) {
    const a = emptyAgg()
    a.dpcCount = Math.round(d.dpc * rr(0.85, 1.15))
    a.isrCount = Math.round(d.isr * rr(0.85, 1.15))
    a.dpcTotalUs = a.dpcCount * d.dpcUs * rr(0.9, 1.1)
    a.isrTotalUs = a.isrCount * d.isrUs * rr(0.9, 1.1)
    a.dpcMaxUs = a.dpcCount ? Math.round(d.dpcUs * rr(3, 9) + (rnd() < 0.05 ? rr(50, 250) : 0)) : 0
    a.isrMaxUs = a.isrCount ? Math.round(d.isrUs * rr(3, 8)) : 0
    if (spikes[d.name]) a.dpcMaxUs = spikes[d.name]
    const n = a.dpcCount + a.isrCount
    a.histogram[0] = Math.round(n * 0.955)
    a.histogram[1] = Math.round(n * 0.03)
    a.histogram[2] = Math.round(n * 0.011)
    a.histogram[3] = Math.round(n * 0.003)
    a.histogram[bucketOf(Math.max(a.dpcMaxUs, a.isrMaxUs))] += 1
    tick.set(d.name, a)

    const s = session.get(d.name) ?? emptyAgg()
    s.dpcCount += a.dpcCount
    s.isrCount += a.isrCount
    s.dpcTotalUs += a.dpcTotalUs
    s.isrTotalUs += a.isrTotalUs
    s.dpcMaxUs = Math.max(s.dpcMaxUs, a.dpcMaxUs)
    s.isrMaxUs = Math.max(s.isrMaxUs, a.isrMaxUs)
    a.histogram.forEach((v, i) => (s.histogram[i] += v))
    session.set(d.name, s)
  }
  recent.push(tick)
  if (recent.length > 10) recent.shift()
  return tick
}

function driverLive(): DriverLive[] {
  const out: DriverLive[] = DRIVERS.map((d) => {
    const a = emptyAgg()
    for (const t of recent) {
      const x = t.get(d.name)!
      a.dpcCount += x.dpcCount
      a.dpcTotalUs += x.dpcTotalUs
      a.dpcMaxUs = Math.max(a.dpcMaxUs, x.dpcMaxUs)
      a.isrCount += x.isrCount
      a.isrTotalUs += x.isrTotalUs
      a.isrMaxUs = Math.max(a.isrMaxUs, x.isrMaxUs)
    }
    return {
      name: d.name,
      friendly: d.friendly,
      category: d.category,
      dpcCount: a.dpcCount,
      dpcTotalUs: Math.round(a.dpcTotalUs),
      dpcMaxUs: a.dpcMaxUs,
      isrCount: a.isrCount,
      isrTotalUs: Math.round(a.isrTotalUs),
      isrMaxUs: a.isrMaxUs,
    }
  })
  return out.sort((a, b) => b.dpcTotalUs + b.isrTotalUs - (a.dpcTotalUs + a.isrTotalUs)).slice(0, 8)
}

// ── frames ───────────────────────────────────────────────────────────────
const frames: number[] = [] // last 10 s of frame times
let hitchesSession = 0

function genFrames(spikeMs: number | null) {
  const base = 1000 / rr(136, 146)
  const n = Math.round(1000 / base)
  const out: number[] = []
  for (let i = 0; i < n; i++) {
    let ms = base + gauss() * 0.45
    if (rnd() < 0.012) ms += rr(2, 6)
    out.push(Math.max(3, ms))
  }
  if (spikeMs) out[Math.floor(n * 0.7)] = spikeMs
  frames.push(...out)
  if (frames.length > 1500) frames.splice(0, frames.length - 1500)
  return out
}

function percentile(values: number[], p: number) {
  if (!values.length) return 0
  const s = [...values].sort((a, b) => a - b)
  return s[Math.min(s.length - 1, Math.floor(p * s.length))]
}

// ── incidents ────────────────────────────────────────────────────────────
interface Gen {
  params: Record<string, string | number>
  evidence: Reason[]
  flag?: { kind: TimelineEvent['kind']; label: string; dt: number }
  disk?: boolean
  faults?: boolean
  cpu?: boolean
  dpc?: { driver: string; us: number }
}

const app = () => GAME.name
const sec = () => round(rr(0.2, 1.6), 1)
const r = (key: string, params?: Record<string, string | number>): Reason => ({ key, params })

const GENERATORS: Record<string, () => Gen> = {
  'drv.network': () => {
    const driver = pick(['Netwtw14.sys', 'ndis.sys'])
    const us = ri(1200, 3800)
    return { params: { driver, us, app: app() }, evidence: [r('ev.dpc_spike', { driver, us })], dpc: { driver, us }, flag: { kind: 'driver', label: driver, dt: 0 } }
  },
  'drv.gpu': () => {
    const driver = 'nvlddmkm.sys'
    const us = ri(900, 2600)
    return { params: { driver, us }, evidence: [r('ev.dpc_spike', { driver, us })], dpc: { driver, us }, flag: { kind: 'driver', label: driver, dt: 0 } }
  },
  'drv.storage': () => {
    const driver = 'storport.sys'
    const us = ri(800, 2100)
    return {
      params: { driver, us },
      evidence: [r('ev.dpc_spike', { driver, us }), r('ev.disk_busy', { mbps: ri(600, 1800), latMs: round(rr(8, 30), 1) })],
      dpc: { driver, us },
      disk: true,
      flag: { kind: 'driver', label: driver, dt: 0 },
    }
  },
  'drv.acpi': () => {
    const driver = 'ACPI.sys'
    const us = ri(1500, 5200)
    return { params: { driver, us }, evidence: [r('ev.dpc_spike', { driver, us })], dpc: { driver, us }, flag: { kind: 'driver', label: driver, dt: 0 } }
  },
  'drv.usb': () => {
    const driver = 'USBXHCI.SYS'
    const us = ri(700, 1700)
    return { params: { driver, us }, evidence: [r('ev.isr_spike', { driver, us })], dpc: { driver, us }, flag: { kind: 'driver', label: driver, dt: 0 } }
  },
  'drv.audio': () => {
    const driver = 'HDAudBus.sys'
    const us = ri(800, 1900)
    return { params: { driver, us }, evidence: [r('ev.dpc_spike', { driver, us })], dpc: { driver, us }, flag: { kind: 'driver', label: driver, dt: 0 } }
  },
  'drv.other': () => {
    const driver = 'Wdf01000.sys'
    const us = ri(900, 2400)
    return { params: { driver, us }, evidence: [r('ev.dpc_spike', { driver, us })], dpc: { driver, us }, flag: { kind: 'driver', label: driver, dt: 0 } }
  },
  'io.background_disk': () => {
    const process = pick(['OneDrive.exe', 'MsMpEng.exe', 'steam.exe', 'svchost.exe'])
    const mb = ri(180, 900)
    const latMs = ri(18, 120)
    return {
      params: { process, mb, latMs },
      evidence: [r('ev.disk_busy', { mbps: ri(500, 1600), latMs }), r('ev.disk_top_process', { process, mb })],
      disk: true,
    }
  },
  'io.game_streaming': () => {
    const mb = ri(300, 1200)
    const latMs = ri(12, 60)
    return {
      params: { app: app(), mb, latMs },
      evidence: [r('ev.game_io', { mb }), r('ev.disk_busy', { mbps: ri(800, 2400), latMs })],
      disk: true,
    }
  },
  'mem.paging': () => {
    const count = ri(800, 4200)
    const mb = ri(300, 900)
    return {
      params: { count, mb },
      evidence: [r('ev.hard_faults', { count }), r('ev.mem_available', { mb, pct: round((mb / 65536) * 100, 1) })],
      faults: true,
      disk: true,
    }
  },
  'proc.defender': () => {
    const process = 'MsMpEng.exe'
    const pct = ri(9, 22)
    return {
      params: { process, pct },
      evidence: [r('ev.proc_cpu', { process, pct }), r('ev.disk_top_process', { process, mb: ri(80, 400) })],
      cpu: true,
      disk: true,
    }
  },
  'proc.windows_update': () => {
    const process = pick(['TiWorker.exe', 'MoUsoCoreWorker.exe'])
    const s = sec()
    return {
      params: { process, sec: s },
      evidence: [r('ev.proc_started', { process, sec: s }), r('ev.proc_cpu', { process, pct: ri(8, 18) })],
      flag: { kind: 'process_start', label: process, dt: s * 1000 },
      cpu: true,
    }
  },
  'proc.telemetry': () => {
    const process = 'CompatTelRunner.exe'
    const s = sec()
    return {
      params: { process, sec: s },
      evidence: [r('ev.proc_started', { process, sec: s }), r('ev.disk_busy', { mbps: ri(400, 1200), latMs: ri(15, 60) })],
      flag: { kind: 'process_start', label: process, dt: s * 1000 },
      disk: true,
    }
  },
  'proc.search_indexer': () => {
    const process = 'SearchProtocolHost.exe'
    const s = sec()
    return {
      params: { process, sec: s },
      evidence: [r('ev.proc_started', { process, sec: s }), r('ev.disk_top_process', { process, mb: ri(60, 300) })],
      flag: { kind: 'process_start', label: process, dt: s * 1000 },
      disk: true,
    }
  },
  'proc.shader_precache': () => {
    const process = 'fossilize_replay.exe'
    const pct = ri(20, 45)
    return {
      params: { process, pct },
      evidence: [r('ev.proc_cpu', { process, pct }), r('ev.cpu_total', { pct: ri(82, 97) })],
      flag: { kind: 'process_start', label: process, dt: ri(800, 2500) },
      cpu: true,
    }
  },
  'proc.cloud_sync': () => {
    const process = pick(['OneDrive.exe', 'Dropbox.exe', 'GoogleDriveFS.exe'])
    return {
      params: { process },
      evidence: [r('ev.disk_top_process', { process, mb: ri(120, 600) }), r('ev.proc_cpu', { process, pct: ri(4, 12) })],
      disk: true,
    }
  },
  'proc.wmi': () => {
    const process = 'WmiPrvSE.exe'
    const pct = ri(6, 15)
    return { params: { process, pct }, evidence: [r('ev.proc_cpu', { process, pct })], cpu: true }
  },
  'proc.scheduled_task': () => {
    const process = pick(['MicrosoftEdgeUpdate.exe', 'GoogleUpdate.exe', 'sdiagnhost.exe'])
    const s = sec()
    return {
      params: { process, sec: s },
      evidence: [r('ev.proc_started', { process, sec: s })],
      flag: { kind: 'process_start', label: process, dt: s * 1000 },
      cpu: true,
    }
  },
  'proc.background_cpu': () => {
    const process = pick(['chrome.exe', 'Discord.exe', 'obs64.exe'])
    const pct = ri(12, 35)
    return { params: { process, pct }, evidence: [r('ev.proc_cpu', { process, pct })], cpu: true }
  },
  'proc.new_process': () => {
    const process = pick(['steam.exe', 'EpicWebHelper.exe', 'Battle.net.exe'])
    const s = sec()
    return {
      params: { process, sec: s },
      evidence: [r('ev.proc_started', { process, sec: s })],
      flag: { kind: 'process_start', label: process, dt: s * 1000 },
    }
  },
  'cpu.saturated': () => {
    const pct = ri(96, 100)
    return { params: { pct }, evidence: [r('ev.cpu_total', { pct }), r('ev.game_cpu', { pct: ri(55, 70) })], cpu: true }
  },
  'cpu.throttled': () => {
    const pct = ri(55, 85)
    return { params: { pct }, evidence: [r('ev.cpu_freq_limit', { pct })] }
  },
  'gpu.power_limit': () => {
    const reason: GpuThrottle = 'sw_power_cap'
    return { params: { reason }, evidence: [r('ev.gpu_throttle', { reason })] }
  },
  'gpu.thermal': () => {
    const c = ri(84, 90)
    return {
      params: { c, reason: 'sw_thermal' },
      evidence: [r('ev.gpu_temp', { c }), r('ev.gpu_throttle', { reason: 'sw_thermal' })],
    }
  },
  'gpu.vram_full': () => {
    const usedMb = ri(31400, 32400)
    const totalMb = 32607
    return { params: { usedMb, totalMb }, evidence: [r('ev.vram', { usedMb, totalMb })] }
  },
  'app.not_responding': () => {
    const s = ri(5, 14)
    return { params: { app: app(), sec: s }, evidence: [r('ev.not_responding', { sec: s })] }
  },
  'game.internal': () => ({ params: { app: app() }, evidence: [r('ev.no_external')] }),
}

const BY_KIND: Record<IncidentKind, string[]> = {
  hitch: [
    'io.background_disk', 'proc.telemetry', 'proc.defender', 'proc.search_indexer', 'proc.shader_precache', 'mem.paging',
    'cpu.saturated', 'gpu.vram_full', 'game.internal', 'io.game_streaming', 'proc.scheduled_task', 'proc.background_cpu',
    'drv.network', 'drv.gpu', 'proc.windows_update', 'proc.cloud_sync', 'proc.wmi', 'proc.new_process',
  ],
  freeze: ['io.background_disk', 'mem.paging', 'drv.storage', 'proc.defender'],
  latency: ['drv.network', 'drv.gpu', 'drv.acpi', 'drv.usb', 'drv.audio', 'drv.storage', 'drv.other'],
  hang: ['app.not_responding'],
  throttle: ['cpu.throttled', 'gpu.power_limit', 'gpu.thermal'],
  pressure: ['mem.paging', 'io.background_disk'],
}

const RULE_OUT: Record<string, () => Reason> = {
  'ro.dpc': () => ({ key: 'ro.dpc', params: { us: ri(90, 280) } }),
  'ro.disk': () => ({ key: 'ro.disk', params: { latMs: round(rr(0.2, 1.5), 1) } }),
  'ro.cpu': () => ({ key: 'ro.cpu', params: { pct: ri(25, 55) } }),
  'ro.memory': () => ({ key: 'ro.memory', params: { mb: ri(28000, 40000) } }),
  'ro.gpu_throttle': () => ({ key: 'ro.gpu_throttle' }),
  'ro.cpu_throttle': () => ({ key: 'ro.cpu_throttle' }),
  'ro.vram': () => ({ key: 'ro.vram', params: { pct: ri(35, 55) } }),
  'ro.processes': () => ({ key: 'ro.processes' }),
}

function ruledOutFor(causeId: string): Reason[] {
  const cat = causeId.split('.')[0]
  const exclude = new Set<string>()
  if (cat === 'drv') exclude.add('ro.dpc')
  if (cat === 'io' || causeId === 'mem.paging' || causeId.startsWith('proc.')) exclude.add('ro.disk')
  if (cat === 'mem') exclude.add('ro.memory')
  if (cat === 'proc') exclude.add('ro.processes')
  if (cat === 'cpu' || cat === 'proc') exclude.add('ro.cpu').add('ro.cpu_throttle')
  if (cat === 'gpu') exclude.add('ro.gpu_throttle').add('ro.vram')
  const pool = Object.keys(RULE_OUT).filter((k) => !exclude.has(k))
  const n = causeId === 'game.internal' ? pool.length : ri(3, 5)
  return pool
    .sort(() => rnd() - 0.5)
    .slice(0, n)
    .map((k) => RULE_OUT[k]())
}

function magnitudeFor(kind: IncidentKind, g: Gen): { value: number; unit: Incident['magnitude']['unit']; severity: Severity } {
  switch (kind) {
    case 'hitch': {
      const v = ri(45, 260)
      return { value: v, unit: 'ms', severity: v < 80 ? 'minor' : v < 200 ? 'major' : 'severe' }
    }
    case 'freeze':
      return { value: ri(520, 1900), unit: 'ms', severity: 'severe' }
    case 'latency': {
      const v = g.dpc?.us ?? ri(1000, 4000)
      return { value: v, unit: 'us', severity: v < 1500 ? 'minor' : v < 3000 ? 'major' : 'severe' }
    }
    case 'hang': {
      const v = Number(g.params.sec ?? ri(5, 14))
      return { value: v, unit: 's', severity: v > 10 ? 'severe' : 'major' }
    }
    case 'throttle':
      return { value: Number(g.params.pct ?? ri(60, 85)), unit: 'pct', severity: 'major' }
    case 'pressure':
      return { value: ri(94, 98), unit: 'pct', severity: 'minor' }
  }
}

function buildTimeline(ts: number, kind: IncidentKind, magnitudeMs: number | null, g: Gen, secondary: Gen[]): Timeline {
  const t0 = ts - 5000
  const t1 = ts + 1500
  const span = t1 - t0
  const bucketMs = 100
  const nb = span / bucketMs
  const marker = ts - t0
  const all = [g, ...secondary]

  // frames
  const out: { t: number; ms: number }[] = []
  if (HAS_GAME && mode !== 'limited') {
    const base = 1000 / rr(136, 146)
    const spikeStart = magnitudeMs ? marker - magnitudeMs : null
    let t = 0
    let spiked = false
    while (t < span) {
      let ms = Math.max(3, base + gauss() * 0.45 + (rnd() < 0.015 ? rr(2, 5) : 0))
      if (kind === 'latency' && !spiked && t + ms >= marker) {
        ms += rr(4, 9)
        spiked = true
      }
      if (spikeStart !== null && !spiked && t + ms >= spikeStart) {
        out.push({ t: round(spikeStart, 1), ms: round(spikeStart - t, 2) })
        out.push({ t: marker, ms: magnitudeMs! })
        t = marker
        spiked = true
        continue
      }
      t += ms
      out.push({ t: round(t, 1), ms: round(ms, 2) })
    }
  }

  const bi = (offset: number) => Math.min(nb - 1, Math.max(0, Math.floor(offset / bucketMs)))
  const near = (i: number, center: number, radius: number) => Math.abs(i - center) <= radius

  const dpcMaxUs = Array.from({ length: nb }, () => ri(20, 140))
  const isrMaxUs = Array.from({ length: nb }, () => ri(4, 30))
  const diskMbps = Array.from({ length: nb }, () => round(rr(1, 25), 1))
  const diskLatMs = Array.from({ length: nb }, () => round(rr(0.1, 0.6), 2))
  const hardFaults = Array.from({ length: nb }, () => ri(0, 4))
  const mb = bi(marker)

  for (const x of all) {
    if (x.dpc) {
      const scale = x === g ? 1 : 0.4
      dpcMaxUs[mb] = Math.max(dpcMaxUs[mb], Math.round(x.dpc.us * scale))
      if (mb > 0) dpcMaxUs[mb - 1] = Math.max(dpcMaxUs[mb - 1], ri(200, 480))
    }
    if (x.disk) {
      for (let i = 0; i < nb; i++) {
        if (near(i, mb - 3, 6)) {
          diskMbps[i] = round(rr(400, 1800) * (x === g ? 1 : 0.4), 1)
          diskLatMs[i] = round(rr(12, 110) * (x === g ? 1 : 0.35), 1)
        }
      }
    }
    if (x.faults) {
      for (let i = 0; i < nb; i++) if (near(i, mb - 2, 4)) hardFaults[i] = ri(300, 950)
    }
  }

  const cpuPct: { t: number; v: number }[] = []
  const gpuPct: { t: number; v: number }[] = []
  const cpuSpike = all.some((x) => x.cpu)
  for (let s = 0; s * 1000 < span; s++) {
    const atMarker = Math.floor(marker / 1000) === s
    cpuPct.push({ t: s * 1000, v: round(cpuSpike && Math.abs(s - Math.floor(marker / 1000)) <= 1 ? rr(62, 99) : rr(21, 31), 1) })
    gpuPct.push({ t: s * 1000, v: round(atMarker && (kind === 'hitch' || kind === 'freeze') ? rr(58, 76) : rr(94, 99), 1) })
  }

  const events: TimelineEvent[] = []
  for (const x of all) {
    if (x.flag) events.push({ t: Math.max(0, marker - x.flag.dt), kind: x.flag.kind, label: x.flag.label, flagged: x === g })
  }
  const noise = ['conhost.exe', 'backgroundTaskHost.exe', 'RuntimeBroker.exe', 'smartscreen.exe']
  for (let i = 0; i < ri(1, 3); i++) {
    events.push({ t: ri(200, span - 300), kind: pick(['process_start', 'process_end'] as const), label: pick(noise), flagged: false })
  }
  events.sort((a, b) => a.t - b.t)

  return { t0, t1, markerTs: ts, bucketMs, frames: out, dpcMaxUs, isrMaxUs, diskMbps, diskLatMs, hardFaults, cpuPct, gpuPct, events }
}

const full = new Map<string, Incident>()
let order: string[] = [] // newest first
let sessionCount = 0
let lastIncidentTs = 0
let lastSeverity: Severity = 'minor'

function pickKind(): IncidentKind {
  const r = rnd()
  if (r < 0.58) return 'hitch'
  if (r < 0.78) return 'latency'
  if (r < 0.85) return 'freeze'
  if (r < 0.9) return 'hang'
  if (r < 0.95) return 'throttle'
  return 'pressure'
}

function makeIncident(ts: number, kind = pickKind()): Incident {
  const pool = BY_KIND[kind]
  const mainId = pick(pool)
  const g = GENERATORS[mainId]()
  const others = pool.filter((id) => id !== mainId && id !== 'game.internal').sort(() => rnd() - 0.5)
  const nSec = mainId === 'game.internal' ? 0 : ri(0, 2)
  const secIds = others.slice(0, nSec)
  const secGens = secIds.map((id) => GENERATORS[id]())
  const mag = magnitudeFor(kind, g)
  const conf = mainId === 'game.internal' ? round(rr(0.3, 0.45), 2) : round(rr(0.62, 0.96), 2)
  const causes: CauseMatch[] = [
    { id: mainId, confidence: conf, params: g.params, evidence: g.evidence },
    ...secIds.map((id, i) => ({ id, confidence: round(rr(0.18, 0.44), 2), params: secGens[i].params, evidence: secGens[i].evidence })),
  ]
  const magnitudeMs = kind === 'hitch' || kind === 'freeze' ? mag.value : null
  const id = `${ts.toString(36)}-${Math.floor(rnd() * 1e6).toString(36)}`
  return {
    id,
    ts,
    kind,
    severity: mag.severity,
    magnitude: { value: mag.value, unit: mag.unit },
    app: kind === 'latency' || kind === 'pressure' ? null : { pid: GAME.pid, name: GAME.name },
    causes,
    ruledOut: ruledOutFor(mainId),
    read: false,
    timeline: buildTimeline(ts, kind, magnitudeMs, g, secGens),
  }
}

function summary(i: Incident): Incident {
  const { timeline: _omit, ...rest } = i
  return rest
}

function addIncident(inc: Incident) {
  full.set(inc.id, inc)
  order = [inc.id, ...order].slice(0, settings.retention)
  for (const k of full.keys()) if (!order.includes(k)) full.delete(k)
}

// seed history
for (const [ago, kind] of [
  [7200, 'hitch'],
  [2820, 'latency'],
  [1560, 'hitch'],
  [660, 'freeze'],
  [180, 'hitch'],
] as const) {
  const inc = makeIncident(START - ago * 1000, kind)
  inc.read = ago > 1000
  addIncident(inc)
}

// ── live status ──────────────────────────────────────────────────────────
let last: LiveStatus | null = null
let nextIncidentAt = START + rr(8, 14) * 1000
const rx = { v: 0 }

function tick() {
  const ts = Date.now()
  const paused = settings.paused
  const effMode: Mode = paused ? 'paused' : mode
  const tracing = effMode === 'full'

  // incidents
  let spikeMs: number | null = null
  const spikes: Record<string, number> = {}
  if (!paused && ts >= nextIncidentAt) {
    nextIncidentAt = ts + rr(20, 40) * 1000
    const kind = tracing ? pickKind() : pick(['hang', 'throttle', 'pressure'] as const)
    const inc = makeIncident(ts - 1600, kind)
    addIncident(inc)
    sessionCount++
    lastIncidentTs = ts
    lastSeverity = inc.severity
    if (inc.kind === 'hitch' || inc.kind === 'freeze') {
      spikeMs = inc.magnitude.value
      hitchesSession++
    }
    const main = inc.causes[0]
    const drv = main?.params.driver
    if (inc.kind === 'latency' && typeof drv === 'string') spikes[drv] = inc.magnitude.value
    emit(EVENTS.incident, summary(inc))
    const toast: Toast = {
      id: `t-${inc.id}`,
      ts,
      severity: inc.severity,
      incidentId: inc.id,
      title: {
        key: 'toast.incident',
        params: { kind: inc.kind, value: inc.magnitude.value, unit: inc.magnitude.unit, app: inc.app?.name ?? '' },
      },
      cause: inc.causes[0] ?? null,
    }
    emit(EVENTS.toast, toast)
  }

  const drivers = tracing ? tickDrivers(spikes) : null
  const gameFrames = tracing && HAS_GAME ? genFrames(spikeMs) : []

  // cpu
  const perCore = Array.from({ length: 16 }, (_, i) => {
    const gameCore = HAS_GAME && i % 2 === 0 && i < 12
    return round(Math.min(100, Math.max(0, (gameCore ? rr(32, 66) : rr(2, 14)) + gauss() * 4)), 0)
  })
  const totalPct = round(perCore.reduce((a, b) => a + b, 0) / perCore.length, 1)

  // processes
  const processes: ProcLive[] = PROCS.filter((p) => HAS_GAME || p.tag !== 'game')
    .map((p) => ({
      pid: p.pid,
      name: p.name,
      cpuPct: round(p.cpu * rr(0.6, 1.4), 2),
      ioBps: Math.round(p.io * rr(0.3, 1.6)),
      workingSetMb: round(p.ws * rr(0.98, 1.02), 1),
      tag: p.tag,
    }))
    .sort((a, b) => b.cpuPct - a.cpuPct)

  // kernel
  let dpcCount = 0
  let isrCount = 0
  let dpcTime = 0
  let isrTime = 0
  let dpcMax = 0
  let isrMax = 0
  let worst = ''
  if (drivers) {
    for (const [name, a] of drivers) {
      dpcCount += a.dpcCount
      isrCount += a.isrCount
      dpcTime += a.dpcTotalUs
      isrTime += a.isrTotalUs
      if (a.dpcMaxUs > dpcMax) {
        dpcMax = a.dpcMaxUs
        worst = name
      }
      isrMax = Math.max(isrMax, a.isrMaxUs)
    }
  }

  const gpuThrottle: GpuThrottle[] = rnd() < 0.18 ? ['sw_power_cap'] : []
  const memUsed = 21400 + gauss() * 300 + (HAS_GAME ? 0 : -9000)
  const sinceIncident = (ts - lastIncidentTs) / 1000

  let health: LiveStatus['health'] = 'ok'
  const reasons: Reason[] = []
  if (effMode === 'paused') {
    health = 'idle'
    reasons.push({ key: 'hr.paused' })
  } else {
    if (effMode === 'limited') {
      health = 'warn'
      reasons.push({ key: 'hr.limited' })
    }
    if (lastIncidentTs && sinceIncident < 45) {
      health = lastSeverity === 'severe' && sinceIncident < 20 ? 'bad' : 'warn'
      reasons.push({ key: 'hr.incidents_recent', params: { count: 1 } })
    }
    if (dpcMax >= 1000) {
      health = health === 'bad' ? 'bad' : 'warn'
      reasons.push({ key: 'hr.dpc_high', params: { driver: worst, us: dpcMax } })
    }
    if (gpuThrottle.length && health === 'ok' && rnd() < 0.3) {
      reasons.push({ key: 'hr.gpu_throttle', params: { reason: gpuThrottle[0] } })
    }
  }

  rx.v = rx.v * 0.7 + rr(20e3, 2.4e6) * 0.3
  const s10 = frames.slice(-1400)
  const lastSec = gameFrames.length ? gameFrames : frames.slice(-140)
  const avg = lastSec.length ? lastSec.reduce((a, b) => a + b, 0) / lastSec.length : 0
  const p99 = percentile(s10, 0.99)

  const status: LiveStatus = {
    ts,
    mode: effMode,
    health,
    healthReasons: reasons,
    overhead: {
      cpuPct: round(paused ? 0 : rr(0.06, 0.14), 2),
      workingSetMb: round(paused ? 7.1 : rr(9.4, 10.2), 1),
      privateMb: round(paused ? 4.2 : rr(5.8, 6.4), 1),
      eventsPerSec: tracing ? Math.round(dpcCount + isrCount + rr(300, 900)) : 0,
      eventsLost: 0,
      uptimeSec: Math.round((ts - START) / 1000) + 742,
    },
    cpu: {
      name: 'AMD Ryzen 7 9800X3D 8-Core Processor',
      totalPct,
      kernelPct: round(totalPct * rr(0.12, 0.2), 1),
      perCore,
      freqLimitPct: 100,
      throttled: false,
      processes: 312,
      threads: 4381 + ri(-20, 20),
    },
    memory: {
      totalMb: 65536,
      usedMb: Math.round(memUsed),
      availableMb: Math.round(65536 - memUsed),
      loadPct: round((memUsed / 65536) * 100, 0),
      commitMb: Math.round(memUsed * 1.31),
      commitLimitMb: 75264,
      hardFaultsPerSec: tracing ? ri(0, 9) : 0,
    },
    gpu: {
      name: 'NVIDIA GeForce RTX 5090',
      utilPct: HAS_GAME ? ri(93, 99) : ri(2, 9),
      memUtilPct: HAS_GAME ? ri(48, 62) : ri(1, 4),
      tempC: HAS_GAME ? ri(62, 68) : ri(36, 40),
      powerW: HAS_GAME ? ri(420, 488) : ri(28, 36),
      powerLimitW: 575,
      coreMhz: HAS_GAME ? ri(2680, 2850) : 600,
      memMhz: 14001,
      fanPct: HAS_GAME ? ri(44, 52) : 0,
      vramUsedMb: HAS_GAME ? ri(13800, 14600) : ri(1800, 2100),
      vramTotalMb: 32607,
      throttle: HAS_GAME ? gpuThrottle : ['gpu_idle'],
    },
    disks: tracing
      ? [
          { id: 0, label: 'Disk 0', readBps: ri(0, 2e6), writeBps: ri(20e3, 1.4e6), iops: ri(20, 260), latAvgMs: round(rr(0.08, 0.3), 2), latMaxMs: round(rr(0.6, 2.4), 1) },
          { id: 1, label: 'Disk 1', readBps: HAS_GAME ? ri(4e6, 60e6) : ri(0, 1e5), writeBps: ri(0, 2e5), iops: HAS_GAME ? ri(180, 900) : ri(0, 20), latAvgMs: round(rr(0.1, 0.4), 2), latMaxMs: round(rr(0.9, 3.5), 1) },
        ]
      : [],
    net: { rxBps: Math.round(rx.v), txBps: Math.round(rx.v * rr(0.05, 0.12)) },
    kernel: tracing
      ? {
          dpcPerSec: dpcCount,
          isrPerSec: isrCount,
          dpcMaxUs: dpcMax,
          isrMaxUs: isrMax,
          dpcTimePct: round((dpcTime / 16e6) * 100, 2),
          isrTimePct: round((isrTime / 16e6) * 100, 2),
          hardFaultsPerSec: ri(0, 9),
          timerResolutionMs: 1,
        }
      : { dpcPerSec: 0, isrPerSec: 0, dpcMaxUs: 0, isrMaxUs: 0, dpcTimePct: 0, isrTimePct: 0, hardFaultsPerSec: 0, timerResolutionMs: 1 },
    drivers: tracing ? driverLive() : [],
    processes: processes.slice(0, 12),
    game:
      tracing && HAS_GAME
        ? {
            pid: GAME.pid,
            name: GAME.name,
            fps: round(gameFrames.length, 0),
            frameTimeMs: round(avg, 2),
            p99Ms: round(p99, 2),
            low1Fps: round(1000 / Math.max(1, p99), 0),
            hitches: hitchesSession,
            sessionSec: Math.round((ts - START) / 1000) + 1834,
            frames: frames.slice(-240).map((v) => round(v, 2)),
          }
        : null,
    counters: {
      incidentsTotal: order.length,
      incidentsSession: sessionCount,
      unread: order.reduce((n, id) => n + (full.get(id)?.read ? 0 : 1), 0),
    },
  }
  last = status
  if (uiActive) emit(EVENTS.status, status)
}

// warm up so the first bootstrap already has 10 s of driver/frame history
for (let i = 0; i < 10; i++) {
  tickDrivers({})
  if (HAS_GAME) genFrames(null)
}
tick()
setInterval(tick, 1000)

function systemInfo(): SystemInfo {
  const elevated = mode !== 'limited'
  return {
    version: '0.1.0',
    os: 'Windows 11 Pro 25H2',
    build: '26200.6899',
    cpu: 'AMD Ryzen 7 9800X3D 8-Core Processor',
    cores: 8,
    threads: 16,
    gpu: 'NVIDIA GeForce RTX 5090',
    ramMb: 65536,
    elevated,
    bootTs: BOOT,
    checks: [
      { id: 'chk.elevation', state: elevated ? 'ok' : 'warn', params: {} },
      { id: 'chk.etw_kernel', state: elevated ? 'ok' : 'warn', params: elevated ? {} : { error: 'Access denied (admin rights required).' } },
      { id: 'chk.etw_presents', state: elevated ? 'ok' : 'warn', params: elevated ? {} : { error: 'Access denied (admin rights required).' } },
      { id: 'chk.power_plan', state: 'ok', params: { name: 'Balanced' } },
      { id: 'chk.on_battery', state: 'info', params: {} },
      { id: 'chk.game_mode', state: 'ok', params: { enabled: 'yes' } },
      { id: 'chk.hags', state: 'ok', params: { enabled: 'yes' } },
      { id: 'chk.vbs', state: 'warn', params: { hvci: 'yes' } },
      { id: 'chk.timer_resolution', state: 'info', params: { currentMs: 1, minMs: 0.5, maxMs: 15.625 } },
      { id: 'chk.nvml', state: 'ok', params: { available: 'yes' } },
    ],
  }
}

function allProcesses(): ProcLive[] {
  const base = last?.processes ?? []
  const extra = ['sihost.exe', 'ctfmon.exe', 'fontdrvhost.exe', 'lsass.exe', 'services.exe', 'winlogon.exe', 'dllhost.exe', 'TextInputHost.exe', 'SecurityHealthService.exe', 'spoolsv.exe']
  return [
    ...base,
    ...extra.map((name, i) => ({ pid: 600 + i * 44, name, cpuPct: round(rr(0, 0.05), 2), ioBps: 0, workingSetMb: round(rr(4, 60), 1), tag: null })),
  ]
}

function driverInfos(): DriverInfo[] {
  return DRIVERS.map((d) => {
    const s = session.get(d.name) ?? emptyAgg()
    return {
      name: d.name,
      friendly: d.friendly,
      category: d.category,
      dpcCount: s.dpcCount,
      dpcTotalUs: Math.round(s.dpcTotalUs),
      dpcMaxUs: s.dpcMaxUs,
      isrCount: s.isrCount,
      isrTotalUs: Math.round(s.isrTotalUs),
      isrMaxUs: s.isrMaxUs,
      loaded: true,
      histogram: s.histogram.slice(),
    }
  }).sort((a, b) => b.dpcTotalUs + b.isrTotalUs - (a.dpcTotalUs + a.isrTotalUs))
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

async function invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  await sleep(12) // a hint of IPC latency
  switch (cmd) {
    case 'get_bootstrap': {
      const b: Bootstrap = {
        status: last,
        system: systemInfo(),
        settings,
        incidents: order.map((id) => summary(full.get(id)!)),
        kb: KB,
      }
      return structuredClone(b)
    }
    case 'get_incident': {
      const inc = full.get(String(args?.id))
      return inc ? structuredClone(inc) : null
    }
    case 'mark_read': {
      const id = (args?.id ?? null) as string | null
      for (const [k, v] of full) if (id === null || k === id) v.read = true
      return null
    }
    case 'delete_incident':
      full.delete(String(args?.id))
      order = order.filter((x) => x !== args?.id)
      return null
    case 'clear_incidents':
      full.clear()
      order = []
      return null
    case 'export_incident':
      return `C:\\Users\\you\\AppData\\Local\\Blackbox\\exports\\incident-${String(args?.id)}.json`
    case 'get_drivers':
      return mode === 'full' && !settings.paused ? driverInfos() : []
    case 'get_processes':
      return allProcesses()
    case 'get_settings':
      return settings
    case 'set_settings': {
      settings = structuredClone(args?.settings as Settings)
      saveSettings()
      emit(EVENTS.settings, settings)
      return settings
    }
    case 'set_ui_active':
      uiActive = Boolean(args?.active)
      return null
    case 'restart_elevated':
      await sleep(900)
      mode = 'full'
      tick()
      return null
    case 'test_notification': {
      const toast: Toast = { id: `t-test-${Date.now()}`, ts: Date.now(), severity: 'minor', incidentId: null, title: { key: 'toast.test' }, cause: null }
      emit(EVENTS.toast, toast)
      return null
    }
    case 'open_data_folder':
      console.info('[mock] open_data_folder')
      return null
    default:
      throw new Error(`[mock] unknown command ${cmd}`)
  }
}

export const backend: Backend = {
  invoke,
  async listen(event, handler) {
    let set = handlers.get(event)
    if (!set) handlers.set(event, (set = new Set()))
    set.add(handler)
    return () => set!.delete(handler)
  },
}
