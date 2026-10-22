<script setup lang="ts">
import { computed, ref } from 'vue'
import Icon from '../components/Icon.vue'
import { kb } from '../store'
import type { Consequence, KbCategory, KnowledgeEntry } from '../lib/contract'
import { lk, t } from '../i18n'

const q = ref('')
const category = ref<KbCategory | null>(null)
const consequence = ref<Consequence | null>(null)

const categories = computed(() => [...new Set(kb.value.map((e) => e.category))])
const consequences = computed(() => [...new Set(kb.value.flatMap((e) => e.consequences))])

/** Split a KB text into plain parts and {placeholder} chips. */
function parts(text: string) {
  return text.split(/(\{\w+\})/g).filter(Boolean).map((p) => {
    const m = /^\{(\w+)\}$/.exec(p)
    return m ? { ph: true, text: t('param.' + m[1]) } : { ph: false, text: p }
  })
}

function haystack(e: KnowledgeEntry) {
  return [e.id, lk(e.title), lk(e.explain), ...e.fixes.map(lk), ...e.consequences.map((c) => t('cons.' + c)), t('kbcat.' + e.category)]
    .join(' ')
    .toLowerCase()
}

const list = computed(() => {
  const needle = q.value.trim().toLowerCase()
  return kb.value.filter(
    (e) =>
      (!category.value || e.category === category.value) &&
      (!consequence.value || e.consequences.includes(consequence.value)) &&
      (!needle || haystack(e).includes(needle)),
  )
})
</script>

<template>
  <div class="page">
    <header class="page-head">
      <div>
        <h1>{{ t('causes.title') }}</h1>
        <p>{{ t('causes.sub') }}</p>
      </div>
      <label class="search">
        <Icon name="search" :size="15" />
        <input v-model="q" class="input" :placeholder="t('causes.search')" />
      </label>
    </header>

    <div class="filters">
      <span class="label">{{ t('causes.category') }}</span>
      <button class="chip" :class="{ on: category === null }" @click="category = null">{{ t('incidents.all') }}</button>
      <button v-for="c in categories" :key="c" class="chip" :class="{ on: category === c }" @click="category = category === c ? null : c">
        {{ t('kbcat.' + c) }}
      </button>
    </div>
    <div class="filters">
      <span class="label">{{ t('causes.consequence') }}</span>
      <button class="chip" :class="{ on: consequence === null }" @click="consequence = null">{{ t('incidents.all') }}</button>
      <button v-for="c in consequences" :key="c" class="chip" :class="{ on: consequence === c }" @click="consequence = consequence === c ? null : c">
        {{ t('cons.' + c) }}
      </button>
    </div>

    <p class="count mono">{{ t('causes.count', { n: list.length }) }}</p>

    <div class="list">
      <article v-for="e in list" :key="e.id" class="card entry">
        <div class="cause">
          <div class="meta">
            <span class="chip">{{ t('kbcat.' + e.category) }}</span>
            <span class="mono id">{{ e.id }}</span>
          </div>
          <h3>
            <template v-for="(p, i) in parts(lk(e.title))" :key="i">
              <span v-if="p.ph" class="ph">{{ p.text }}</span>
              <template v-else>{{ p.text }}</template>
            </template>
          </h3>
          <p class="explain">
            <template v-for="(p, i) in parts(lk(e.explain))" :key="i">
              <span v-if="p.ph" class="ph">{{ p.text }}</span>
              <template v-else>{{ p.text }}</template>
            </template>
          </p>
        </div>
        <div class="effect">
          <span class="label arrow">→ {{ t('causes.leadsTo') }}</span>
          <div class="cons">
            <span v-for="c in e.consequences" :key="c" class="chip red">{{ t('cons.' + c) }}</span>
          </div>
        </div>
        <ol class="fixes">
          <li v-for="(f, i) in e.fixes" :key="i">
            <span class="n mono">{{ String(i + 1).padStart(2, '0') }}</span>
            <span>
              <template v-for="(p, j) in parts(lk(f))" :key="j">
                <span v-if="p.ph" class="ph">{{ p.text }}</span>
                <template v-else>{{ p.text }}</template>
              </template>
            </span>
          </li>
        </ol>
      </article>
      <div v-if="!list.length" class="card empty">{{ t('causes.empty') }}</div>
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
  width: 300px;
}
.filters {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin-bottom: 10px;
}
.filters .label {
  width: 92px;
}
.count {
  margin: 14px 0 12px;
  font-size: 11px;
  color: var(--text-3);
}
.list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.entry {
  display: grid;
  grid-template-columns: minmax(0, 1.4fr) 180px minmax(0, 1.2fr);
  gap: 24px;
  padding: 18px 22px;
}
@media (max-width: 1200px) {
  .entry {
    grid-template-columns: 1fr;
    gap: 12px;
  }
}
.meta {
  display: flex;
  align-items: center;
  gap: 8px;
}
.id {
  font-size: 10.5px;
  color: var(--text-3);
}
h3 {
  margin: 10px 0 6px;
  font-size: 15.5px;
  font-weight: 600;
  line-height: 1.35;
}
.explain {
  margin: 0;
  font-size: 12.5px;
  color: var(--text-2);
  line-height: 1.55;
}
.ph {
  display: inline-block;
  padding: 0 6px;
  margin: 0 1px;
  border-radius: 6px;
  background: var(--surface-3);
  border: 1px solid var(--line-strong);
  font-family: var(--font-mono);
  font-size: 0.82em;
  color: var(--text-2);
  line-height: 1.5;
  vertical-align: 1px;
}
.effect {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding-top: 4px;
}
.arrow {
  color: var(--red);
}
.cons {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.fixes {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.fixes li {
  display: flex;
  gap: 12px;
  font-size: 12.5px;
  line-height: 1.5;
}
.fixes .n {
  color: var(--text-3);
  font-size: 11px;
  padding-top: 1px;
}
</style>
