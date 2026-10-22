<script setup lang="ts">
import { computed, defineAsyncComponent, type Component } from 'vue'
import TitleBar from './components/TitleBar.vue'
import NavRail from './components/NavRail.vue'
import ToastStack from './components/ToastStack.vue'
import DotMatrix from './components/DotMatrix.vue'
import LivePage from './pages/LivePage.vue'
import { bootError, page, ready, type PageId } from './store'

// Live is the landing page; the others load on first visit.
const PAGES: Record<PageId, Component> = {
  live: LivePage,
  incidents: defineAsyncComponent(() => import('./pages/IncidentsPage.vue')),
  drivers: defineAsyncComponent(() => import('./pages/DriversPage.vue')),
  processes: defineAsyncComponent(() => import('./pages/ProcessesPage.vue')),
  system: defineAsyncComponent(() => import('./pages/SystemPage.vue')),
  causes: defineAsyncComponent(() => import('./pages/CausesPage.vue')),
  settings: defineAsyncComponent(() => import('./pages/SettingsPage.vue')),
}

const current = computed(() => PAGES[page.value])
</script>

<template>
  <div class="shell">
    <TitleBar />
    <div class="body">
      <NavRail />
      <main class="main">
        <div v-if="!ready" class="boot">
          <DotMatrix text="BLACKBOX" :dot="6" ghost />
          <p v-if="bootError" class="mono err">{{ bootError }}</p>
        </div>
        <component :is="current" v-else />
      </main>
    </div>
    <ToastStack />
  </div>
</template>

<style scoped>
.shell {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-width: 880px;
}
.body {
  flex: 1;
  min-height: 0;
  display: flex;
}
.main {
  flex: 1;
  min-width: 0;
  position: relative;
  background-image: radial-gradient(var(--bg-dots) 1px, transparent 1.4px);
  background-size: 18px 18px;
  background-position: -9px -9px;
}
.boot {
  height: 100%;
  display: grid;
  place-content: center;
  gap: 16px;
  justify-items: center;
}
.err {
  color: var(--red);
  font-size: 12px;
}
</style>
