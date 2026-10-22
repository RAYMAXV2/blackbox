<script setup lang="ts">
import { computed, ref } from 'vue'
import DotMatrix from '../DotMatrix.vue'
import Led from '../Led.vue'
import Icon from '../Icon.vue'
import { status, updateSettings } from '../../store'
import { call } from '../../lib/ipc'
import { t } from '../../i18n'

const health = computed(() => status.value?.health ?? 'idle')
const mode = computed(() => status.value?.mode ?? 'full')
const reasons = computed(() => status.value?.healthReasons ?? [])
const color = computed(() =>
  health.value === 'bad' ? 'var(--red)' : health.value === 'warn' ? 'var(--amber)' : health.value === 'idle' ? 'var(--text-3)' : 'var(--text)',
)
const restarting = ref(false)

async function restart() {
  restarting.value = true
  try {
    await call('restart_elevated')
  } finally {
    restarting.value = false
  }
}
</script>

<template>
  <article class="card health" :class="health">
    <div class="head">
      <span class="label">{{ t('health.title') }}</span>
      <Led :state="health" :pulse="health !== 'ok' && health !== 'idle'" />
    </div>

    <div class="word">
      <DotMatrix :text="t('health.word.' + health)" :dot="7" :color="color" ghost />
    </div>
    <p class="sub">{{ t('health.sub.' + health) }}</p>

    <ul v-if="reasons.length" class="reasons">
      <li v-for="(r, i) in reasons" :key="i">{{ t(r.key, r.params) }}</li>
    </ul>

    <div class="mode" :class="mode">
      <div class="mode-top">
        <span class="chip" :class="mode === 'full' ? 'green' : mode === 'limited' ? 'amber' : ''">{{ t('mode.' + mode) }}</span>
      </div>
      <p>{{ t('mode.' + mode + '.hint') }}</p>
      <button v-if="mode === 'limited'" class="btn primary" :disabled="restarting" @click="restart">
        <Icon name="shield" :size="15" />
        {{ t('action.restartAdmin') }}
      </button>
      <button v-else-if="mode === 'paused'" class="btn" @click="updateSettings({ paused: false })">
        <Icon name="play" :size="14" />
        {{ t('action.resume') }}
      </button>
    </div>
  </article>
</template>

<style scoped>
.health {
  padding: 22px 24px;
  display: flex;
  flex-direction: column;
  min-height: 300px;
}
.head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.word {
  margin-top: 26px;
  min-height: 49px;
  overflow: hidden;
}
.sub {
  margin: 14px 0 0;
  font-size: 15px;
  color: var(--text-2);
}
.reasons {
  margin: 14px 0 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.reasons li {
  position: relative;
  padding-left: 16px;
  font-size: 12.5px;
  color: var(--text);
}
.reasons li::before {
  content: '';
  position: absolute;
  left: 2px;
  top: 8px;
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--amber);
}
.health.bad .reasons li::before {
  background: var(--red);
}
.mode {
  margin-top: auto;
  padding-top: 18px;
  border-top: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-start;
}
.mode p {
  margin: 0;
  font-size: 12px;
  color: var(--text-3);
}
</style>
