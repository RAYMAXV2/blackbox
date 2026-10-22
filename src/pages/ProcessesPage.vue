<script setup lang="ts">
import { computed, ref } from 'vue'
import ProcTable from '../components/ProcTable.vue'
import Icon from '../components/Icon.vue'
import { call } from '../lib/ipc'
import { useLiveQuery } from '../lib/useLiveQuery'
import type { ProcLive } from '../lib/contract'
import { t } from '../i18n'

const { data } = useLiveQuery<ProcLive[]>(() => call('get_processes'), [], 1000)
const q = ref('')
const rows = computed(() => {
  const needle = q.value.trim().toLowerCase()
  return needle ? data.value.filter((p) => p.name.toLowerCase().includes(needle) || String(p.pid).includes(needle)) : data.value
})
</script>

<template>
  <div class="page">
    <header class="page-head">
      <div>
        <h1>{{ t('processes.title') }}</h1>
        <p>{{ t('processes.sub') }}</p>
      </div>
      <label class="search">
        <Icon name="search" :size="15" />
        <input v-model="q" class="input" :placeholder="t('processes.search')" />
      </label>
    </header>
    <div class="card wrap">
      <ProcTable v-if="rows.length" :rows="rows" />
      <div v-else class="empty">{{ t('processes.empty') }}</div>
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
</style>
