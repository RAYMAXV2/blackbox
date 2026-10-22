<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import Led from '../components/Led.vue'
import IncidentDetail from '../components/IncidentDetail.vue'
import { causeTitle, clearIncidents, incidents, markRead, now, selectedId, unread } from '../store'
import type { IncidentKind, Severity } from '../lib/contract'
import { t } from '../i18n'
import { fmtMagnitude, fmtRelative } from '../lib/format'

const KINDS: IncidentKind[] = ['hitch', 'freeze', 'latency', 'hang', 'throttle', 'pressure']
const SEVERITIES: Severity[] = ['severe', 'major', 'minor']

const kind = ref<IncidentKind | null>(null)
const severity = ref<Severity | null>(null)

const list = computed(() =>
  incidents.value.filter((i) => (!kind.value || i.kind === kind.value) && (!severity.value || i.severity === severity.value)),
)
const presentKinds = computed(() => KINDS.filter((k) => incidents.value.some((i) => i.kind === k)))

// select the newest incident by default
watch(
  list,
  (l) => {
    if (!selectedId.value && l.length) selectedId.value = l[0].id
  },
  { immediate: true },
)

function clearAll() {
  if (window.confirm(t('incidents.clearConfirm'))) clearIncidents()
}
</script>

<template>
  <div class="incidents">
    <aside class="list">
      <div class="list-head">
        <h2>{{ t('incidents.title') }}</h2>
        <div class="actions">
          <button v-if="unread" class="btn small" @click="markRead(null)">{{ t('incidents.markAllRead') }}</button>
          <button v-if="incidents.length" class="btn small danger" @click="clearAll">{{ t('incidents.clear') }}</button>
        </div>
      </div>

      <div class="filters">
        <button class="chip" :class="{ on: kind === null }" @click="kind = null">{{ t('incidents.all') }}</button>
        <button v-for="k in presentKinds" :key="k" class="chip" :class="{ on: kind === k }" @click="kind = kind === k ? null : k">
          {{ t('kind.' + k) }}
        </button>
      </div>
      <div class="filters">
        <button v-for="s in SEVERITIES" :key="s" class="chip" :class="{ on: severity === s }" @click="severity = severity === s ? null : s">
          {{ t('severity.' + s) }}
        </button>
      </div>

      <div class="items">
        <button
          v-for="i in list"
          :key="i.id"
          class="item"
          :class="{ on: selectedId === i.id, unread: !i.read }"
          @click="selectedId = i.id"
        >
          <div class="row1">
            <Led :state="i.severity === 'severe' ? 'bad' : i.severity === 'major' ? 'warn' : 'idle'" :size="7" />
            <span class="label">{{ t('kind.' + i.kind) }}</span>
            <span class="mag num">{{ fmtMagnitude(i.magnitude) }}</span>
            <span class="when mono">{{ fmtRelative(i.ts, now) }}</span>
          </div>
          <div class="title">{{ causeTitle(i.causes[0]) || t('incident.noCause') }}</div>
          <div v-if="i.app" class="app mono">{{ i.app.name }}</div>
        </button>
        <div v-if="!list.length" class="empty">{{ incidents.length ? t('incidents.emptyFilter') : t('incidents.empty') }}</div>
      </div>
    </aside>

    <section class="detail">
      <IncidentDetail v-if="selectedId" :id="selectedId" :key="selectedId" />
      <div v-else class="placeholder">{{ t('incidents.select') }}</div>
    </section>
  </div>
</template>

<style scoped>
.incidents {
  height: 100%;
  display: grid;
  grid-template-columns: 360px minmax(0, 1fr);
}
@media (max-width: 1100px) {
  .incidents {
    grid-template-columns: 300px minmax(0, 1fr);
  }
}
.list {
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-right: 1px solid var(--line);
  background: var(--bg);
}
.list-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 22px 18px 12px 22px;
}
h2 {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.02em;
}
.actions {
  display: flex;
  gap: 6px;
}
.filters {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 0 18px 10px 22px;
}
.items {
  flex: 1;
  overflow-y: auto;
  padding: 4px 10px 20px;
}
.item {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 12px;
  border: 1px solid transparent;
  border-radius: 14px;
  background: transparent;
  text-align: left;
  position: relative;
}
.item:hover {
  background: var(--surface-2);
}
.item.on {
  background: var(--surface);
  border-color: var(--line-strong);
}
.item.unread::after {
  content: '';
  position: absolute;
  right: 12px;
  bottom: 14px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--red);
}
.row1 {
  display: flex;
  align-items: center;
  gap: 8px;
}
.mag {
  font-size: 12.5px;
}
.when {
  margin-left: auto;
  white-space: nowrap;
  font-size: 10.5px;
  color: var(--text-3);
}
.title {
  font-size: 13px;
  line-height: 1.4;
  padding-right: 14px;
}
.app {
  font-size: 11px;
  color: var(--text-3);
}
.detail {
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
}
.placeholder {
  height: 100%;
  display: grid;
  place-items: center;
  color: var(--text-3);
}
</style>
