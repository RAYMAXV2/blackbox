<script setup lang="ts">
import Led from './Led.vue'
import Icon from './Icon.vue'
import { causeTitle, dismissToast, openIncident, toasts } from '../store'
import type { Toast } from '../lib/contract'
import { t } from '../i18n'
import { fmtTime } from '../lib/format'

function title(n: Toast): string {
  const p = n.title.params
  if (n.title.key === 'toast.incident' && p && !p.app) return t('toast.incident.noapp', p)
  return t(n.title.key, p)
}

function click(n: Toast) {
  if (n.incidentId) openIncident(n.incidentId)
  dismissToast(n.id)
}
</script>

<template>
  <div class="stack" aria-live="polite">
    <TransitionGroup name="toast">
      <div
        v-for="n in toasts"
        :key="n.id"
        class="toast"
        :class="n.severity"
        role="button"
        tabindex="0"
        @click="click(n)"
        @keydown.enter="click(n)"
      >
        <Led :state="n.incidentId ? (n.severity === 'severe' ? 'bad' : n.severity === 'major' ? 'warn' : 'idle') : 'info'" :size="8" />
        <div class="body">
          <div class="top">
            <span class="ttl">{{ title(n) }}</span>
            <span class="time mono">{{ fmtTime(n.ts) }}</span>
          </div>
          <div v-if="n.cause" class="cause">{{ causeTitle(n.cause) }}</div>
        </div>
        <button class="x" :aria-label="t('common.close')" @click.stop="dismissToast(n.id)">
          <Icon name="x" :size="14" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.stack {
  position: fixed;
  right: 20px;
  bottom: 20px;
  width: 360px;
  display: flex;
  flex-direction: column-reverse;
  gap: 10px;
  z-index: 50;
  pointer-events: none;
}
.toast {
  pointer-events: auto;
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px 14px 14px 16px;
  border-radius: 16px;
  background: var(--surface-2);
  border: 1px solid var(--line-strong);
  box-shadow: 0 16px 40px rgba(0, 0, 0, 0.4);
  cursor: pointer;
}
.toast.severe {
  border-color: var(--red);
}
.toast .led {
  margin-top: 6px;
}
.body {
  flex: 1;
  min-width: 0;
}
.top {
  display: flex;
  justify-content: space-between;
  gap: 8px;
}
.ttl {
  font-family: var(--font-mono);
  font-size: 11px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.time {
  flex: none;
  font-size: 10.5px;
  color: var(--text-3);
  white-space: nowrap;
}
.cause {
  margin-top: 4px;
  font-size: 13.5px;
  line-height: 1.4;
}
.x {
  border: 0;
  background: transparent;
  color: var(--text-3);
  padding: 2px;
  border-radius: 6px;
}
.x:hover {
  color: var(--text);
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.22s var(--ease), transform 0.22s var(--ease);
}
.toast-enter-from {
  opacity: 0;
  transform: translateY(10px);
}
.toast-leave-to {
  opacity: 0;
  transform: translateX(16px);
}
</style>
