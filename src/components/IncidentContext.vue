<script setup lang="ts">
// "At that moment": the machine at the second of the incident and the processes
// that were using it, sorted by whatever matters for this verdict.
import { computed, ref } from 'vue'
import Segmented from './Segmented.vue'
import StatTile from './games/StatTile.vue'
import type { CauseMatch, IncidentContext, IncidentKind } from '../lib/contract'
import { t } from '../i18n'
import { fmtMb, fmtNum, fmtPct, fmtRate } from '../lib/format'

type SortKey = 'cpu' | 'ram' | 'gpu'

const props = defineProps<{ ctx: IncidentContext; kind: IncidentKind; verdict: CauseMatch | null }>()

function defaultSort(): SortKey {
  const id = props.verdict?.id
  if (id === 'mem.paging' || props.kind === 'pressure') return 'ram'
  if (id === 'gpu.thermal' || id === 'gpu.power_limit') return 'gpu'
  return 'cpu'
}

const sort = ref<SortKey>(defaultSort())
const hasGpu = computed(() => props.ctx.processes.some((p) => p.gpuPct !== null))
const options = computed(() => {
  const o: { value: SortKey; label: string }[] = [
    { value: 'cpu', label: 'CPU' },
    { value: 'ram', label: 'RAM' },
  ]
  if (hasGpu.value) o.push({ value: 'gpu', label: 'GPU' })
  return o
})

const rows = computed(() => {
  const list = [...props.ctx.processes]
  const key = sort.value === 'gpu' && !hasGpu.value ? 'cpu' : sort.value
  list.sort((a, b) => (key === 'ram' ? b.ramMb - a.ramMb : key === 'gpu' ? (b.gpuPct ?? -1) - (a.gpuPct ?? -1) : b.cpuPct - a.cpuPct))
  return list
})

/** Process names the verdict talks about (params and evidence), lower-cased. */
const named = computed(() => {
  const out = new Set<string>()
  const v = props.verdict
  if (!v) return out
  const add = (params?: Record<string, string | number>) => {
    for (const val of Object.values(params ?? {})) if (typeof val === 'string') out.add(val.toLowerCase())
  }
  add(v.params)
  for (const e of v.evidence) add(e.params)
  return out
})

const deg = (c: number | null) => (c === null ? '—' : fmtNum(c))
</script>

<template>
  <div class="ctx">
    <div class="tiles">
      <StatTile :label="t('ctx.cpuTemp')" :value="deg(ctx.cpuTempC)" :unit="ctx.cpuTempC === null ? '' : '°C'" :tone="(ctx.cpuTempC ?? 0) >= 90 ? 'worse' : ''" />
      <StatTile :label="t('ctx.gpuTemp')" :value="deg(ctx.gpuTempC)" :unit="ctx.gpuTempC === null ? '' : '°C'" :tone="(ctx.gpuTempC ?? 0) >= 83 ? 'worse' : ''" />
      <StatTile :label="t('ctx.cpu')" :value="fmtPct(ctx.cpuPct)" />
      <StatTile :label="t('ctx.gpu')" :value="fmtPct(ctx.gpuPct)" />
      <StatTile
        :label="t('ctx.ramFree')"
        :value="fmtMb(ctx.memAvailableMb)"
        :sub="t('ctx.memLoad', { pct: fmtPct(ctx.memLoadPct) })"
        :tone="ctx.memLoadPct >= 90 ? 'worse' : ''"
      />
    </div>
    <p v-if="ctx.cpuTempC === null" class="note">{{ t('ctx.cpuTempNa') }}</p>

    <div v-if="ctx.processes.length" class="procs">
      <div class="procs-head">
        <span class="label">{{ t('ctx.processes') }}</span>
        <Segmented v-model="sort" :options="options" :label="t('ctx.sortBy')" />
      </div>
      <table class="table">
        <thead>
          <tr>
            <th>{{ t('ctx.col.name') }}</th>
            <th class="r">CPU</th>
            <th class="r">RAM</th>
            <th class="r">{{ t('ctx.col.disk') }}</th>
            <th v-if="hasGpu" class="r">GPU</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="p in rows" :key="p.pid" :class="{ hit: named.has(p.name.toLowerCase()) }">
            <td class="name mono">
              <i v-if="named.has(p.name.toLowerCase())" class="dot" />{{ p.name }}
              <span v-if="p.tag" class="chip tag">{{ t('tag.' + p.tag) }}</span>
            </td>
            <td class="r num" :class="{ on: sort === 'cpu' }">{{ fmtPct(p.cpuPct, 1) }}</td>
            <td class="r num" :class="{ on: sort === 'ram' }">{{ fmtMb(p.ramMb) }}</td>
            <td class="r num">{{ p.ioBps ? fmtRate(p.ioBps) : '—' }}</td>
            <td v-if="hasGpu" class="r num" :class="{ on: sort === 'gpu' }">{{ fmtPct(p.gpuPct) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.ctx {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.tiles {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 8px;
}
.note {
  margin: -6px 0 0;
  font-size: 12px;
  color: var(--text-3);
}
.procs-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 6px;
}
.name {
  font-size: 12px;
  white-space: nowrap;
}
.dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  margin-right: 8px;
  border-radius: 50%;
  background: var(--red);
  vertical-align: middle;
}
.hit td {
  background: var(--red-soft);
}
.table tbody tr.hit:hover td {
  background: var(--red-soft);
}
.tag {
  margin-left: 8px;
  height: 18px;
  font-size: 9.5px;
}
td.on {
  color: var(--text);
  font-weight: 500;
}
td.num:not(.on) {
  color: var(--text-2);
}
</style>
