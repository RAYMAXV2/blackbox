// Helpers for the Games tab: readable game names, cause labels without
// per-incident params, "better / worse than the average" tones, and the
// in-game / everything view switch.
import { ref, watch } from 'vue'
import { kbEntry } from '../store'
import { fill, lk, t } from '../i18n'
import type { PhaseTotals, SessionStats } from './contract'

// ── view scope: gameplay seconds only (default) or every second in the game ──

const SCOPE_KEY = 'blackbox.games.scope'
const HINT_KEY = 'blackbox.games.scopeHintSeen'

function readStore(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function writeStore(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* storage unavailable: the choice just won't be remembered */
  }
}

export type Scope = 'gameplay' | 'all'
export const scope = ref<Scope>(readStore(SCOPE_KEY) === 'all' ? 'all' : 'gameplay')
watch(scope, (s) => writeStore(SCOPE_KEY, s))

/** The one-line explanation under the toggle shows until the toggle is used or dismissed. */
export const scopeHintSeen = ref(readStore(HINT_KEY) === '1')
export function dismissScopeHint() {
  scopeHintSeen.value = true
  writeStore(HINT_KEY, '1')
}

/** Stats for the current scope. Records saved before the "everything" view existed fall back to gameplay. */
export function view(s: SessionStats): SessionStats
export function view(s: SessionStats | null): SessionStats | null
export function view(s: SessionStats | null): SessionStats | null {
  if (!s) return null
  return scope.value === 'all' && s.all ? s.all : s
}

/** True when this stat set has no "everything" view (older record) while that view is selected. */
export function missingAll(s: SessionStats | null): boolean {
  return scope.value === 'all' && !!s && !s.all
}

export function phaseTotal(p: PhaseTotals | null | undefined): number {
  return p ? p.gameplaySec + p.menuSec + p.loadingSec + p.awaySec : 0
}

/** Share of the session spent actually playing, 0..100, or null when phases weren't recorded. */
export function gameplayPct(p: PhaseTotals | null | undefined): number | null {
  const total = phaseTotal(p)
  return total > 0 && p ? (p.gameplaySec / total) * 100 : null
}

const SUFFIXES = [/-win64-shipping$/i, /-win64$/i, /_win64$/i, /-shipping$/i, /_x64$/i, /x64$/i, /_dx1[12]$/i, /_be$/i]

/** "RocketLeague.exe" → "Rocket League", "VALORANT-Win64-Shipping.exe" → "VALORANT". */
export function prettyGameName(exe: string): string {
  let n = exe.replace(/\.exe$/i, '')
  for (const s of SUFFIXES) n = n.replace(s, '')
  n = n
    .replace(/[_-]+/g, ' ')
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/([a-z]{2,})(\d{2,})/g, '$1 $2')
    .replace(/\s+/g, ' ')
    .trim()
  return n || exe
}

/**
 * A cause's KB title when there is no incident to fill it from: the game's own name
 * goes in, driver names drop out, other processes become "a background process".
 */
export function causeLabel(id: string, game: string): string {
  const e = kbEntry(id)
  if (!e) return id
  const template = lk(e.title).replace(/\s*\(\{\w+\}\)/g, '')
  const text = fill(template, { app: prettyGameName(game), process: t('games.cause.process'), driver: '' })
  return text.charAt(0).toUpperCase() + text.slice(1)
}

export type Tone = 'better' | 'worse' | ''

/**
 * Compares a session value with the average. `higherIsBetter` decides the direction;
 * `tol` is the relative difference that still counts as "the same".
 */
export function tone(value: number | null | undefined, ref: number | null | undefined, higherIsBetter: boolean, tol = 0.03): Tone {
  if (value === null || value === undefined || ref === null || ref === undefined || !Number.isFinite(value) || !Number.isFinite(ref)) return ''
  const base = Math.max(Math.abs(ref), 1e-6)
  const rel = (value - ref) / base
  if (Math.abs(rel) <= tol) return ''
  return rel > 0 === higherIsBetter ? 'better' : 'worse'
}
