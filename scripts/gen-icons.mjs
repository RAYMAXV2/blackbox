// Generates the app icon source (1024 px) and the tray icons as PNG, with no dependencies.
// Usage: node scripts/gen-icons.mjs   then   npx tauri icon src-tauri/icons/app-icon.png -o src-tauri/icons
import { deflateSync } from 'node:zlib'
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const out = join(dirname(fileURLToPath(import.meta.url)), '..', 'src-tauri', 'icons')
mkdirSync(out, { recursive: true })

const CRC = new Uint32Array(256).map((_, n) => {
  let c = n
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
  return c >>> 0
})
const crc32 = (buf) => {
  let c = 0xffffffff
  for (const b of buf) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8)
  return (c ^ 0xffffffff) >>> 0
}
const chunk = (type, data) => {
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length)
  const td = Buffer.concat([Buffer.from(type, 'ascii'), data])
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(crc32(td))
  return Buffer.concat([len, td, crc])
}
const png = (w, h, rgba) => {
  const raw = Buffer.alloc((w * 4 + 1) * h)
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0
    rgba.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4)
  }
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(w, 0)
  ihdr.writeUInt32BE(h, 4)
  ihdr[8] = 8 // bit depth
  ihdr[9] = 6 // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ])
}

// Shapes: { kind: 'rrect', x, y, w, h, r, color } | { kind: 'circle', cx, cy, r, color }. color = [r,g,b,a]
const inside = (s, x, y) => {
  if (s.kind === 'circle') return (x - s.cx) ** 2 + (y - s.cy) ** 2 <= s.r * s.r
  const dx = Math.max(s.x + s.r - x, 0, x - (s.x + s.w - s.r))
  const dy = Math.max(s.y + s.r - y, 0, y - (s.y + s.h - s.r))
  return x >= s.x && x <= s.x + s.w && y >= s.y && y <= s.y + s.h && dx * dx + dy * dy <= s.r * s.r
}
const render = (size, shapes) => {
  const buf = Buffer.alloc(size * size * 4)
  const SS = 4
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let [r, g, b, a] = [0, 0, 0, 0]
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const px = x + (sx + 0.5) / SS
          const py = y + (sy + 0.5) / SS
          let col = [0, 0, 0, 0]
          for (const s of shapes) if (inside(s, px, py)) col = s.color
          const ca = col[3] / 255
          r += col[0] * ca
          g += col[1] * ca
          b += col[2] * ca
          a += ca
        }
      }
      const n = SS * SS
      const i = (y * size + x) * 4
      const alpha = a / n
      buf[i] = alpha ? Math.round(r / a) : 0
      buf[i + 1] = alpha ? Math.round(g / a) : 0
      buf[i + 2] = alpha ? Math.round(b / a) : 0
      buf[i + 3] = Math.round(alpha * 255)
    }
  }
  return png(size, size, buf)
}

const WHITE = [242, 242, 240, 255]
const DIM = [52, 52, 50, 255]
const RED = [215, 25, 33, 255]
const GREY = [140, 140, 136, 255]

// App icon: black squircle, a 5x5 dot "box" with a red recording dot in the middle.
{
  const S = 1024
  const shapes = [{ kind: 'rrect', x: 0, y: 0, w: S, h: S, r: 230, color: [0, 0, 0, 255] }]
  const step = 150
  const start = S / 2 - 2 * step
  for (let gy = 0; gy < 5; gy++) {
    for (let gx = 0; gx < 5; gx++) {
      const edge = gx === 0 || gy === 0 || gx === 4 || gy === 4
      const center = gx === 2 && gy === 2
      shapes.push({
        kind: 'circle',
        cx: start + gx * step,
        cy: start + gy * step,
        r: center ? 58 : 46,
        color: center ? RED : edge ? WHITE : DIM,
      })
    }
  }
  writeFileSync(join(out, 'app-icon.png'), render(S, shapes))
}

// Tray icons (64 px, shown at 16-32 px): a 3x3 dot box.
const tray = (name, ring, center) => {
  const S = 64
  const step = 22
  const shapes = []
  for (let gy = 0; gy < 3; gy++) {
    for (let gx = 0; gx < 3; gx++) {
      const isCenter = gx === 1 && gy === 1
      shapes.push({ kind: 'circle', cx: 10 + gx * step, cy: 10 + gy * step, r: isCenter ? 8.5 : 7, color: isCenter ? center : ring })
    }
  }
  writeFileSync(join(out, name), render(S, shapes))
}
tray('tray-ok.png', WHITE, WHITE)
tray('tray-alert.png', WHITE, RED)
tray('tray-paused.png', GREY, GREY)
console.log('icons written to', out)
