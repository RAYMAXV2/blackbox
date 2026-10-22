<script setup lang="ts">
// Tiny area sparkline drawn on canvas (60 points at most).
import { ref } from 'vue'
import { alpha, palette, prepare } from '../lib/canvas'
import { useCanvas } from '../lib/useCanvas'

const props = withDefaults(defineProps<{ values: number[]; max?: number; tone?: 'kernel' | 'drivers' | 'hardware' | 'apps' | 'text' }>(), {
  max: 100,
  tone: 'text',
})

const host = ref<HTMLDivElement>()
const canvas = ref<HTMLCanvasElement>()

useCanvas(
  host,
  (w, h) => {
    const ctx = canvas.value && prepare(canvas.value, w, h)
    if (!ctx) return
    const p = palette(host.value!)
    const color = props.tone === 'text' ? p.text2 : p[props.tone]
    const vals = props.values
    if (vals.length < 2) return
    const max = Math.max(props.max, ...vals) || 1
    const step = w / (60 - 1)
    const x0 = w - (vals.length - 1) * step
    ctx.beginPath()
    vals.forEach((v, i) => {
      const x = x0 + i * step
      const y = h - 1 - (v / max) * (h - 3)
      if (i === 0) ctx.moveTo(x, y)
      else ctx.lineTo(x, y)
    })
    ctx.strokeStyle = color
    ctx.lineWidth = 1.25
    ctx.lineJoin = 'round'
    ctx.stroke()
    ctx.lineTo(w, h)
    ctx.lineTo(x0, h)
    ctx.closePath()
    ctx.fillStyle = alpha(color, 0.12)
    ctx.fill()
  },
  [() => props.values],
)
</script>

<template>
  <div ref="host" class="spark"><canvas ref="canvas" /></div>
</template>

<style scoped>
.spark {
  width: 100%;
  height: 32px;
}
canvas {
  display: block;
}
</style>
