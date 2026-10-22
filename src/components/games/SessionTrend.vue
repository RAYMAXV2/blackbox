<script setup lang="ts">
// FPS average (bars) and 1% low (line) across the stored sessions, oldest → newest.
import { computed, ref } from 'vue'
import type { SessionStats } from '../../lib/contract'
import { alpha, palette, prepare } from '../../lib/canvas'
import { useCanvas } from '../../lib/useCanvas'
import { fmtNum } from '../../lib/format'
import { t } from '../../i18n'

const props = defineProps<{ sessions: SessionStats[] }>()
const host = ref<HTMLDivElement>()
const canvas = ref<HTMLCanvasElement>()

// sessions arrive newest first
const series = computed(() => [...props.sessions].reverse())

useCanvas(
  host,
  (w, h) => {
    const ctx = canvas.value && prepare(canvas.value, w, h)
    if (!ctx) return
    const p = palette(host.value!)
    const data = series.value
    if (!data.length) return
    const padL = 38
    const padB = 6
    const padT = 10
    const top = Math.max(...data.map((s) => s.fpsAvg)) * 1.12 || 1
    const y = (v: number) => h - padB - (v / top) * (h - padT - padB)
    const slot = (w - padL) / data.length
    const bar = Math.min(34, slot * 0.56)

    // grid: 3 horizontal guides with labels
    ctx.font = `10px ${p.mono}`
    ctx.textBaseline = 'middle'
    ctx.textAlign = 'right'
    for (const f of [0.33, 0.66, 1]) {
      const v = top * f
      const yy = Math.round(y(v)) + 0.5
      ctx.strokeStyle = alpha(p.text3, 0.35)
      ctx.setLineDash([2, 4])
      ctx.beginPath()
      ctx.moveTo(padL, yy)
      ctx.lineTo(w, yy)
      ctx.stroke()
      ctx.setLineDash([])
      ctx.fillStyle = p.text3
      ctx.fillText(fmtNum(v), padL - 8, yy)
    }

    // FPS avg bars, newest emphasized
    data.forEach((s, i) => {
      const cx = padL + slot * (i + 0.5)
      const newest = i === data.length - 1
      ctx.fillStyle = newest ? alpha(p.text, 0.55) : alpha(p.text, 0.16)
      const yy = y(s.fpsAvg)
      ctx.beginPath()
      ctx.roundRect(cx - bar / 2, yy, bar, h - padB - yy, [5, 5, 2, 2])
      ctx.fill()
    })

    // 1% low line
    ctx.strokeStyle = p.red
    ctx.lineWidth = 1.6
    ctx.lineJoin = 'round'
    ctx.beginPath()
    data.forEach((s, i) => {
      const cx = padL + slot * (i + 0.5)
      if (i === 0) ctx.moveTo(cx, y(s.low1Fps))
      else ctx.lineTo(cx, y(s.low1Fps))
    })
    ctx.stroke()
    ctx.fillStyle = p.red
    data.forEach((s, i) => {
      const cx = padL + slot * (i + 0.5)
      ctx.beginPath()
      ctx.arc(cx, y(s.low1Fps), 2.8, 0, Math.PI * 2)
      ctx.fill()
    })
  },
  [series],
)
</script>

<template>
  <div class="trend">
    <div class="legend mono">
      <span><i class="sw bar" />{{ t('games.trend.fps') }}</span>
      <span><i class="sw line" />{{ t('games.trend.low1') }}</span>
    </div>
    <div ref="host" class="plot"><canvas ref="canvas" /></div>
  </div>
</template>

<style scoped>
.trend {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.legend {
  display: flex;
  gap: 16px;
  font-size: 10.5px;
  color: var(--text-3);
  text-transform: uppercase;
  letter-spacing: 0.08em;
}
.legend span {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.sw {
  display: inline-block;
}
.sw.bar {
  width: 8px;
  height: 10px;
  border-radius: 2px;
  background: var(--text-2);
  opacity: 0.6;
}
.sw.line {
  width: 14px;
  height: 2px;
  background: var(--red);
}
.plot {
  height: 140px;
}
canvas {
  display: block;
}
</style>
