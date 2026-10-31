<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import DotMatrix from './DotMatrix.vue'
import Led from './Led.vue'
import Icon from './Icon.vue'
import { causeTitle, incidents, markRead, notifications, now, openIncident, status, system, unread } from '../store'
import { isTauri } from '../lib/ipc'
import { t } from '../i18n'
import { fmtMagnitude, fmtNum, fmtPct, fmtRelative } from '../lib/format'

const mode = computed(() => status.value?.mode ?? 'full')
const overhead = computed(() => status.value?.overhead ?? null)
const version = computed(() => system.value?.version ?? '0.2.0')

const open = ref(false)
const panel = ref<HTMLElement>()
const bell = ref<HTMLElement>()

const items = computed(() => incidents.value.slice(0, 8))
const systemNotes = computed(() => notifications.value.filter((n) => n.incidentId === null).slice(0, 3))

function onDocDown(e: PointerEvent) {
  const target = e.target as Node
  if (panel.value?.contains(target) || bell.value?.contains(target)) return
  open.value = false
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') open.value = false
}
watch(open, (v) => {
  if (v) {
    document.addEventListener('pointerdown', onDocDown)
    document.addEventListener('keydown', onKey)
  } else {
    document.removeEventListener('pointerdown', onDocDown)
    document.removeEventListener('keydown', onKey)
  }
})
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocDown)
  document.removeEventListener('keydown', onKey)
})

function go(id: string) {
  open.value = false
  openIncident(id)
}

async function win(action: 'minimize' | 'toggleMaximize' | 'close') {
  if (!isTauri) return
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow()[action]()
}
</script>

<template>
  <header class="bar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <DotMatrix text="BLACKBOX" :dot="2.4" />
      <span class="ver mono" data-tauri-drag-region>v{{ version }}</span>
    </div>

    <div class="rec" :class="mode" data-tauri-drag-region>
      <Led :state="mode === 'full' ? 'rec' : mode === 'limited' ? 'warn' : 'idle'" :pulse="mode === 'full'" :size="7" />
      <span class="label" data-tauri-drag-region>{{ mode === 'full' ? t('rec') : t('mode.' + mode) }}</span>
    </div>

    <div class="spacer" data-tauri-drag-region />

    <div v-if="overhead" class="overhead mono" :title="t('overhead.tooltip')" data-tauri-drag-region>
      <span class="dim" data-tauri-drag-region>{{ t('overhead.label') }}</span>
      <span data-tauri-drag-region>{{ fmtPct(overhead.cpuPct, 2) }} {{ t('overhead.cpu') }}</span>
      <span class="sep" data-tauri-drag-region>·</span>
      <span data-tauri-drag-region>{{ fmtNum(overhead.workingSetMb, 1) }} {{ t('bytes.MB') }}</span>
      <span class="sep" data-tauri-drag-region>·</span>
      <span data-tauri-drag-region>{{ fmtNum(overhead.eventsPerSec) }} {{ t('overhead.events') }}</span>
      <span class="sep" data-tauri-drag-region>·</span>
      <span :class="{ lost: overhead.eventsLost > 0 }" data-tauri-drag-region>{{ fmtNum(overhead.eventsLost) }} {{ t('overhead.lost') }}</span>
    </div>

    <div class="bell-wrap">
      <button
        ref="bell"
        class="icon-btn"
        :class="{ on: open }"
        :title="t('title.notifications')"
        :aria-label="t('title.notifications')"
        :aria-expanded="open"
        @click="open = !open"
      >
        <Icon name="bell" :size="17" />
        <span v-if="unread" class="badge mono">{{ unread > 99 ? '99+' : unread }}</span>
      </button>

      <Transition name="drop">
        <div v-if="open" ref="panel" class="panel card" role="dialog" :aria-label="t('notif.title')">
          <div class="panel-head">
            <span class="label">{{ t('notif.title') }}</span>
            <button v-if="unread" class="btn small ghost" @click="markRead(null)">{{ t('notif.markAll') }}</button>
          </div>
          <div v-for="n in systemNotes" :key="n.id" class="note">
            <Led state="info" :size="6" />
            <span>{{ t(n.title.key, n.title.params) }}</span>
          </div>
          <button v-for="i in items" :key="i.id" class="item" @click="go(i.id)">
            <Led :state="i.severity === 'severe' ? 'bad' : i.severity === 'major' ? 'warn' : 'idle'" :size="7" />
            <span class="item-body">
              <span class="item-top">
                <span class="mono">{{ t('kind.' + i.kind) }} · {{ fmtMagnitude(i.magnitude) }}</span>
                <span class="dim mono">{{ fmtRelative(i.ts, now) }}</span>
              </span>
              <span class="item-title">{{ causeTitle(i.causes[0]) || t('incident.noCause') }}</span>
            </span>
            <span v-if="!i.read" class="dot" />
          </button>
          <div v-if="!items.length && !systemNotes.length" class="empty">{{ t('notif.empty') }}</div>
        </div>
      </Transition>
    </div>

    <div class="winctl">
      <button class="wc" :title="t('title.minimize')" :aria-label="t('title.minimize')" @click="win('minimize')">
        <Icon name="minimize" :size="15" />
      </button>
      <button class="wc" :title="t('title.maximize')" :aria-label="t('title.maximize')" @click="win('toggleMaximize')">
        <Icon name="maximize" :size="13" />
      </button>
      <button class="wc close" :title="t('title.close')" :aria-label="t('title.close')" @click="win('close')">
        <Icon name="close" :size="15" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.bar {
  height: var(--titlebar);
  display: flex;
  align-items: center;
  gap: 16px;
  padding-left: 18px;
  border-bottom: 1px solid var(--line);
  background: var(--bg);
  position: relative;
  z-index: 20;
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  color: var(--text);
}
.ver {
  font-size: 10px;
  color: var(--text-3);
  padding: 1px 6px;
  border: 1px solid var(--line-strong);
  border-radius: 99px;
}
.rec {
  display: flex;
  align-items: center;
  gap: 8px;
}
.rec.full .label {
  color: var(--red);
}
.spacer {
  flex: 1;
  align-self: stretch;
}
.overhead {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10.5px;
  color: var(--text-2);
  white-space: nowrap;
}
.overhead .sep {
  color: var(--text-3);
}
.overhead .lost {
  color: var(--red);
}
@media (max-width: 1080px) {
  .overhead .dim {
    display: none;
  }
}
@media (max-width: 960px) {
  .overhead {
    display: none;
  }
}
.bell-wrap {
  position: relative;
}
.icon-btn {
  position: relative;
  width: 32px;
  height: 32px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 50%;
  background: transparent;
  color: var(--text-2);
}
.icon-btn:hover,
.icon-btn.on {
  background: var(--surface-3);
  color: var(--text);
}
.badge {
  position: absolute;
  top: 2px;
  right: 0;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  border-radius: 99px;
  background: var(--red);
  color: #fff;
  font-size: 9.5px;
  line-height: 16px;
  text-align: center;
}
.panel {
  position: absolute;
  right: -8px;
  top: 40px;
  width: 360px;
  max-height: 460px;
  overflow-y: auto;
  padding: 8px;
  background: var(--surface-2);
  box-shadow: 0 18px 50px rgba(0, 0, 0, 0.45);
}
.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 8px 8px;
}
.note {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  font-size: 12.5px;
  color: var(--text-2);
}
.item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  width: 100%;
  padding: 10px;
  border: 0;
  border-radius: 12px;
  background: transparent;
  text-align: left;
}
.item:hover {
  background: var(--surface-3);
}
.item .led {
  margin-top: 6px;
}
.item-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.item-top {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-2);
}
.item-title {
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dot {
  width: 6px;
  height: 6px;
  margin-top: 7px;
  border-radius: 50%;
  background: var(--red);
  flex: none;
}
.winctl {
  display: flex;
  align-self: stretch;
}
.wc {
  width: 46px;
  display: grid;
  place-items: center;
  border: 0;
  background: transparent;
  color: var(--text-2);
}
.wc:hover {
  background: var(--surface-3);
  color: var(--text);
}
.wc.close:hover {
  background: var(--red);
  color: #fff;
}
.drop-enter-active,
.drop-leave-active {
  transition: opacity 0.14s var(--ease), transform 0.14s var(--ease);
}
.drop-enter-from,
.drop-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
