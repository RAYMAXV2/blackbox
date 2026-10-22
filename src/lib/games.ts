// Helpers for the Games tab: readable game names, cause labels without
// per-incident params, and "better / worse than the average" tones.
import { kbEntry } from '../store'
import { fill, lk, t } from '../i18n'

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
