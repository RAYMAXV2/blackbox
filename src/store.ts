import { computed, ref, shallowRef } from 'vue'
import { lang, lk, fill } from './i18n'
import { call, isTauri, on } from './lib/ipc'
import { SEVERITY_RANK } from './lib/format'
import {
  EVENTS,
  type CauseMatch,
  type Incident,
  type KnowledgeEntry,
  type LiveStatus,
  type Settings,
  type SystemInfo,
  type Toast,
} from './lib/contract'

export type PageId = 'live' | 'incidents' | 'drivers' | 'processes' | 'system' | 'causes' | 'settings'

export const DEFAULT_SETTINGS: Settings = {
  lang: 'en',
  theme: 'dark',
  notify: { overlay: 'smart', inApp: true, minSeverity: 'major' },
  sensitivity: 'normal',
  autostart: false,
  retention: 200,
  paused: false,
}

// ── state ────────────────────────────────────────────────────────────────
// Large payloads use shallowRef: they are replaced wholesale, never mutated.

export const ready = ref(false)
export const bootError = ref<string | null>(null)
export const page = ref<PageId>('live')
export const selectedId = ref<string | null>(null)

export const status = shallowRef<LiveStatus | null>(null)
export const system = shallowRef<SystemInfo | null>(null)
export const settings = shallowRef<Settings>(DEFAULT_SETTINGS)
export const incidents = shallowRef<Incident[]>([])
export const kb = shallowRef<KnowledgeEntry[]>([])
export const kbById = computed(() => new Map(kb.value.map((e) => [e.id, e])))

/** Visible toast stack and the notification history behind the bell. */
export const toasts = shallowRef<Toast[]>([])
export const notifications = shallowRef<Toast[]>([])

/** false while the window is hidden/minimized: components skip all drawing. */
export const visible = ref(true)
/** bumps on theme change so canvases redraw with the new palette. */
export const themeTick = ref(0)

/** 60 s of history for sparklines, appended on each status tick. */
const HISTORY = 60
export const history = shallowRef({ cpu: [] as number[], gpu: [] as number[], mem: [] as number[], dpc: [] as number[] })

export const unread = computed(() => incidents.value.reduce((n, i) => n + (i.read ? 0 : 1), 0))
export const now = computed(() => status.value?.ts ?? Date.now())

// ── KB helpers ───────────────────────────────────────────────────────────

export function kbEntry(id: string): KnowledgeEntry | undefined {
  return kbById.value.get(id)
}

export function causeTitle(c: CauseMatch | undefined | null): string {
  if (!c) return ''
  const e = kbEntry(c.id)
  return e ? fill(lk(e.title), c.params) : c.id
}

export function causeExplain(c: CauseMatch): string {
  const e = kbEntry(c.id)
  return e ? fill(lk(e.explain), c.params) : ''
}

export function causeFixes(c: CauseMatch): string[] {
  const e = kbEntry(c.id)
  return e ? e.fixes.map((f) => fill(lk(f), c.params)) : []
}

// ── actions ──────────────────────────────────────────────────────────────

const media = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null

function applyTheme(theme: Settings['theme']) {
  const resolved = theme === 'system' ? (media?.matches ? 'dark' : 'light') : theme
  const root = document.documentElement
  if (root.dataset.theme !== resolved) {
    root.dataset.theme = resolved
    themeTick.value++
  }
}

function applySettings(s: Settings) {
  settings.value = s
  lang.value = s.lang
  document.documentElement.lang = s.lang
  applyTheme(s.theme)
}

export async function updateSettings(patch: Partial<Omit<Settings, 'notify'>> & { notify?: Partial<Settings['notify']> }) {
  const prev = settings.value
  const next: Settings = { ...prev, ...patch, notify: { ...prev.notify, ...(patch.notify ?? {}) } }
  applySettings(next)
  try {
    applySettings(await call('set_settings', { settings: next }))
  } catch (e) {
    console.error('set_settings failed', e)
    applySettings({ ...next, autostart: prev.autostart })
    if (String(e).includes('autostart_needs_admin')) {
      pushToast({ id: `t-autostart-${Date.now()}`, ts: Date.now(), severity: 'major', incidentId: null, title: { key: 'toast.autostart_admin' }, cause: null })
    }
  }
}

export function navigate(p: PageId) {
  page.value = p
}

export function openIncident(id: string) {
  selectedId.value = id
  page.value = 'incidents'
}

export async function markRead(id: string | null) {
  incidents.value = incidents.value.map((i) => (id === null || i.id === id ? { ...i, read: true } : i))
  await call('mark_read', { id })
}

export async function deleteIncident(id: string) {
  incidents.value = incidents.value.filter((i) => i.id !== id)
  if (selectedId.value === id) selectedId.value = null
  await call('delete_incident', { id })
}

export async function clearIncidents() {
  incidents.value = []
  selectedId.value = null
  await call('clear_incidents')
}

const TOAST_MS = 6500
const MAX_TOASTS = 4

export function dismissToast(id: string) {
  toasts.value = toasts.value.filter((t) => t.id !== id)
}

function pushToast(t: Toast) {
  notifications.value = [t, ...notifications.value].slice(0, 40)
  const s = settings.value.notify
  const isIncident = t.incidentId !== null
  if (!s.inApp) return
  if (isIncident && SEVERITY_RANK[t.severity] < SEVERITY_RANK[s.minSeverity]) return
  toasts.value = [t, ...toasts.value].slice(0, MAX_TOASTS)
  window.setTimeout(() => dismissToast(t.id), TOAST_MS)
}

function pushHistory(s: LiveStatus) {
  const h = history.value
  const add = (arr: number[], v: number) => {
    const next = arr.length >= HISTORY ? arr.slice(arr.length - HISTORY + 1) : arr.slice()
    next.push(v)
    return next
  }
  history.value = {
    cpu: add(h.cpu, s.cpu.totalPct),
    gpu: add(h.gpu, s.gpu?.utilPct ?? 0),
    mem: add(h.mem, s.memory.loadPct),
    dpc: add(h.dpc, s.kernel.dpcMaxUs),
  }
}

// ── visibility: the engine only streams while the UI is actually visible ──

let lastActive: boolean | null = null

function setActive(active: boolean) {
  visible.value = active
  if (lastActive === active) return
  lastActive = active
  call('set_ui_active', { active }).catch(() => {})
}

async function watchVisibility() {
  const evaluate = async () => {
    let minimized = false
    if (isTauri) {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window')
        minimized = await getCurrentWindow().isMinimized()
      } catch {
        /* window API unavailable: rely on document visibility */
      }
    }
    setActive(!document.hidden && !minimized)
  }
  document.addEventListener('visibilitychange', evaluate)
  if (isTauri) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().onResized(evaluate)
    } catch {
      /* ignore */
    }
  }
  await evaluate()
}

// ── boot ─────────────────────────────────────────────────────────────────

export async function boot() {
  media?.addEventListener('change', () => applyTheme(settings.value.theme))
  try {
    const b = await call('get_bootstrap')
    applySettings(b.settings)
    system.value = b.system
    incidents.value = b.incidents
    kb.value = b.kb
    if (b.status) {
      status.value = b.status
      pushHistory(b.status)
    }
  } catch (e) {
    bootError.value = String(e)
    console.error('bootstrap failed', e)
  }

  await Promise.all([
    on<LiveStatus>(EVENTS.status, (s) => {
      status.value = s
      pushHistory(s)
    }),
    on<Incident>(EVENTS.incident, (i) => {
      incidents.value = [i, ...incidents.value.filter((x) => x.id !== i.id)]
    }),
    on<Toast>(EVENTS.toast, pushToast),
    on<Settings>(EVENTS.settings, applySettings),
  ])
  await watchVisibility()
  ready.value = true
}

/** Refresh SystemInfo (e.g. after a settings change that affects checks). */
export async function refreshSystem() {
  try {
    const b = await call('get_bootstrap')
    system.value = b.system
  } catch {
    /* keep previous */
  }
}
