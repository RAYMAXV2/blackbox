// Browser-only simulation of the per-game session history (Games tab).
// Averages follow the engine's rules: duration-weighted means, max of maxima,
// min of minima, sums of counts, hitches per hour = total hitches / total hours.

import type { GameRecord, SessionStats } from './contract'

let seed = 0x2545f491
function rnd(): number {
  seed ^= seed << 13
  seed ^= seed >>> 17
  seed ^= seed << 5
  return ((seed >>> 0) % 1_000_000) / 1_000_000
}
const rr = (a: number, b: number) => a + rnd() * (b - a)
const ri = (a: number, b: number) => Math.round(rr(a, b))
const round = (v: number, d = 0) => Math.round(v * 10 ** d) / 10 ** d

interface Profile {
  name: string
  fps: number
  cap?: number
  low1: number // ratio of fps
  low01: number
  hitchesH: number
  freezesH: number
  causes: string[]
  gpuUtil: number
  gpuTemp: number
  gpuPower: number
  gpuClock: number
  gpuFan: number
  vram: number
  throttle: number
  cpu: number
  gameCpu: number
  ram: number
  gameRam: number
  disk: number
  sessions: number
  minutes: [number, number]
}

const PROFILES: Profile[] = [
  {
    name: 'Cyberpunk2077.exe',
    fps: 112,
    low1: 0.64,
    low01: 0.42,
    hitchesH: 22,
    freezesH: 0.8,
    causes: ['io.game_streaming', 'gpu.power_limit', 'game.internal', 'proc.shader_precache'],
    gpuUtil: 97,
    gpuTemp: 74,
    gpuPower: 548,
    gpuClock: 2640,
    gpuFan: 58,
    vram: 14600,
    throttle: 21,
    cpu: 34,
    gameCpu: 26,
    ram: 22400,
    gameRam: 9800,
    disk: 18,
    sessions: 6,
    minutes: [40, 150],
  },
  {
    name: 'Overwatch.exe',
    fps: 358,
    low1: 0.66,
    low01: 0.45,
    hitchesH: 3.5,
    freezesH: 0.05,
    causes: ['game.internal', 'drv.gpu', 'proc.background_cpu'],
    gpuUtil: 71,
    gpuTemp: 61,
    gpuPower: 318,
    gpuClock: 2715,
    gpuFan: 38,
    vram: 6400,
    throttle: 2,
    cpu: 23,
    gameCpu: 15,
    ram: 18100,
    gameRam: 3100,
    disk: 4,
    sessions: 8,
    minutes: [25, 120],
  },
  {
    name: 'RocketLeague.exe',
    fps: 238,
    cap: 240,
    low1: 0.79,
    low01: 0.58,
    hitchesH: 6,
    freezesH: 0.1,
    causes: ['game.internal', 'proc.background_cpu', 'app.not_responding'],
    gpuUtil: 33,
    gpuTemp: 52,
    gpuPower: 176,
    gpuClock: 2310,
    gpuFan: 30,
    vram: 3900,
    throttle: 0,
    cpu: 14,
    gameCpu: 7,
    ram: 16800,
    gameRam: 2300,
    disk: 3,
    sessions: 5,
    minutes: [15, 70],
  },
  {
    name: 'VALORANT-Win64-Shipping.exe',
    fps: 505,
    low1: 0.58,
    low01: 0.38,
    hitchesH: 4.2,
    freezesH: 0.05,
    causes: ['drv.network', 'game.internal', 'cpu.saturated'],
    gpuUtil: 54,
    gpuTemp: 59,
    gpuPower: 262,
    gpuClock: 2760,
    gpuFan: 35,
    vram: 4200,
    throttle: 1,
    cpu: 31,
    gameCpu: 22,
    ram: 17600,
    gameRam: 2900,
    disk: 5,
    sessions: 4,
    minutes: [30, 110],
  },
]

const profileOf = (name: string) => PROFILES.find((p) => p.name === name)

function session(p: Profile, startTs: number, durationSec: number, id: string): SessionStats {
  const hours = durationSec / 3600
  let fps = p.fps * rr(0.95, 1.05)
  if (p.cap) fps = Math.min(fps, p.cap - rr(0.5, 2.5))
  const ft = 1000 / fps
  const hitches = Math.max(0, Math.round(p.hitchesH * hours * rr(0.5, 1.6)))
  const major = Math.round(hitches * rr(0.25, 0.4))
  const severe = Math.round(hitches * rr(0.02, 0.1))
  const minor = Math.max(0, hitches - major - severe)
  const freezes = Math.round(p.freezesH * hours * rr(0, 2))
  const incidents = Math.round(major + severe + freezes + rr(0, 2))
  let left = incidents
  const topCauses = p.causes
    .slice(0, ri(1, Math.min(3, p.causes.length)))
    .map((id, i, arr) => {
      const count = i === arr.length - 1 ? left : Math.max(1, Math.round(left * rr(0.45, 0.7)))
      left -= count
      return { id, count }
    })
    .filter((c) => c.count > 0)
  const gpuTemp = p.gpuTemp + rr(-2.5, 2.5)
  const gpuUtil = Math.min(99, p.gpuUtil + rr(-4, 3))
  const gpuPower = p.gpuPower * rr(0.94, 1.04)
  const vram = p.vram * rr(0.93, 1.05)
  const cpu = p.cpu + rr(-3, 3)
  const ram = p.ram + rr(-600, 600)
  return {
    id,
    game: p.name,
    sessions: 1,
    startTs,
    endTs: startTs + durationSec * 1000,
    durationSec: Math.round(durationSec),
    fullTracing: true,
    frames: Math.round(fps * durationSec),
    fpsAvg: round(fps, 1),
    fpsMin1s: round(fps * p.low1 * rr(1.05, 1.18), 1),
    fpsMax1s: round(p.cap ? p.cap + rr(0, 1.5) : fps * rr(1.08, 1.16), 1),
    frameTimeAvgMs: round(ft, 2),
    frameTimeP50Ms: round(ft * rr(0.96, 0.99), 2),
    frameTimeP95Ms: round(ft * rr(1.15, 1.3), 2),
    frameTimeP99Ms: round(ft * rr(1.45, 1.75), 2),
    low1Fps: round(fps * p.low1 * rr(0.94, 1.05), 1),
    low01Fps: round(fps * p.low01 * rr(0.9, 1.08), 1),
    hitches,
    hitchesMinor: minor,
    hitchesMajor: major,
    hitchesSevere: severe,
    freezes,
    hitchesPerHour: round(hitches / Math.max(hours, 1 / 60), 1),
    incidents,
    topCauses,
    cpuAvgPct: round(cpu, 1),
    cpuMaxPct: round(Math.min(100, cpu + rr(18, 34)), 1),
    gameCpuAvgPct: round(p.gameCpu + rr(-2, 2), 1),
    cpuFreqLimitMinPct: p.throttle > 10 && rnd() < 0.4 ? round(rr(88, 96), 0) : 100,
    ramUsedAvgMb: Math.round(ram),
    ramUsedMaxMb: Math.round(ram + rr(400, 1600)),
    gameRamMaxMb: Math.round(p.gameRam * rr(0.95, 1.08)),
    gpuUtilAvgPct: round(gpuUtil, 1),
    gpuUtilMaxPct: Math.min(100, Math.round(gpuUtil + rr(4, 14))),
    gpuTempAvgC: round(gpuTemp, 1),
    gpuTempMaxC: Math.round(gpuTemp + rr(3, 8)),
    gpuPowerAvgW: round(gpuPower, 0),
    gpuPowerMaxW: Math.round(Math.min(575, gpuPower * rr(1.04, 1.12))),
    gpuClockAvgMhz: Math.round(p.gpuClock * rr(0.98, 1.01)),
    gpuFanAvgPct: round(p.gpuFan + rr(-3, 3), 0),
    vramAvgMb: Math.round(vram),
    vramMaxMb: Math.round(vram * rr(1.04, 1.12)),
    gpuThrottlePct: round(Math.max(0, p.throttle + rr(-3, 4)), 1),
    dpcMaxUs: ri(180, 920),
    isrMaxUs: ri(40, 320),
    diskLatMaxMs: round(p.disk * rr(0.6, 1.8), 1),
    fullscreenPct: round(rr(93, 100), 0),
  }
}

type Num = number | null
const AVG_KEYS = [
  'fpsAvg', 'frameTimeAvgMs', 'frameTimeP50Ms', 'frameTimeP95Ms', 'frameTimeP99Ms', 'low1Fps', 'low01Fps',
  'cpuAvgPct', 'gameCpuAvgPct', 'ramUsedAvgMb', 'gpuUtilAvgPct', 'gpuTempAvgC', 'gpuPowerAvgW', 'gpuClockAvgMhz',
  'gpuFanAvgPct', 'vramAvgMb', 'gpuThrottlePct', 'fullscreenPct',
] as const
const MAX_KEYS = ['fpsMax1s', 'cpuMaxPct', 'ramUsedMaxMb', 'gameRamMaxMb', 'gpuUtilMaxPct', 'gpuTempMaxC', 'gpuPowerMaxW', 'vramMaxMb', 'dpcMaxUs', 'isrMaxUs', 'diskLatMaxMs'] as const
const MIN_KEYS = ['fpsMin1s', 'cpuFreqLimitMinPct'] as const
const SUM_KEYS = ['frames', 'durationSec', 'hitches', 'hitchesMinor', 'hitchesMajor', 'hitchesSevere', 'freezes', 'incidents'] as const

/** Duration-weighted average of sessions (same rules as the engine). */
export function average(list: SessionStats[]): SessionStats | null {
  if (!list.length) return null
  const out = structuredClone(list[0]) as SessionStats & Record<string, unknown>
  const rec = out as unknown as Record<string, Num>
  const get = (s: SessionStats, k: string) => (s as unknown as Record<string, Num>)[k]
  for (const k of AVG_KEYS) {
    let w = 0
    let sum = 0
    for (const s of list) {
      const v = get(s, k)
      if (v === null || v === undefined) continue
      w += s.durationSec
      sum += v * s.durationSec
    }
    rec[k] = w ? round(sum / w, k.endsWith('Ms') ? 2 : 1) : null
  }
  for (const k of MAX_KEYS) {
    const vals = list.map((s) => get(s, k)).filter((v): v is number => v !== null)
    rec[k] = vals.length ? Math.max(...vals) : null
  }
  for (const k of MIN_KEYS) rec[k] = Math.min(...list.map((s) => get(s, k) ?? Infinity))
  for (const k of SUM_KEYS) rec[k] = list.reduce((a, s) => a + (get(s, k) ?? 0), 0)
  const causes = new Map<string, number>()
  for (const s of list) for (const c of s.topCauses) causes.set(c.id, (causes.get(c.id) ?? 0) + c.count)
  out.topCauses = [...causes].map(([id, count]) => ({ id, count })).sort((a, b) => b.count - a.count).slice(0, 5)
  out.hitchesPerHour = round(out.hitches / Math.max(out.durationSec / 3600, 1 / 60), 1)
  out.id = 'avg3'
  out.sessions = list.length
  out.fullTracing = list.every((s) => s.fullTracing)
  out.startTs = Math.min(...list.map((s) => s.startTs))
  out.endTs = Math.max(...list.map((s) => s.endTs))
  return out
}

interface Stored {
  sessions: SessionStats[] // newest first
  totalSessions: number
  totalPlaySec: number
}

const stored = new Map<string, Stored>()
const DAY = 86_400_000
let liveStart = Date.now() - 1834 * 1000
let nextId = 1

if (new URLSearchParams(location.search).get('games') !== '0') {
  for (const p of PROFILES) {
    const list: SessionStats[] = []
    let t = Date.now() - rr(0.3, 1.5) * DAY
    for (let i = 0; i < p.sessions; i++) {
      const dur = rr(p.minutes[0], p.minutes[1]) * 60
      const start = t - dur * 1000
      list.push(session(p, start, dur, `s${nextId++}`))
      t = start - rr(0.6, 2.6) * DAY
    }
    const extra = ri(0, 9)
    stored.set(p.name, {
      sessions: list,
      totalSessions: list.length + extra,
      totalPlaySec: list.reduce((a, s) => a + s.durationSec, 0) + extra * ri(1800, 4200),
    })
  }
}

function record(name: string, s: Stored | undefined, live: SessionStats | null): GameRecord {
  const sessions = s?.sessions ?? []
  return {
    name,
    sessions,
    last3: average(sessions.slice(0, 3)),
    live,
    totalSessions: s?.totalSessions ?? 0,
    totalPlaySec: (s?.totalPlaySec ?? 0) + (live?.durationSec ?? 0),
    lastPlayedTs: live ? Date.now() : (sessions[0]?.endTs ?? 0),
  }
}

/** All games, most recently played first. `liveGame` is the game being played right now. */
export function getGames(liveGame: string | null): GameRecord[] {
  const names = new Set([...stored.keys()])
  const lp = liveGame ? profileOf(liveGame) : undefined
  if (lp) names.add(lp.name)
  const out = [...names].map((name) => {
    const live = lp && lp.name === name ? session(lp, liveStart, (Date.now() - liveStart) / 1000, 'live') : null
    return record(name, stored.get(name), live)
  })
  return out.sort((a, b) => b.lastPlayedTs - a.lastPlayedTs)
}

export function deleteGame(name: string) {
  stored.delete(name)
}

/** The live session ends: it joins the history and a new one starts. */
export function endLiveSession(liveGame: string): GameRecord | null {
  const p = profileOf(liveGame)
  if (!p) return null
  const now = Date.now()
  const done = session(p, liveStart, (now - liveStart) / 1000, `s${nextId++}`)
  const s = stored.get(p.name) ?? { sessions: [], totalSessions: 0, totalPlaySec: 0 }
  s.sessions = [done, ...s.sessions].slice(0, 20)
  s.totalSessions += 1
  s.totalPlaySec += done.durationSec
  stored.set(p.name, s)
  liveStart = now
  return record(p.name, s, null)
}
