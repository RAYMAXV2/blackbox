/** Size a canvas for crisp drawing at the device pixel ratio. Returns a 2D context in CSS pixels. */
export function prepare(canvas: HTMLCanvasElement, width: number, height: number): CanvasRenderingContext2D | null {
  const dpr = Math.min(window.devicePixelRatio || 1, 3)
  const w = Math.max(1, Math.round(width * dpr))
  const h = Math.max(1, Math.round(height * dpr))
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w
    canvas.height = h
    canvas.style.width = `${width}px`
    canvas.style.height = `${height}px`
  }
  const ctx = canvas.getContext('2d')
  if (!ctx) return null
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, width, height)
  return ctx
}

/** Read resolved CSS custom properties once per draw. */
export function palette(el: Element) {
  const cs = getComputedStyle(el)
  const v = (name: string) => cs.getPropertyValue(name).trim()
  return {
    text: v('--text'),
    text2: v('--text-2'),
    text3: v('--text-3'),
    line: v('--line'),
    lineStrong: v('--line-strong'),
    surface: v('--surface'),
    red: v('--red'),
    amber: v('--amber'),
    green: v('--green'),
    kernel: v('--l-kernel'),
    drivers: v('--l-drivers'),
    hardware: v('--l-hardware'),
    apps: v('--l-apps'),
    mono: v('--font-mono'),
  }
}

export type Palette = ReturnType<typeof palette>

/** Hex (#rrggbb) or rgb() color with alpha. */
export function alpha(color: string, a: number): string {
  if (color.startsWith('#') && color.length === 7) {
    const n = parseInt(color.slice(1), 16)
    return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`
  }
  return color
}

export function median(values: number[]): number {
  if (!values.length) return 0
  const s = [...values].sort((a, b) => a - b)
  const m = s.length >> 1
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2
}
