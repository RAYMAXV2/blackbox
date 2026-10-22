<script setup lang="ts">
import { computed, onMounted, ref, shallowRef } from 'vue'
import DotMatrix from './DotMatrix.vue'
import Icon from './Icon.vue'
import TimelineChart from './TimelineChart.vue'
import { causeExplain, causeFixes, causeTitle, deleteIncident, incidents, kbEntry, markRead } from '../store'
import { call } from '../lib/ipc'
import type { CauseMatch, Incident } from '../lib/contract'
import { t } from '../i18n'
import { fmtDateTime, fmtNum, fmtPct } from '../lib/format'

const props = defineProps<{ id: string }>()

const inc = shallowRef<Incident | null>(null)
const loading = ref(true)
const exported = ref<string | null>(null)

const summary = computed(() => incidents.value.find((i) => i.id === props.id) ?? null)
const verdict = computed<CauseMatch | null>(() => inc.value?.causes[0] ?? null)
const others = computed(() => inc.value?.causes.slice(1) ?? [])

const magnitudeText = computed(() => {
  const m = inc.value?.magnitude
  if (!m) return ''
  return m.unit === 'pct' ? fmtNum(m.value) : fmtNum(m.value, m.value < 10 ? 1 : 0).replace(/\s/g, '')
})
const magnitudeUnit = computed(() => {
  const m = inc.value?.magnitude
  return m ? t('unit.' + m.unit) : ''
})

function confidence(c: number) {
  if (c >= 0.75) return { key: 'conf.high', tone: 'green' }
  if (c >= 0.45) return { key: 'conf.medium', tone: 'amber' }
  return { key: 'conf.low', tone: '' }
}

onMounted(async () => {
  try {
    inc.value = await call('get_incident', { id: props.id })
  } finally {
    loading.value = false
  }
  if (summary.value && !summary.value.read) markRead(props.id)
})

async function exportJson() {
  exported.value = await call('export_incident', { id: props.id })
}
</script>

<template>
  <div v-if="loading" class="pad dim">{{ t('incident.loading') }}</div>
  <div v-else-if="!inc" class="pad dim">{{ t('incidents.select') }}</div>
  <article v-else class="pad detail">
    <header class="top">
      <div class="meta">
        <span class="chip" :class="inc.severity === 'severe' ? 'red' : inc.severity === 'major' ? 'amber' : ''">
          {{ t('severity.' + inc.severity) }}
        </span>
        <span class="chip">{{ t('kind.' + inc.kind) }}</span>
        <span class="mono dim when">{{ fmtDateTime(inc.ts) }}</span>
        <span v-if="inc.app" class="mono app">{{ inc.app.name }}</span>
      </div>
      <div class="actions">
        <button class="btn small" @click="exportJson"><Icon name="export" :size="14" />{{ t('incident.export') }}</button>
        <button class="btn small danger" @click="deleteIncident(inc.id)"><Icon name="trash" :size="14" />{{ t('incident.delete') }}</button>
      </div>
    </header>
    <p v-if="exported" class="exported mono">{{ t('incident.exported', { path: exported }) }}</p>

    <section class="hero">
      <div class="mag">
        <DotMatrix :text="magnitudeText" :dot="7" :color="inc.severity === 'severe' ? 'var(--red)' : 'var(--text)'" />
        <span class="unit mono">{{ magnitudeUnit }}</span>
      </div>
      <div class="verdict">
        <span class="label">{{ t('incident.verdict') }}</span>
        <template v-if="verdict">
          <h2>{{ causeTitle(verdict) }}</h2>
          <div class="conf">
            <span class="chip" :class="confidence(verdict.confidence).tone">{{ t(confidence(verdict.confidence).key) }}</span>
            <span class="mono dim">{{ fmtPct(verdict.confidence * 100) }}</span>
            <span v-for="c in kbEntry(verdict.id)?.consequences ?? []" :key="c" class="chip">{{ t('cons.' + c) }}</span>
          </div>
          <p class="explain">{{ causeExplain(verdict) }}</p>
        </template>
        <h2 v-else>{{ t('incident.noCause') }}</h2>
      </div>
    </section>

    <section v-if="inc.timeline" class="card block">
      <div class="block-head"><span class="label">{{ t('incident.timeline') }}</span></div>
      <TimelineChart :tl="inc.timeline" />
    </section>

    <section class="two">
      <div class="card block">
        <div class="block-head"><span class="label">{{ t('incident.evidence') }}</span></div>
        <ul class="facts">
          <li v-for="(e, i) in verdict?.evidence ?? []" :key="i" class="yes">
            <Icon name="check" :size="15" />
            <span>{{ t(e.key, e.params) }}</span>
          </li>
        </ul>
      </div>
      <div class="card block">
        <div class="block-head"><span class="label">{{ t('incident.ruledOut') }}</span></div>
        <ul class="facts">
          <li v-for="(r, i) in inc.ruledOut" :key="i" class="no">
            <Icon name="x" :size="15" />
            <span>{{ t(r.key, r.params) }}</span>
          </li>
        </ul>
      </div>
    </section>

    <section v-if="verdict" class="card block">
      <div class="block-head"><span class="label">{{ t('incident.fixes') }}</span></div>
      <ol class="fixes">
        <li v-for="(f, i) in causeFixes(verdict)" :key="i">
          <span class="n mono">{{ String(i + 1).padStart(2, '0') }}</span>
          <span>{{ f }}</span>
        </li>
      </ol>
    </section>

    <section v-if="others.length" class="card block">
      <div class="block-head"><span class="label">{{ t('incident.alsoSeen') }}</span></div>
      <div v-for="c in others" :key="c.id" class="other">
        <div class="other-top">
          <span class="other-title">{{ causeTitle(c) }}</span>
          <span class="mono dim">{{ fmtPct(c.confidence * 100) }}</span>
        </div>
        <ul class="facts small">
          <li v-for="(e, i) in c.evidence" :key="i" class="yes">
            <Icon name="check" :size="13" />
            <span>{{ t(e.key, e.params) }}</span>
          </li>
        </ul>
      </div>
    </section>
  </article>
</template>

<style scoped>
.pad {
  padding: 24px 32px 48px;
}
.detail {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}
.meta {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.when,
.app {
  font-size: 11.5px;
}
.app {
  color: var(--text-2);
}
.actions {
  display: flex;
  gap: 6px;
}
.exported {
  margin: -6px 0 0;
  font-size: 11px;
  color: var(--green);
  user-select: text;
}
.hero {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 32px;
  align-items: start;
  padding: 10px 0 6px;
}
.mag {
  display: flex;
  align-items: flex-end;
  gap: 10px;
  padding-top: 26px;
}
.unit {
  font-size: 15px;
  color: var(--text-2);
  line-height: 1;
}
.verdict h2 {
  margin: 8px 0 10px;
  font-size: 24px;
  line-height: 1.25;
  font-weight: 600;
  letter-spacing: -0.02em;
}
.conf {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.explain {
  margin: 12px 0 0;
  color: var(--text-2);
  max-width: 760px;
  font-size: 14px;
  line-height: 1.6;
  user-select: text;
}
.block {
  padding: 16px 20px;
}
.block-head {
  margin-bottom: 12px;
}
.two {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
}
.facts {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.facts li {
  display: flex;
  gap: 10px;
  align-items: flex-start;
  font-size: 13px;
}
.facts li .icon {
  margin-top: 2px;
}
.facts .yes .icon {
  color: var(--green);
}
.facts .no {
  color: var(--text-2);
}
.facts .no .icon {
  color: var(--text-3);
}
.facts.small li {
  font-size: 12px;
  color: var(--text-2);
}
.fixes {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.fixes li {
  display: flex;
  gap: 14px;
  font-size: 13.5px;
  user-select: text;
}
.fixes .n {
  color: var(--red);
  font-size: 12px;
  padding-top: 1px;
}
.other {
  padding: 10px 0;
  border-top: 1px solid var(--line);
}
.other:first-of-type {
  border-top: 0;
  padding-top: 0;
}
.other-top {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 6px;
}
.other-title {
  font-size: 13.5px;
}
</style>
