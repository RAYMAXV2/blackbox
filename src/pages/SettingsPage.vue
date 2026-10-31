<script setup lang="ts">
import { computed, ref } from 'vue'
import Segmented from '../components/Segmented.vue'
import Toggle from '../components/Toggle.vue'
import DotMatrix from '../components/DotMatrix.vue'
import Icon from '../components/Icon.vue'
import { settings, system, updateSettings } from '../store'
import { call } from '../lib/ipc'
import type { Lang, Settings, Severity, Theme } from '../lib/contract'
import { t } from '../i18n'

const s = computed(() => settings.value)
const busy = ref(false)

const langs = [
  { value: 'en' as Lang, label: 'English' },
  { value: 'fr' as Lang, label: 'Français' },
]
const themes = computed(() => (['dark', 'light', 'system'] as Theme[]).map((v) => ({ value: v, label: t('theme.' + v) })))
const overlays = computed(() => (['off', 'smart', 'always'] as Settings['notify']['overlay'][]).map((v) => ({ value: v, label: t('overlay.' + v) })))
const severities = computed(() => (['minor', 'major', 'severe'] as Severity[]).map((v) => ({ value: v, label: t('severity.' + v) })))
const sens = computed(() => (['low', 'normal', 'high'] as Settings['sensitivity'][]).map((v) => ({ value: v, label: t('sens.' + v) })))
const retentions = [50, 100, 200, 500, 1000].map((v) => ({ value: v, label: String(v) }))

async function test() {
  await call('test_notification')
}

async function openData() {
  await call('open_data_folder')
}

async function setAutostart(v: boolean) {
  busy.value = true
  try {
    await updateSettings({ autostart: v })
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="page">
    <header class="page-head">
      <div>
        <h1>{{ t('settings.title') }}</h1>
        <p>{{ t('settings.sub') }}</p>
      </div>
    </header>

    <div class="cols">
      <div class="col">
        <section class="card group">
          <h2 class="label">{{ t('settings.general') }}</h2>
          <div class="row">
            <div class="txt"><div class="name">{{ t('settings.language') }}</div></div>
            <Segmented :model-value="s.lang" :options="langs" :label="t('settings.language')" @update:model-value="updateSettings({ lang: $event })" />
          </div>
          <div class="row">
            <div class="txt"><div class="name">{{ t('settings.theme') }}</div></div>
            <Segmented :model-value="s.theme" :options="themes" :label="t('settings.theme')" @update:model-value="updateSettings({ theme: $event })" />
          </div>
        </section>

        <section class="card group">
          <h2 class="label">{{ t('settings.notifications') }}</h2>
          <div class="row stack">
            <div class="txt">
              <div class="name">{{ t('settings.overlay') }}</div>
              <div class="desc">{{ t('settings.overlay.desc') }}</div>
            </div>
            <Segmented
              :model-value="s.notify.overlay"
              :options="overlays"
              :label="t('settings.overlay')"
              @update:model-value="updateSettings({ notify: { overlay: $event } })"
            />
            <p class="hint">{{ t('overlay.' + s.notify.overlay + '.desc') }}</p>
          </div>
          <div class="row">
            <div class="txt">
              <div class="name">{{ t('settings.inApp') }}</div>
              <div class="desc">{{ t('settings.inApp.desc') }}</div>
            </div>
            <Toggle :model-value="s.notify.inApp" :label="t('settings.inApp')" @update:model-value="updateSettings({ notify: { inApp: $event } })" />
          </div>
          <div class="row">
            <div class="txt">
              <div class="name">{{ t('settings.minSeverity') }}</div>
              <div class="desc">{{ t('settings.minSeverity.desc') }}</div>
            </div>
            <Segmented
              :model-value="s.notify.minSeverity"
              :options="severities"
              :label="t('settings.minSeverity')"
              @update:model-value="updateSettings({ notify: { minSeverity: $event } })"
            />
          </div>
          <div class="row">
            <div class="txt"><div class="name">{{ t('settings.test') }}</div></div>
            <button class="btn" @click="test"><Icon name="bell" :size="14" />{{ t('settings.test') }}</button>
          </div>
        </section>
      </div>

      <div class="col">
        <section class="card group">
          <h2 class="label">{{ t('settings.detection') }}</h2>
          <div class="row">
            <div class="txt">
              <div class="name">{{ t('settings.sensitivity') }}</div>
              <div class="desc">{{ t('settings.sensitivity.desc') }}</div>
            </div>
            <Segmented :model-value="s.sensitivity" :options="sens" :label="t('settings.sensitivity')" @update:model-value="updateSettings({ sensitivity: $event })" />
          </div>
        </section>

        <section class="card group">
          <h2 class="label">{{ t('settings.recording') }}</h2>
          <div class="row">
            <div class="txt">
              <div class="name">{{ t('settings.pause') }}</div>
              <div class="desc">{{ t('settings.pause.desc') }}</div>
            </div>
            <Toggle :model-value="s.paused" :label="t('settings.pause')" @update:model-value="updateSettings({ paused: $event })" />
          </div>
          <div class="row">
            <div class="txt">
              <div class="name">{{ t('settings.autostart') }}</div>
              <div class="desc">{{ t('settings.autostart.desc') }}</div>
            </div>
            <Toggle :model-value="s.autostart" :label="t('settings.autostart')" :class="{ busy }" @update:model-value="setAutostart" />
          </div>
          <div class="row">
            <div class="txt">
              <div class="name">{{ t('settings.retention') }}</div>
              <div class="desc">{{ t('settings.retention.desc') }}</div>
            </div>
            <Segmented :model-value="s.retention" :options="retentions" :label="t('settings.retention')" @update:model-value="updateSettings({ retention: $event })" />
          </div>
          <div class="row">
            <div class="txt"><div class="name">{{ t('settings.openData') }}</div></div>
            <button class="btn" @click="openData"><Icon name="folder" :size="14" />{{ t('settings.openData') }}</button>
          </div>
        </section>

        <section class="card group about">
          <h2 class="label">{{ t('settings.about') }}</h2>
          <DotMatrix text="BLACKBOX" :dot="4" ghost />
          <p class="mono ver">{{ t('settings.version', { v: system?.version ?? '0.2.0' }) }}</p>
          <p class="tag">{{ t('settings.tagline') }}</p>
        </section>
      </div>
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
.cols {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  align-items: start;
}
@media (max-width: 1150px) {
  .cols {
    grid-template-columns: 1fr;
  }
}
.col {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}
.group {
  padding: 18px 22px 8px;
}
.group h2 {
  margin: 0 0 6px;
  font-weight: 400;
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  padding: 14px 0;
  border-bottom: 1px solid var(--line);
}
.row:last-child {
  border-bottom: 0;
}
.row.stack {
  flex-direction: column;
  align-items: flex-start;
  gap: 10px;
}
.txt {
  min-width: 0;
}
.name {
  font-size: 13.5px;
}
.desc,
.hint {
  font-size: 12px;
  color: var(--text-3);
  margin: 2px 0 0;
  max-width: 460px;
}
.busy {
  opacity: 0.6;
  pointer-events: none;
}
.about {
  padding-bottom: 20px;
}
.about .dm {
  margin: 14px 0 12px;
}
.ver {
  margin: 0;
  font-size: 11.5px;
  color: var(--text-2);
}
.tag {
  margin: 6px 0 0;
  font-size: 12.5px;
  color: var(--text-3);
}
</style>
