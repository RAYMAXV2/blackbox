<script setup lang="ts">
import { computed, ref } from 'vue'
import MiniHist from '../components/MiniHist.vue'
import Icon from '../components/Icon.vue'
import { call } from '../lib/ipc'
import { useLiveQuery } from '../lib/useLiveQuery'
import type { DriverInfo } from '../lib/contract'
import { t } from '../i18n'
import { fmtMs, fmtNum, fmtUs } from '../lib/format'

const { data } = useLiveQuery<DriverInfo[]>(() => call('get_drivers'), [])

type Key = 'name' | 'category' | 'dpcCount' | 'dpcTotalUs' | 'dpcMaxUs' | 'isrCount' | 'isrMaxUs'
const sortKey = ref<Key>('dpcTotalUs')
const desc = ref(true)
const q = ref('')

function sortBy(k: Key) {
  if (sortKey.value === k) desc.value = !desc.value
  else {
    sortKey.value = k
    desc.value = k !== 'name' && k !== 'category'
  }
}

const rows = computed(() => {
  const needle = q.value.trim().toLowerCase()
  const k = sortKey.value
  const dir = desc.value ? -1 : 1
  return data.value
    .filter((d) => !needle || d.name.toLowerCase().includes(needle) || (d.friendly ?? '').toLowerCase().includes(needle) || t('cat.' + d.category).toLowerCase().includes(needle))
    .sort((a, b) => {
      const av = a[k]
      const bv = b[k]
      if (typeof av === 'string' && typeof bv === 'string') return av.localeCompare(bv) * dir
      return ((av as number) - (bv as number)) * dir
    })
})

const tone = (us: number) => (us >= 1000 ? 'bad' : us >= 500 ? 'warn' : '')
const cols: { key: Key; label: string; right?: boolean }[] = [
  { key: 'name', label: 'col.driver' },
  { key: 'category', label: 'col.category' },
  { key: 'dpcCount', label: 'col.dpc', right: true },
  { key: 'dpcTotalUs', label: 'col.dpcTotal', right: true },
  { key: 'dpcMaxUs', label: 'col.dpcMax', right: true },
  { key: 'isrCount', label: 'col.isr', right: true },
  { key: 'isrMaxUs', label: 'col.isrMax', right: true },
]
</script>

<template>
  <div class="page">
    <header class="page-head">
      <div>
        <h1>{{ t('drivers.title') }}</h1>
        <p>{{ t('drivers.sub') }}</p>
      </div>
      <label class="search">
        <Icon name="search" :size="15" />
        <input v-model="q" class="input" :placeholder="t('drivers.search')" />
      </label>
    </header>

    <div class="card wrap">
      <table v-if="rows.length" class="table">
        <thead>
          <tr>
            <th v-for="c in cols" :key="c.key" :class="{ r: c.right }">
              <button :class="{ on: sortKey === c.key }" @click="sortBy(c.key)">
                {{ t(c.label) }}<span v-if="sortKey === c.key" class="arrow">{{ desc ? '↓' : '↑' }}</span>
              </button>
            </th>
            <th>{{ t('col.histogram') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="d in rows" :key="d.name" :class="{ unloaded: !d.loaded }">
            <td>
              <div class="mono name">{{ d.name }}<span v-if="!d.loaded" class="chip">{{ t('drivers.unloaded') }}</span></div>
              <div class="friendly">{{ d.friendly ?? '' }}</div>
            </td>
            <td><span class="chip">{{ t('cat.' + d.category) }}</span></td>
            <td class="r num">{{ d.dpcCount ? fmtNum(d.dpcCount) : '—' }}</td>
            <td class="r num">{{ fmtMs(d.dpcTotalUs / 1000, 1) }}</td>
            <td class="r num" :class="tone(d.dpcMaxUs)">{{ d.dpcCount ? fmtUs(d.dpcMaxUs) : '—' }}</td>
            <td class="r num">{{ d.isrCount ? fmtNum(d.isrCount) : '—' }}</td>
            <td class="r num" :class="tone(d.isrMaxUs)">{{ d.isrCount ? fmtUs(d.isrMaxUs) : '—' }}</td>
            <td><MiniHist :values="d.histogram" /></td>
          </tr>
        </tbody>
      </table>
      <div v-else class="empty">{{ t('drivers.empty') }}</div>
    </div>
  </div>
</template>

<style scoped>
h1 {
  margin: 0;
  font-size: 26px;
  font-weight: 600;
  letter-spacing: -0.02em;
}
.search {
  position: relative;
  display: flex;
  align-items: center;
  color: var(--text-3);
}
.search .icon {
  position: absolute;
  left: 12px;
}
.search .input {
  padding-left: 34px;
  width: 260px;
}
.wrap {
  overflow: auto;
}
.name {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
}
.friendly {
  font-size: 11.5px;
  color: var(--text-3);
}
.arrow {
  margin-left: 4px;
}
.warn {
  color: var(--amber);
}
.bad {
  color: var(--red);
}
tr.unloaded td {
  opacity: 0.55;
}
</style>
