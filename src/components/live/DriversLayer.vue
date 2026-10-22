<script setup lang="ts">
import { computed } from 'vue'
import { status } from '../../store'
import { t } from '../../i18n'
import { fmtMs, fmtNum, fmtUs } from '../../lib/format'

const drivers = computed(() => status.value?.drivers ?? [])
// log scale so 20 µs and 2000 µs both read well; 4 ms fills the bar
const barPct = (us: number) => Math.min(100, (Math.log10(Math.max(1, us)) / Math.log10(4000)) * 100)
const tone = (us: number) => (us >= 1000 ? 'bad' : us >= 500 ? 'warn' : '')
</script>

<template>
  <div class="card wrap">
    <table v-if="drivers.length" class="table">
      <thead>
        <tr>
          <th>{{ t('col.driver') }}</th>
          <th>{{ t('col.category') }}</th>
          <th class="r">{{ t('col.dpc') }}</th>
          <th class="r">{{ t('col.isr') }}</th>
          <th>{{ t('col.dpcMax') }}</th>
          <th class="r">{{ t('col.dpcTotal') }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="d in drivers" :key="d.name">
          <td>
            <div class="mono name">{{ d.name }}</div>
            <div class="friendly">{{ d.friendly ?? '' }}</div>
          </td>
          <td><span class="chip">{{ t('cat.' + d.category) }}</span></td>
          <td class="r num">{{ fmtNum(d.dpcCount) }}</td>
          <td class="r num">{{ fmtNum(d.isrCount) }}</td>
          <td class="maxcell">
            <div class="bar"><div :class="tone(Math.max(d.dpcMaxUs, d.isrMaxUs))" :style="{ width: `${barPct(Math.max(d.dpcMaxUs, d.isrMaxUs))}%` }" /></div>
            <span class="num" :class="tone(Math.max(d.dpcMaxUs, d.isrMaxUs))">{{ fmtUs(Math.max(d.dpcMaxUs, d.isrMaxUs)) }}</span>
          </td>
          <td class="r num">{{ fmtMs((d.dpcTotalUs + d.isrTotalUs) / 1000, 1) }}</td>
        </tr>
      </tbody>
    </table>
    <div v-else class="empty">{{ t('drivers.empty') }}</div>
  </div>
</template>

<style scoped>
.wrap {
  overflow: hidden;
}
.name {
  font-size: 12.5px;
}
.friendly {
  font-size: 11.5px;
  color: var(--text-3);
}
.maxcell {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 190px;
}
.bar {
  flex: 1;
  height: 4px;
  border-radius: 99px;
  background: var(--surface-3);
  overflow: hidden;
}
.bar > div {
  height: 100%;
  border-radius: 99px;
  background: var(--l-drivers);
  transition: width 0.5s var(--ease);
}
.bar > div.warn {
  background: var(--amber);
}
.bar > div.bad {
  background: var(--red);
}
.maxcell .num {
  width: 70px;
  text-align: right;
  font-size: 12px;
}
span.warn {
  color: var(--amber);
}
span.bad {
  color: var(--red);
}
.table td {
  padding-top: 8px;
  padding-bottom: 8px;
}
</style>
