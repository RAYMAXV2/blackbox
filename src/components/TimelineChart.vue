<script setup lang="ts">
// Multi-track incident timeline, all tracks share one time axis.
// The base layer is drawn once per data/theme/size change; hover only
// repaints a thin overlay canvas, so moving the mouse costs almost nothing.
import { computed, ref, shallowRef } from 'vue'
import type { Timeline } from '../lib/contract'
import { alpha, median, palette, prepare, type Palette } from '../lib/canvas'
import { useCanvas } from '../lib/useCanvas'
import { t } from '../i18n'
import { fmtMs, fmtNum, fmtPct, fmtUs } from '../lib/format'

const props = defineProps<{ tl: Timeline }>()

const host = ref<HTMLDivElement>()
const base = ref<HTMLCanvasElement>()
const over = ref<HTMLCanvasElement>()

const LABEL_W = 104
const PAD_R = 16
const GAP = 8
const AXIS_H = 22
const TRACKS = [
  { key: 'frames', h: 84 },
  { key: 'dpc', h: 46 },
  { key: 'disk', h: 46 },
  { key: 'faults', h: 30 },
  { key: 'cpugpu', h: 46 },
  { key: 'events', h: 46 },
] as const
type TrackKey = (typeof TRACKS)[number]['key']

const HEIGHT = TRACKS.reduce((s, tr) => s + tr.h + GAP, 0) + AXIS_H

const geo = shallowRef({ w: 0, plotW: 0 })
const hover = ref<{ x: number; off: number } | null>(null)

const span = computed(() => props.tl.t1 - props.tl.t0)
const marker = computed(() => props.tl.markerTs - props.tl.t0)

function xOf(off: number) {
  return LABEL_W + (off / span.value) * geo.value.plotW
}

function trackY(key: TrackKey) {
  let y = 0
  for (const tr of TRACKS) {
    if (tr.key === key) return { y, h: tr.h }
    y += tr.h + GAP
  }
  return { y: 0, h: 0 }
}

function drawLabel(ctx: CanvasRenderingContext2D, p: Palette, key: TrackKey, text: string, swatches: string[] = []) {
  const { y, h } = trackY(key)
  ctx.fillStyle = p.text3
  ctx.font = `10px ${p.mono}`
  ctx.textBaseline = 'top'
  ctx.textAlign = 'left'
  ctx.fillText(text.toUpperCase(), 0, y + 4)
  swatches.forEach((c, i) => {
    ctx.fillStyle = c
    ctx.fillRect(i * 12, y + 20, 8, 3)
  })
  ctx.strokeStyle = p.line
  ctx.lineWidth = 1
  ctx.beginPath()
  ctx.moveTo(LABEL_W, Math.round(y + h) + 0.5)
  ctx.lineTo(geo.value.w - PAD_R, Math.round(y + h) + 0.5)
  ctx.stroke()
}

function drawBase(w: number) {
  const ctx = base.value && prepare(base.value, w, HEIGHT)
  if (!ctx) return
  geo.value = { w, plotW: w - LABEL_W - PAD_R }
  const p = palette(host.value!)
  const tl = props.tl
  const nb = tl.dpcMaxUs.length
  const bw = geo.value.plotW / Math.max(1, nb)

  // frames
  {
    drawLabel(ctx, p, 'frames', t('track.frames'))
    const { y, h } = trackY('frames')
    const f = tl.frames
    if (f.length) {
      const med = median(f.map((v) => v.ms))
      const peak = Math.max(...f.map((v) => v.ms))
      const top = Math.max(peak * 1.12, med * 3, 20)
      const yy = (ms: number) => y + h - 1 - (ms / top) * (h - 8)
      ctx.strokeStyle = alpha(p.text3, 0.8)
      ctx.setLineDash([2, 4])
      ctx.beginPath()
      ctx.moveTo(LABEL_W, Math.round(yy(med)) + 0.5)
      ctx.lineTo(w - PAD_R, Math.round(yy(med)) + 0.5)
      ctx.stroke()
      ctx.setLineDash([])
      ctx.beginPath()
      f.forEach((pt, i) => {
        const x = xOf(pt.t)
        if (i === 0) ctx.moveTo(x, yy(pt.ms))
        else ctx.lineTo(x, yy(pt.ms))
      })
      ctx.strokeStyle = p.text
      ctx.lineWidth = 1.2
      ctx.lineJoin = 'round'
      ctx.stroke()
      const limit = Math.max(med * 2.5, med + 8)
      for (const pt of f) {
        if (pt.ms < limit) continue
        const x = xOf(pt.t)
        ctx.fillStyle = p.red
        ctx.beginPath()
        ctx.arc(x, yy(pt.ms), 3.2, 0, Math.PI * 2)
        ctx.fill()
        ctx.font = `600 11px ${p.mono}`
        ctx.textBaseline = 'middle'
        ctx.textAlign = 'left'
        ctx.fillText(fmtMs(pt.ms, 0), x + 8, yy(pt.ms) + 1)
      }
    } else {
      ctx.fillStyle = p.text3
      ctx.font = `11px ${p.mono}`
      ctx.textBaseline = 'middle'
      ctx.fillText('—', LABEL_W + 4, y + h / 2)
    }
  }

  // DPC / ISR worst call per bucket
  {
    drawLabel(ctx, p, 'dpc', t('track.dpc'), [p.drivers])
    const { y, h } = trackY('dpc')
    const vals = tl.dpcMaxUs.map((v, i) => Math.max(v, tl.isrMaxUs[i] ?? 0))
    const top = Math.max(1000, ...vals) * 1.05
    vals.forEach((v, i) => {
      const bh = Math.max(1, (v / top) * (h - 6))
      ctx.fillStyle = v >= 1000 ? p.red : v >= 500 ? p.amber : alpha(p.drivers, 0.75)
      ctx.fillRect(LABEL_W + i * bw + 0.5, y + h - bh, Math.max(1, bw - 1.5), bh)
    })
    const y1000 = y + h - (1000 / top) * (h - 6)
    ctx.strokeStyle = alpha(p.red, 0.45)
    ctx.setLineDash([2, 3])
    ctx.beginPath()
    ctx.moveTo(LABEL_W, Math.round(y1000) + 0.5)
    ctx.lineTo(w - PAD_R, Math.round(y1000) + 0.5)
    ctx.stroke()
    ctx.setLineDash([])
  }

  // disk throughput (area) + latency (line)
  {
    drawLabel(ctx, p, 'disk', t('track.disk'), [p.hardware, p.amber])
    const { y, h } = trackY('disk')
    const top = Math.max(50, ...tl.diskMbps) * 1.1
    ctx.beginPath()
    ctx.moveTo(LABEL_W, y + h)
    tl.diskMbps.forEach((v, i) => {
      const x = LABEL_W + i * bw + bw / 2
      ctx.lineTo(x, y + h - (v / top) * (h - 6))
    })
    ctx.lineTo(w - PAD_R, y + h)
    ctx.closePath()
    ctx.fillStyle = alpha(p.hardware, 0.28)
    ctx.fill()
    const ltop = Math.max(5, ...tl.diskLatMs) * 1.1
    ctx.beginPath()
    tl.diskLatMs.forEach((v, i) => {
      const x = LABEL_W + i * bw + bw / 2
      const yy = y + h - (v / ltop) * (h - 6)
      if (i === 0) ctx.moveTo(x, yy)
      else ctx.lineTo(x, yy)
    })
    ctx.strokeStyle = p.amber
    ctx.lineWidth = 1.2
    ctx.stroke()
  }

  // hard faults
  {
    drawLabel(ctx, p, 'faults', t('track.faults'), [p.kernel])
    const { y, h } = trackY('faults')
    const top = Math.max(50, ...tl.hardFaults) * 1.05
    tl.hardFaults.forEach((v, i) => {
      if (!v) return
      const bh = Math.max(1, (v / top) * (h - 4))
      ctx.fillStyle = alpha(p.kernel, 0.85)
      ctx.fillRect(LABEL_W + i * bw + 0.5, y + h - bh, Math.max(1, bw - 1.5), bh)
    })
  }

  // CPU / GPU per second (step lines)
  {
    drawLabel(ctx, p, 'cpugpu', t('track.cpugpu'), [p.kernel, p.hardware])
    const { y, h } = trackY('cpugpu')
    const step = (series: { t: number; v: number }[], color: string) => {
      if (!series.length) return
      ctx.beginPath()
      series.forEach((pt, i) => {
        const x0 = xOf(pt.t)
        const x1 = xOf(Math.min(span.value, pt.t + 1000))
        const yy = y + h - (pt.v / 100) * (h - 6)
        if (i === 0) ctx.moveTo(x0, yy)
        else ctx.lineTo(x0, yy)
        ctx.lineTo(x1, yy)
      })
      ctx.strokeStyle = color
      ctx.lineWidth = 1.4
      ctx.stroke()
    }
    step(tl.cpuPct, p.kernel)
    step(tl.gpuPct, p.hardware)
  }

  // events lane
  {
    drawLabel(ctx, p, 'events', t('track.events'))
    const { y } = trackY('events')
    const rows = [y + 10, y + 23, y + 36]
    const lastX = rows.map(() => -1e9)
    ctx.font = `11px ${p.mono}`
    ctx.textBaseline = 'middle'
    // flagged events first so they get the top row; then lay out greedily
    const ordered = [...props.tl.events].sort((a, b) => Number(b.flagged) - Number(a.flagged) || a.t - b.t)
    for (const ev of ordered) {
      const x = xOf(ev.t)
      const label = ev.label
      const tw = ctx.measureText(label).width + 16
      const right = x + tw < w - PAD_R
      const start = right ? x - 4 : x - tw
      const end = right ? x + tw : x + 4
      let row = rows.findIndex((_, i) => start > lastX[i] + 6)
      if (row === -1) row = lastX.indexOf(Math.min(...lastX))
      lastX[row] = Math.max(lastX[row], end)
      const ry = rows[row]
      ctx.strokeStyle = ev.flagged ? alpha(p.red, 0.6) : p.line
      ctx.beginPath()
      ctx.moveTo(Math.round(x) + 0.5, y + 2)
      ctx.lineTo(Math.round(x) + 0.5, ry)
      ctx.stroke()
      ctx.fillStyle = ev.flagged ? p.red : ev.kind === 'process_end' ? p.text3 : p.text2
      ctx.beginPath()
      if (ev.kind === 'process_end') ctx.rect(x - 3, ry - 3, 6, 6)
      else ctx.arc(x, ry, 3.5, 0, Math.PI * 2)
      ctx.fill()
      ctx.fillStyle = ev.flagged ? p.text : p.text3
      ctx.textAlign = right ? 'left' : 'right'
      ctx.fillText(label, right ? x + 8 : x - 8, ry + 0.5)
    }
    ctx.textAlign = 'left'
  }

  // marker
  const mx = Math.round(xOf(marker.value)) + 0.5
  const plotBottom = HEIGHT - AXIS_H
  ctx.strokeStyle = p.red
  ctx.lineWidth = 1
  ctx.setLineDash([3, 3])
  ctx.beginPath()
  ctx.moveTo(mx, 0)
  ctx.lineTo(mx, plotBottom)
  ctx.stroke()
  ctx.setLineDash([])

  // axis: seconds relative to the incident
  ctx.font = `10px ${p.mono}`
  ctx.textBaseline = 'top'
  ctx.textAlign = 'center'
  const first = Math.ceil(-marker.value / 1000)
  const last = Math.floor((span.value - marker.value) / 1000)
  for (let k = first; k <= last; k++) {
    const x = xOf(marker.value + k * 1000)
    ctx.fillStyle = k === 0 ? p.red : p.text3
    ctx.fillText(k === 0 ? '0' : `${k > 0 ? '+' : '−'}${Math.abs(k)} s`, x, plotBottom + 6)
    ctx.strokeStyle = p.lineStrong
    ctx.beginPath()
    ctx.moveTo(Math.round(x) + 0.5, plotBottom)
    ctx.lineTo(Math.round(x) + 0.5, plotBottom + 4)
    ctx.stroke()
  }
}

function drawOverlay() {
  const w = geo.value.w
  const ctx = over.value && prepare(over.value, w, HEIGHT)
  if (!ctx || !hover.value) return
  const p = palette(host.value!)
  const x = Math.round(hover.value.x) + 0.5
  ctx.strokeStyle = p.text2
  ctx.lineWidth = 1
  ctx.beginPath()
  ctx.moveTo(x, 0)
  ctx.lineTo(x, HEIGHT - AXIS_H)
  ctx.stroke()
}

const { redraw } = useCanvas(host, (w) => drawBase(w), [() => props.tl])

function onMove(e: MouseEvent) {
  const rect = host.value!.getBoundingClientRect()
  const x = e.clientX - rect.left
  if (x < LABEL_W || x > geo.value.w - PAD_R) {
    onLeave()
    return
  }
  const off = ((x - LABEL_W) / geo.value.plotW) * span.value
  hover.value = { x, off }
  requestAnimationFrame(drawOverlay)
}

function onLeave() {
  hover.value = null
  requestAnimationFrame(drawOverlay)
}

const tip = computed(() => {
  const hv = hover.value
  if (!hv) return null
  const tl = props.tl
  const bi = Math.min(tl.dpcMaxUs.length - 1, Math.max(0, Math.floor(hv.off / tl.bucketMs)))
  let frame: number | null = null
  let best = Infinity
  for (const f of tl.frames) {
    // a frame covers [t - ms, t]
    const d = hv.off < f.t - f.ms ? f.t - f.ms - hv.off : hv.off > f.t ? hv.off - f.t : 0
    if (d < best) {
      best = d
      frame = f.ms
    }
    if (d === 0) break
  }
  const sec = Math.floor(hv.off / 1000)
  const cpu = tl.cpuPct.find((v) => Math.floor(v.t / 1000) === sec)?.v
  const gpu = tl.gpuPct.find((v) => Math.floor(v.t / 1000) === sec)?.v
  const rel = (hv.off - marker.value) / 1000
  return {
    left: hv.x,
    rel: `${rel >= 0 ? '+' : '−'}${fmtNum(Math.abs(rel), 2)} s`,
    rows: [
      [t('track.frames'), frame !== null && best < 40 ? fmtMs(frame) : '—'],
      ['DPC', fmtUs(tl.dpcMaxUs[bi])],
      ['ISR', fmtUs(tl.isrMaxUs[bi])],
      [t('track.disk'), `${fmtNum(tl.diskMbps[bi], tl.diskMbps[bi] >= 100 ? 0 : 1)} ${t('bytes.MB')}/s · ${fmtMs(tl.diskLatMs[bi])}`],
      [t('track.faults'), fmtNum(tl.hardFaults[bi])],
      ['CPU', cpu !== undefined ? fmtPct(cpu) : '—'],
      ['GPU', gpu !== undefined ? fmtPct(gpu) : '—'],
    ],
  }
})

defineExpose({ redraw })
</script>

<template>
  <div ref="host" class="tl" :style="{ height: `${HEIGHT}px` }" @mousemove="onMove" @mouseleave="onLeave">
    <canvas ref="base" class="layer" />
    <canvas ref="over" class="layer" />
    <div v-if="tip" class="tip" :class="{ flip: tip.left > geo.w - 220 }" :style="{ left: `${tip.left}px` }">
      <div class="tip-head num">{{ tip.rel }}</div>
      <div v-for="r in tip.rows" :key="r[0]" class="tip-row">
        <span>{{ r[0] }}</span>
        <b class="num">{{ r[1] }}</b>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tl {
  position: relative;
  width: 100%;
  cursor: crosshair;
}
.layer {
  position: absolute;
  inset: 0;
  display: block;
}
.tip {
  position: absolute;
  top: 6px;
  transform: translateX(12px);
  min-width: 230px;
  padding: 10px 12px;
  border-radius: 12px;
  background: var(--surface-3);
  border: 1px solid var(--line-strong);
  font-size: 11.5px;
  pointer-events: none;
  z-index: 2;
}
.tip.flip {
  transform: translateX(calc(-100% - 12px));
}
.tip-head {
  color: var(--text-2);
  margin-bottom: 6px;
}
.tip-row {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  color: var(--text-3);
}
.tip-row b {
  white-space: nowrap;
  font-weight: 500;
  color: var(--text);
}
</style>
