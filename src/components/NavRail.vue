<script setup lang="ts">
import Icon from './Icon.vue'
import { navigate, page, settings, unread, updateSettings, type PageId } from '../store'
import { t } from '../i18n'

const ITEMS: { id: PageId; icon: string }[] = [
  { id: 'live', icon: 'live' },
  { id: 'incidents', icon: 'incidents' },
  { id: 'drivers', icon: 'drivers' },
  { id: 'processes', icon: 'processes' },
  { id: 'system', icon: 'system' },
  { id: 'causes', icon: 'causes' },
]

function toggleLang() {
  updateSettings({ lang: settings.value.lang === 'en' ? 'fr' : 'en' })
}
</script>

<template>
  <nav class="rail" aria-label="Main">
    <button
      v-for="item in ITEMS"
      :key="item.id"
      class="nav"
      :class="{ on: page === item.id }"
      :aria-current="page === item.id ? 'page' : undefined"
      @click="navigate(item.id)"
    >
      <span class="ico">
        <Icon :name="item.icon" :size="19" />
        <span v-if="item.id === 'incidents' && unread" class="pip" />
      </span>
      <span class="txt">{{ t('nav.' + item.id) }}</span>
    </button>
    <div class="grow" />
    <button class="nav lang" :title="t('settings.language')" @click="toggleLang">
      <span class="ico mono">{{ settings.lang.toUpperCase() }}</span>
    </button>
    <button
      class="nav"
      :class="{ on: page === 'settings' }"
      :aria-current="page === 'settings' ? 'page' : undefined"
      @click="navigate('settings')"
    >
      <span class="ico"><Icon name="settings" :size="19" /></span>
      <span class="txt">{{ t('nav.settings') }}</span>
    </button>
  </nav>
</template>

<style scoped>
.rail {
  width: var(--rail);
  flex: none;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 14px 0 12px;
  border-right: 1px solid var(--line);
  background: var(--bg);
}
.nav {
  width: 64px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 8px 0 7px;
  border: 0;
  border-radius: 14px;
  background: transparent;
  color: var(--text-3);
  transition: color 0.15s var(--ease), background 0.15s var(--ease);
}
.nav:hover {
  color: var(--text);
}
.ico {
  position: relative;
  width: 40px;
  height: 30px;
  display: grid;
  place-items: center;
  border-radius: 99px;
  transition: background 0.15s var(--ease);
}
.nav.on {
  color: var(--text);
}
.nav.on .ico {
  background: var(--surface-3);
}
.txt {
  font-family: var(--font-mono);
  font-size: 9px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  max-width: 64px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pip {
  position: absolute;
  top: 4px;
  right: 8px;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--red);
  box-shadow: 0 0 0 2px var(--bg);
}
.grow {
  flex: 1;
}
.lang .ico {
  font-size: 11px;
  letter-spacing: 0.08em;
  border: 1px solid var(--line-strong);
  height: 26px;
  width: 38px;
}
</style>
