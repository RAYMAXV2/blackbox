<script setup lang="ts">
// Every number of a play session (or of an average of sessions), grouped like the
// Live page reads: frames → stability → GPU → CPU & memory → system latency.
import { computed } from 'vue'
import PhaseBar from './PhaseBar.vue'
import StatTile from './StatTile.vue'
import type { SessionStats } from '../../lib/contract'
import { t } from '../../i18n'
import { fmtMb, fmtMs, fmtNum, fmtPct, fmtUs } from '../../lib/format'
import { causeLabel, phaseTotal, tone, type Tone } from '../../lib/games'

const props = withDefaults(
  defineProps<{
    /** Already resolved for the current scope (see `view`). */
    stats: SessionStats
    game: string
    /** When set, values are tinted better/worse against it. */
    reference?: SessionStats | null
    /** A short subset (live session card). */
    compact?: boolean
    /** Shown above the numbers, e.g. why the "everything" view isn't available. */
    note?: string
  }>(),
  { reference: null, compact: false, note: '' },
)

const hasPhases = computed(() => phaseTotal(props.stats.phases) > 0)
const secs = (v: number) => `${fmtNum(v, v < 10 ? 1 : 0)} s`

const s = computed(() => props.stats)
const r = computed(() => props.reference)

function tn(key: keyof SessionStats, higherIsBetter: boolean, tol = 0.03): Tone {
  if (!r.value) return ''
  return tone(s.value[key] as number | null, r.value[key] as number | null, higherIsBetter, tol)
}

const deg = (v: number | null) => (v === null ? '—' : fmtNum(v))
const maxOf = (v: string) => `${t('games.m.max')} ${v}`

const hasGpu = computed(() =>
  [s.value.gpuUtilAvgPct, s.value.gpuTempAvgC, s.value.gpuPowerAvgW, s.value.vramAvgMb, s.value.gpuClockAvgMhz].some((v) => v !== null),
)
</script>

<template>
  <p v-if="note" class="note top-note">{{ note }}</p>
  <PhaseBar v-if="hasPhases" :phases="s.phases" class="phase-bar" />
  <div v-if="compact" class="tiles compact">
    <StatTile :label="t('games.m.fpsAvg')" :value="fmtNum(s.fpsAvg)" unit="FPS" />
    <StatTile :label="t('games.m.low1')" :value="fmtNum(s.low1Fps)" unit="FPS" />
    <StatTile :label="t('games.m.ftP99')" :value="fmtMs(s.frameTimeP99Ms, 2)" />
    <StatTile :label="t('games.m.hitches')" :value="fmtNum(s.hitches)" :sub="`${fmtNum(s.hitchesPerHour, 1)} / h`" />
    <StatTile v-if="hasGpu" :label="t('games.m.gpuTemp')" :value="deg(s.gpuTempAvgC)" unit="°C" :sub="maxOf(`${deg(s.gpuTempMaxC)} °C`)" />
    <StatTile v-if="hasGpu" :label="t('games.m.gpuUtil')" :value="fmtPct(s.gpuUtilAvgPct)" :sub="maxOf(fmtPct(s.gpuUtilMaxPct))" />
    <StatTile :label="t('games.m.cpu')" :value="fmtPct(s.cpuAvgPct)" :sub="maxOf(fmtPct(s.cpuMaxPct))" />
    <StatTile v-if="s.cpuTempAvgC !== null" :label="t('games.m.cpuTemp')" :value="fmtNum(s.cpuTempAvgC)" unit="°C" :sub="maxOf(`${deg(s.cpuTempMaxC)} °C`)" />
  </div>

  <div v-else class="grid">
    <section class="group">
      <div class="g-head"><span class="dot frames" /><span class="label">{{ t('games.sec.frames') }}</span></div>
      <div class="tiles">
        <StatTile :label="t('games.m.fpsAvg')" :value="fmtNum(s.fpsAvg)" unit="FPS" :tone="tn('fpsAvg', true)" />
        <StatTile :label="t('games.m.low1')" :value="fmtNum(s.low1Fps)" unit="FPS" :tone="tn('low1Fps', true)" />
        <StatTile :label="t('games.m.low01')" :value="fmtNum(s.low01Fps)" unit="FPS" :tone="tn('low01Fps', true)" />
        <StatTile :label="t('games.m.fps1s')" :value="`${fmtNum(s.fpsMin1s)} / ${fmtNum(s.fpsMax1s)}`" unit="FPS" />
        <StatTile :label="t('games.m.ftAvg')" :value="fmtMs(s.frameTimeAvgMs, 2)" :tone="tn('frameTimeAvgMs', false)" />
        <StatTile :label="t('games.m.ftP50')" :value="fmtMs(s.frameTimeP50Ms, 2)" :tone="tn('frameTimeP50Ms', false)" />
        <StatTile :label="t('games.m.ftP95')" :value="fmtMs(s.frameTimeP95Ms, 2)" :tone="tn('frameTimeP95Ms', false)" />
        <StatTile :label="t('games.m.ftP99')" :value="fmtMs(s.frameTimeP99Ms, 2)" :tone="tn('frameTimeP99Ms', false)" />
      </div>
    </section>

    <section class="group">
      <div class="g-head"><span class="dot stability" /><span class="label">{{ t('games.sec.stability') }}</span></div>
      <div class="tiles">
        <StatTile :label="t('games.m.hitchesH')" :value="fmtNum(s.hitchesPerHour, 1)" :tone="tn('hitchesPerHour', false, 0.15)" />
        <StatTile
          :label="t('games.m.hitchesSev')"
          :value="`${fmtNum(s.hitchesMinor)} / ${fmtNum(s.hitchesMajor)} / ${fmtNum(s.hitchesSevere)}`"
          :sub="t('games.m.hitchesTotal', { count: s.hitches })"
        />
        <StatTile :label="t('games.m.freezes')" :value="fmtNum(s.freezes)" :tone="tn('freezes', false, 0.15)" />
        <StatTile :label="t('games.m.incidents')" :value="fmtNum(s.incidents)" />
        <template v-if="hasPhases">
          <StatTile :label="t('games.m.loadings')" :value="fmtNum(s.phases.loadings)" />
          <StatTile
            :label="t('games.m.loadingAvg')"
            :value="s.phases.loadings ? secs(s.phases.loadingAvgSec) : '—'"
            :tone="r && phaseTotal(r.phases) > 0 ? tone(s.phases.loadingAvgSec, r.phases.loadingAvgSec, false, 0.1) : ''"
          />
          <StatTile :label="t('games.m.loadingMax')" :value="s.phases.loadings ? secs(s.phases.loadingMaxSec) : '—'" />
        </template>
      </div>
      <div class="causes">
        <span class="label">{{ t('games.m.causes') }}</span>
        <ul v-if="s.topCauses.length">
          <li v-for="c in s.topCauses" :key="c.id">
            <span class="c-title">{{ causeLabel(c.id, game) }}</span>
            <span class="chip">× {{ fmtNum(c.count) }}</span>
          </li>
        </ul>
        <p v-else class="note">{{ t('games.m.noCauses') }}</p>
      </div>
    </section>

    <section class="group">
      <div class="g-head"><span class="dot gpu" /><span class="label">{{ t('games.sec.gpu') }}</span></div>
      <div v-if="hasGpu" class="tiles">
        <StatTile :label="t('games.m.gpuUtil')" :value="fmtPct(s.gpuUtilAvgPct)" :sub="maxOf(fmtPct(s.gpuUtilMaxPct))" />
        <StatTile
          :label="t('games.m.gpuTemp')"
          :value="deg(s.gpuTempAvgC)"
          unit="°C"
          :sub="maxOf(`${deg(s.gpuTempMaxC)} °C`)"
          :tone="tn('gpuTempAvgC', false)"
        />
        <StatTile :label="t('games.m.gpuPower')" :value="fmtNum(s.gpuPowerAvgW)" unit="W" :sub="maxOf(`${fmtNum(s.gpuPowerMaxW)} W`)" />
        <StatTile :label="t('games.m.gpuClock')" :value="fmtNum(s.gpuClockAvgMhz)" unit="MHz" />
        <StatTile :label="t('games.m.gpuFan')" :value="fmtPct(s.gpuFanAvgPct)" />
        <StatTile :label="t('games.m.vram')" :value="fmtMb(s.vramAvgMb)" :sub="maxOf(fmtMb(s.vramMaxMb))" />
        <StatTile
          :label="t('games.m.gpuThrottle')"
          :value="fmtPct(s.gpuThrottlePct)"
          :sub="t('games.m.ofTime')"
          :tone="tn('gpuThrottlePct', false, 0.2)"
        />
      </div>
      <p v-else class="note">{{ t('games.gpuNa') }}</p>
    </section>

    <section class="group">
      <div class="g-head"><span class="dot cpu" /><span class="label">{{ t('games.sec.cpu') }}</span></div>
      <div class="tiles">
        <StatTile :label="t('games.m.cpu')" :value="fmtPct(s.cpuAvgPct)" :sub="maxOf(fmtPct(s.cpuMaxPct))" />
        <StatTile :label="t('games.m.gameCpu')" :value="fmtPct(s.gameCpuAvgPct, 1)" />
        <StatTile :label="t('games.m.freqLimit')" :value="fmtPct(s.cpuFreqLimitMinPct)" :tone="s.cpuFreqLimitMinPct < 95 ? 'worse' : ''" />
        <StatTile :label="t('games.m.ram')" :value="fmtMb(s.ramUsedAvgMb)" :sub="maxOf(fmtMb(s.ramUsedMaxMb))" />
        <StatTile :label="t('games.m.gameRam')" :value="fmtMb(s.gameRamMaxMb)" />
        <StatTile
          v-if="s.cpuTempAvgC !== null"
          :label="t('games.m.cpuTemp')"
          :value="fmtNum(s.cpuTempAvgC)"
          unit="°C"
          :sub="maxOf(`${deg(s.cpuTempMaxC)} °C`)"
          :tone="tn('cpuTempAvgC', false)"
        />
      </div>
      <p v-if="s.cpuTempAvgC === null" class="note">{{ t('games.cpuTempNa') }}</p>
    </section>

    <section class="group">
      <div class="g-head"><span class="dot latency" /><span class="label">{{ t('games.sec.latency') }}</span></div>
      <div class="tiles">
        <template v-if="s.fullTracing">
          <StatTile :label="t('games.m.dpc')" :value="fmtUs(s.dpcMaxUs)" :tone="s.dpcMaxUs >= 1000 ? 'worse' : ''" />
          <StatTile :label="t('games.m.isr')" :value="fmtUs(s.isrMaxUs)" :tone="s.isrMaxUs >= 1000 ? 'worse' : ''" />
          <StatTile :label="t('games.m.disk')" :value="fmtMs(s.diskLatMaxMs)" :tone="s.diskLatMaxMs >= 50 ? 'worse' : ''" />
        </template>
        <StatTile :label="t('games.m.fullscreen')" :value="fmtPct(s.fullscreenPct)" :sub="t('games.m.ofTime')" />
      </div>
      <p v-if="!s.fullTracing" class="note">{{ t('games.limited') }}</p>
    </section>
  </div>
</template>

<style scoped>
.grid {
  display: flex;
  flex-direction: column;
  gap: 22px;
}
.phase-bar {
  margin-bottom: 18px;
}
.top-note {
  margin: 0 0 12px;
}
.g-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
}
.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-3);
}
.dot.frames {
  background: var(--text);
}
.dot.stability {
  background: var(--red);
}
.dot.gpu {
  background: var(--l-hardware);
}
.dot.cpu {
  background: var(--l-apps);
}
.dot.latency {
  background: var(--l-kernel);
}
.tiles {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 8px;
}
.tiles.compact {
  grid-template-columns: repeat(auto-fill, minmax(128px, 1fr));
}
.causes {
  margin-top: 12px;
}
.causes ul {
  list-style: none;
  margin: 8px 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.causes li {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  font-size: 13px;
  padding: 8px 12px;
  border-radius: 10px;
  border: 1px solid var(--line);
}
.c-title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.note {
  margin: 10px 0 0;
  font-size: 12px;
  color: var(--text-3);
}
</style>
