import { lang, locale, numberFormat, t } from '../i18n'

const pctSuffix = () => (lang.value === 'fr' ? ' %' : '%')

export function fmtNum(n: number | null | undefined, digits = 0): string {
  if (n === null || n === undefined || !Number.isFinite(n)) return '—'
  return numberFormat(digits).format(n)
}

export function fmtPct(n: number | null | undefined, digits = 0): string {
  if (n === null || n === undefined || !Number.isFinite(n)) return '—'
  return fmtNum(n, digits) + pctSuffix()
}

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const

/** Binary units, localized symbols (MB / Mo). */
export function fmtBytes(bytes: number | null | undefined, digits = 1): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes)) return '—'
  let v = Math.abs(bytes)
  let i = 0
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024
    i++
  }
  const d = i === 0 ? 0 : v >= 100 ? 0 : digits
  return `${fmtNum(v, d)} ${t('bytes.' + UNITS[i])}`
}

export function fmtRate(bps: number | null | undefined): string {
  if (bps === null || bps === undefined) return '—'
  return fmtBytes(bps) + '/s'
}

export function fmtMb(mb: number | null | undefined, digits = 1): string {
  if (mb === null || mb === undefined) return '—'
  return fmtBytes(mb * 1024 * 1024, digits)
}

export function fmtUs(us: number | null | undefined): string {
  if (us === null || us === undefined) return '—'
  return `${fmtNum(us, us < 10 ? 1 : 0)} µs`
}

export function fmtMs(ms: number | null | undefined, digits = 1): string {
  if (ms === null || ms === undefined) return '—'
  return `${fmtNum(ms, ms >= 100 ? 0 : digits)} ms`
}

export function fmtDuration(sec: number): string {
  const s = Math.max(0, Math.floor(sec))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const r = s % 60
  const pad = (n: number) => String(n).padStart(2, '0')
  if (h > 0) return `${h} h ${pad(m)} min`
  if (m > 0) return `${m} min ${pad(r)} s`
  return `${r} s`
}

export function fmtTime(ts: number): string {
  return new Date(ts).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23' })
}

export function fmtDateTime(ts: number): string {
  return new Date(ts).toLocaleString(locale(), {
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23',
  })
}

const rtfCache = new Map<string, Intl.RelativeTimeFormat>()

export function fmtRelative(ts: number, now: number): string {
  const key = locale()
  let rtf = rtfCache.get(key)
  if (!rtf) {
    rtf = new Intl.RelativeTimeFormat(key, { numeric: 'auto', style: 'short' })
    rtfCache.set(key, rtf)
  }
  const diff = Math.round((ts - now) / 1000)
  const abs = Math.abs(diff)
  if (abs < 60) return rtf.format(diff, 'second')
  if (abs < 3600) return rtf.format(Math.round(diff / 60), 'minute')
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), 'hour')
  return rtf.format(Math.round(diff / 86400), 'day')
}

/** Magnitude of an incident with its unit, localized. */
export function fmtMagnitude(m: { value: number; unit: 'ms' | 'us' | 's' | 'pct' }): string {
  if (m.unit === 'pct') return fmtPct(m.value)
  return `${fmtNum(m.value, m.value < 10 ? 1 : 0)} ${t('unit.' + m.unit)}`
}

export const SEVERITY_RANK = { minor: 0, major: 1, severe: 2 } as const
