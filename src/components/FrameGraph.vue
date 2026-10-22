<script setup lang="ts">
// Live frame-time graph: one sample per frame, spikes highlighted.
import { ref } from 'vue'
import { alpha, median, palette, prepare } from '../lib/canvas'
import { useCanvas } from '../lib/useCanvas'
import { fmtMs } from '../lib/format'

const props = defineProps<{ frames: number[] }>()
const host = ref<HTMLDivElement>()
const canvas = ref<HTMLCanvasElement>()

useCanvas(
  host,
  (w, h) => {
    const ctx = canvas.value && prepare(canvas.value, w, h)
    if (!ctx) return
    const p = palette(host.value!)
    const f = props.frames
    if (!f.length) return
    const med = median(f)
    const peak = Math.max(...f)
    const top = Math.max(peak * 1.15, med * 2.4, 8)
    const padT = 14
    const y = (ms: number) => h - 2 - (ms / top) * (h - padT - 2)
    const step = w / Math.max(1, 240 - 1)
    const x0 = w - (f.length - 1) * step

    // reference lines: median and 2× median
    ctx.font = `10px ${p.mono}`
    ctx.textBaseline = 'bottom'
    // skip the 2× line when a big spike squashes the scale and the labels would collide
    const guides: [number, number][] = y(med) - y(med * 2) > 12 ? [[med, 0.9], [med * 2, 0.5]] : [[med, 0.9]]
    for (const [v, a] of guides) {
      const yy = Math.round(y(v)) + 0.5
      ctx.strokeStyle = alpha(p.text3, a)
      ctx.setLineDash([2, 4])
      ctx.beginPath()
      ctx.moveTo(0, yy)
      ctx.lineTo(w, yy)
      ctx.stroke()
      ctx.setLineDash([])
      ctx.fillStyle = p.text3
      ctx.fillText(fmtMs(v), 2, yy - 2)
    }

    // area + line
    ctx.beginPath()
    f.forEach((ms, i) => {
      const xx = x0 + i * step
      if (i === 0) ctx.moveTo(xx, y(ms))
      else ctx.lineTo(xx, y(ms))
    })
    ctx.strokeStyle = p.text
    ctx.lineWidth = 1.2
    ctx.lineJoin = 'round'
    ctx.stroke()
    ctx.lineTo(w, h)
    ctx.lineTo(x0, h)
    ctx.closePath()
    ctx.fillStyle = alpha(p.text, 0.06)
    ctx.fill()

    // hitches
    const limit = Math.max(med * 2.5, med + 8)
    ctx.fillStyle = p.red
    f.forEach((ms, i) => {
      if (ms < limit) return
      const xx = x0 + i * step
      ctx.beginPath()
      ctx.arc(xx, y(ms), 3, 0, Math.PI * 2)
      ctx.fill()
      ctx.textBaseline = 'top'
      ctx.textAlign = xx > w - 60 ? 'right' : 'left'
      ctx.fillText(fmtMs(ms, 0), xx + (xx > w - 60 ? -6 : 6), Math.max(0, y(ms) - 6))
      ctx.textAlign = 'left'
    })
  },
  [() => props.frames],
)
</script>

<template>
  <div ref="host" class="fg"><canvas ref="canvas" /></div>
</template>

<style scoped>
.fg {
  width: 100%;
  height: 100%;
  min-height: 90px;
}
canvas {
  display: block;
}
</style>
