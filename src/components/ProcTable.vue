<script setup lang="ts">
import { computed, ref } from 'vue'
import type { ProcLive } from '../lib/contract'
import { t } from '../i18n'
import { fmtMb, fmtPct, fmtRate } from '../lib/format'

const props = defineProps<{ rows: ProcLive[]; compact?: boolean }>()

type Key = 'name' | 'pid' | 'cpuPct' | 'ioBps' | 'workingSetMb'
const sortKey = ref<Key>('cpuPct')
const desc = ref(true)

function sortBy(k: Key) {
  if (props.compact) return
  if (sortKey.value === k) desc.value = !desc.value
  else {
    sortKey.value = k
    desc.value = k !== 'name'
  }
}

const sorted = computed(() => {
  if (props.compact) return props.rows
  const k = sortKey.value
  const dir = desc.value ? -1 : 1
  return [...props.rows].sort((a, b) => {
    const av = a[k]
    const bv = b[k]
    if (typeof av === 'string' && typeof bv === 'string') return av.localeCompare(bv) * dir
    return ((av as number) - (bv as number)) * dir
  })
})

const maxCpu = computed(() => Math.max(5, ...props.rows.map((r) => r.cpuPct)))
const TAG_TONE: Record<string, string> = { game: 'green', blackbox: '', defender: 'amber', windows_update: 'amber', telemetry: 'amber', search_indexer: 'amber', shader_precache: 'amber', cloud_sync: 'amber', wmi: 'amber' }
</script>

<template>
  <table class="table">
    <thead>
      <tr>
        <th><button :class="{ on: sortKey === 'name' }" @click="sortBy('name')">{{ t('col.process') }}</button></th>
        <th v-if="!compact" class="r"><button :class="{ on: sortKey === 'pid' }" @click="sortBy('pid')">{{ t('col.pid') }}</button></th>
        <th>{{ t('col.tag') }}</th>
        <th><button :class="{ on: sortKey === 'cpuPct' }" @click="sortBy('cpuPct')">{{ t('col.cpu') }}</button></th>
        <th class="r"><button :class="{ on: sortKey === 'ioBps' }" @click="sortBy('ioBps')">{{ t('col.io') }}</button></th>
        <th class="r"><button :class="{ on: sortKey === 'workingSetMb' }" @click="sortBy('workingSetMb')">{{ t('col.memory') }}</button></th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="p in sorted" :key="p.pid" :class="{ game: p.tag === 'game', self: p.tag === 'blackbox' }">
        <td class="mono name">
          {{ p.name }}
          <span v-if="compact" class="pid">{{ p.pid }}</span>
        </td>
        <td v-if="!compact" class="r num dim">{{ p.pid }}</td>
        <td><span v-if="p.tag" class="chip" :class="TAG_TONE[p.tag]">{{ t('tag.' + p.tag) }}</span></td>
        <td class="cpu">
          <div class="bar"><div :style="{ width: `${(p.cpuPct / maxCpu) * 100}%` }" /></div>
          <span class="num">{{ fmtPct(p.cpuPct, p.cpuPct < 10 ? 1 : 0) }}</span>
        </td>
        <td class="r num">{{ p.ioBps ? fmtRate(p.ioBps) : '—' }}</td>
        <td class="r num">{{ fmtMb(p.workingSetMb) }}</td>
      </tr>
    </tbody>
  </table>
</template>

<style scoped>
.name {
  font-size: 12.5px;
}
.pid {
  margin-left: 8px;
  font-size: 10.5px;
  color: var(--text-3);
}
.cpu {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 170px;
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
  background: var(--l-apps);
  border-radius: 99px;
  transition: width 0.5s var(--ease);
}
.cpu .num {
  width: 52px;
  text-align: right;
  font-size: 12px;
}
tr.game td.name {
  color: var(--green);
}
tr.self td.name {
  color: var(--text-2);
}
</style>
