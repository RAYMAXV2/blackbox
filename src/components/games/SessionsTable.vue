<script setup lang="ts">
// Every stored session, newest first, tinted against the recent average.
// A row expands into the session's full statistics.
import { computed, ref } from 'vue'
import SessionStatsGrid from './SessionStatsGrid.vue'
import Icon from '../Icon.vue'
import type { SessionStats } from '../../lib/contract'
import { t } from '../../i18n'
import { fmtDateShort, fmtMs, fmtNum, fmtPlaytime } from '../../lib/format'
import { missingAll, tone, view } from '../../lib/games'

/** `sessions` are raw records; `reference` is already resolved for the current scope. */
const props = defineProps<{ sessions: SessionStats[]; reference: SessionStats | null; game: string }>()

const open = ref<string | null>(null)
const rows = computed(() => props.sessions.map((raw) => ({ raw, s: view(raw), missing: missingAll(raw) })))

function toggle(id: string) {
  open.value = open.value === id ? null : id
}

const tn = (v: number | null, key: keyof SessionStats, higherIsBetter: boolean, tol = 0.03) =>
  props.reference ? tone(v, props.reference[key] as number | null, higherIsBetter, tol) : ''
</script>

<template>
  <table class="table">
    <thead>
      <tr>
        <th>{{ t('games.col.date') }}</th>
        <th class="r">{{ t('games.col.duration') }}</th>
        <th class="r">{{ t('games.col.fps') }}</th>
        <th class="r">{{ t('games.col.low1') }}</th>
        <th class="r">{{ t('games.col.p99') }}</th>
        <th class="r">{{ t('games.col.hitchesH') }}</th>
        <th class="r">{{ t('games.col.gpuTemp') }}</th>
        <th class="x" />
      </tr>
    </thead>
    <tbody>
      <template v-for="{ raw, s, missing } in rows" :key="raw.id">
        <tr class="row" :class="{ on: open === raw.id }" tabindex="0" @click="toggle(raw.id)" @keydown.enter="toggle(raw.id)">
          <td class="mono">
            {{ fmtDateShort(s.startTs) }}<span v-if="missing" class="dim old" :title="t('games.scope.noAll')">*</span>
          </td>
          <td class="r num">{{ fmtPlaytime(s.durationSec) }}</td>
          <td class="r num" :class="tn(s.fpsAvg, 'fpsAvg', true)">{{ fmtNum(s.fpsAvg) }}</td>
          <td class="r num" :class="tn(s.low1Fps, 'low1Fps', true)">{{ fmtNum(s.low1Fps) }}</td>
          <td class="r num" :class="tn(s.frameTimeP99Ms, 'frameTimeP99Ms', false)">{{ fmtMs(s.frameTimeP99Ms, 2) }}</td>
          <td class="r num" :class="tn(s.hitchesPerHour, 'hitchesPerHour', false, 0.15)">{{ fmtNum(s.hitchesPerHour, 1) }}</td>
          <td class="r num" :class="tn(s.gpuTempMaxC, 'gpuTempMaxC', false)">
            {{ s.gpuTempMaxC === null ? '—' : `${fmtNum(s.gpuTempMaxC)} °C` }}
          </td>
          <td class="x"><Icon name="arrow" :size="14" class="chev" /></td>
        </tr>
        <tr v-if="open === raw.id" class="expanded">
          <td colspan="8">
            <SessionStatsGrid :stats="s" :game="game" :reference="reference" :note="missing ? t('games.scope.noAll') : ''" />
          </td>
        </tr>
      </template>
    </tbody>
  </table>
</template>

<style scoped>
.row {
  cursor: pointer;
}
.old {
  margin-left: 3px;
}
.row td {
  transition: background 0.15s var(--ease);
}
.row.on td {
  background: var(--surface-2);
}
.better {
  color: var(--green);
}
.worse {
  color: var(--red);
}
.x {
  width: 32px;
  color: var(--text-3);
}
.chev {
  transition: transform 0.18s var(--ease);
}
.row.on .chev {
  transform: rotate(90deg);
  color: var(--text);
}
.expanded td {
  padding: 18px 16px 22px;
  background: transparent;
}
.table tbody tr.expanded:hover td {
  background: transparent;
}
</style>
