<script setup lang="ts">
import { computed } from 'vue'
import Spark from '../Spark.vue'
import { history, status } from '../../store'
import { t } from '../../i18n'
import { fmtNum, fmtPct } from '../../lib/format'

const k = computed(() => (status.value?.mode === 'full' ? status.value.kernel : null))
const tone = (us: number) => (us >= 1000 ? 'bad' : us >= 500 ? 'warn' : '')
</script>

<template>
  <div v-if="k" class="grid">
    <div class="tile card">
      <span class="label">{{ t('kernel.dpcPerSec') }}</span>
      <b class="num">{{ fmtNum(k.dpcPerSec) }}</b>
    </div>
    <div class="tile card">
      <span class="label">{{ t('kernel.isrPerSec') }}</span>
      <b class="num">{{ fmtNum(k.isrPerSec) }}</b>
    </div>
    <div class="tile card">
      <span class="label">{{ t('kernel.dpcMax') }}</span>
      <b class="num" :class="tone(k.dpcMaxUs)">{{ fmtNum(k.dpcMaxUs) }}<small>µs</small></b>
      <Spark :values="history.dpc" :max="1000" tone="kernel" />
    </div>
    <div class="tile card">
      <span class="label">{{ t('kernel.isrMax') }}</span>
      <b class="num" :class="tone(k.isrMaxUs)">{{ fmtNum(k.isrMaxUs) }}<small>µs</small></b>
    </div>
    <div class="tile card">
      <span class="label">{{ t('kernel.dpcTime') }}</span>
      <b class="num">{{ fmtPct(k.dpcTimePct, 2) }}</b>
    </div>
    <div class="tile card">
      <span class="label">{{ t('kernel.isrTime') }}</span>
      <b class="num">{{ fmtPct(k.isrTimePct, 2) }}</b>
    </div>
    <div class="tile card">
      <span class="label">{{ t('kernel.hardFaults') }}</span>
      <b class="num">{{ fmtNum(k.hardFaultsPerSec) }}</b>
    </div>
    <div class="tile card">
      <span class="label">{{ t('kernel.timerRes') }}</span>
      <b class="num">{{ fmtNum(k.timerResolutionMs, 3) }}<small>ms</small></b>
    </div>
  </div>
  <div v-else class="card off">{{ t('kernel.unavailable') }}</div>
</template>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}
.tile {
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  border-radius: 14px;
}
b {
  font-size: 22px;
  font-weight: 500;
  letter-spacing: -0.02em;
}
small {
  margin-left: 4px;
  font-size: 11px;
  color: var(--text-3);
  font-weight: 400;
}
.off {
  padding: 22px 20px;
  font-size: 12.5px;
  color: var(--text-3);
  border-style: dashed;
}
.warn {
  color: var(--amber);
}
.bad {
  color: var(--red);
}
</style>
