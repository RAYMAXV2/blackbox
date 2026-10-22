<script setup lang="ts">
import { computed } from 'vue'
import Led from '../Led.vue'
import Icon from '../Icon.vue'
import { causeTitle, incidents, navigate, now, openIncident } from '../../store'
import { t } from '../../i18n'
import { fmtMagnitude, fmtRelative } from '../../lib/format'

const recent = computed(() => incidents.value.slice(0, 4))
</script>

<template>
  <section class="recent">
    <div class="head">
      <span class="label">{{ t('live.recent') }}</span>
      <button v-if="incidents.length" class="btn small ghost" @click="navigate('incidents')">
        {{ t('live.recent.all') }} <Icon name="arrow" :size="14" />
      </button>
    </div>
    <div v-if="recent.length" class="row">
      <button v-for="i in recent" :key="i.id" class="card item" @click="openIncident(i.id)">
        <div class="top">
          <Led :state="i.severity === 'severe' ? 'bad' : i.severity === 'major' ? 'warn' : 'idle'" :size="7" />
          <span class="label">{{ t('kind.' + i.kind) }}</span>
          <span v-if="!i.read" class="new" />
          <span class="when mono">{{ fmtRelative(i.ts, now) }}</span>
        </div>
        <div class="mag num">{{ fmtMagnitude(i.magnitude) }}</div>
        <div class="title">{{ causeTitle(i.causes[0]) || t('incident.noCause') }}</div>
      </button>
    </div>
    <div v-else class="card empty">{{ t('live.recent.none') }}</div>
  </section>
</template>

<style scoped>
.recent {
  margin: 18px 0 30px;
}
.head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 10px;
  min-height: 26px;
}
.row {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 12px;
}
.item {
  text-align: left;
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  transition: border-color 0.15s var(--ease), transform 0.15s var(--ease);
}
.item:hover {
  border-color: var(--line-strong);
  transform: translateY(-1px);
}
.top {
  display: flex;
  align-items: center;
  gap: 8px;
}
.when {
  margin-left: auto;
  white-space: nowrap;
  font-size: 10.5px;
  color: var(--text-3);
}
.new {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--red);
}
.mag {
  font-size: 20px;
}
.title {
  font-size: 12.5px;
  color: var(--text-2);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
</style>
