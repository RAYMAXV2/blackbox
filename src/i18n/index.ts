import { ref } from 'vue'
import type { Lang, Localized } from '../lib/contract'
import en from './en'
import fr from './fr'

export type { MessageKey } from './en'

type Params = Record<string, string | number>

/** Current UI language. The store keeps it in sync with Settings.lang. */
export const lang = ref<Lang>('en')

const dicts: Record<Lang, Record<string, string>> = { en, fr }

/** Some params carry ids that must be translated themselves (e.g. a GPU throttle reason). */
const PARAM_NS: Record<string, string> = {
  reason: 'gpuThrottle',
  kind: 'kind',
  unit: 'unit',
  enabled: 'val',
  available: 'val',
  hvci: 'val',
}

const numberFormats = new Map<string, Intl.NumberFormat>()

export function locale(): string {
  return lang.value === 'fr' ? 'fr-FR' : 'en-US'
}

export function numberFormat(digits = 1): Intl.NumberFormat {
  const key = `${lang.value}:${digits}`
  let f = numberFormats.get(key)
  if (!f) {
    f = new Intl.NumberFormat(locale(), { maximumFractionDigits: digits })
    numberFormats.set(key, f)
  }
  return f
}

function formatParam(name: string, value: string | number | undefined): string {
  if (value === undefined || value === null) return ''
  if (typeof value === 'number') return numberFormat(value >= 100 ? 0 : 1).format(value)
  const ns = PARAM_NS[name]
  if (ns) {
    const hit = dicts[lang.value][`${ns}.${value}`] ?? en[`${ns}.${value}` as keyof typeof en]
    if (hit) return hit
  }
  return value
}

/** Replace {placeholders}; unknown/missing params collapse to an empty string. */
export function fill(text: string, params?: Params): string {
  return text
    .replace(/\{(\w+)\}/g, (_, name: string) => formatParam(name, params?.[name]))
    .replace(/\s{2,}/g, ' ')
    .trim()
}

/** Translate a UI key. Falls back to English, then to the key itself. */
export function t(key: string, params?: Params): string {
  const text = dicts[lang.value][key] ?? (en as Record<string, string>)[key] ?? key
  return fill(text, params)
}

/** Does this key exist? (used for optional per-state strings) */
export function has(key: string): boolean {
  return key in dicts[lang.value] || key in en
}

/** Pick the active language from a KB localized string. */
export function lk(text: Localized): string {
  return text[lang.value] ?? text.en
}
