<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import DotMatrix from '../components/DotMatrix.vue'
import Icon from '../components/Icon.vue'
import Led from '../components/Led.vue'
import GameDetail from '../components/games/GameDetail.vue'
import { call, on } from '../lib/ipc'
import { EVENTS, type GameRecord } from '../lib/contract'
import { t } from '../i18n'
import { fmtNum, fmtPlaytime, fmtRelative } from '../lib/format'
import { prettyGameName } from '../lib/games'
import { now, status, visible } from '../store'

/** While a game is live, its record is refreshed this often (driven by status ticks, no timer). */
const LIVE_REFRESH_MS = 5000

const games = shallowRef<GameRecord[]>([])
const loaded = ref(false)
const selected = ref<string | null>(null)
let lastLoad = 0
let busy = false

const current = computed(() => games.value.find((g) => g.name === selected.value) ?? null)

async function load() {
  if (busy) return
  busy = true
  lastLoad = Date.now()
  try {
    games.value = await call('get_games')
  } catch (e) {
    console.error('get_games failed', e)
  } finally {
    busy = false
    loaded.value = true
  }
  if (!games.value.some((g) => g.name === selected.value)) selected.value = games.value[0]?.name ?? null
}

async function forget(name: string) {
  games.value = games.value.filter((g) => g.name !== name)
  if (selected.value === name) selected.value = games.value[0]?.name ?? null
  await call('delete_game', { name })
}

let off: (() => void) | null = null
onMounted(async () => {
  load()
  off = await on<GameRecord>(EVENTS.session, () => load())
})
onBeforeUnmount(() => off?.())

// Status ticks only arrive while the UI is visible: they pace the live refresh.
watch(status, () => {
  if (visible.value && games.value.some((g) => g.live) && Date.now() - lastLoad >= LIVE_REFRESH_MS) load()
})
</script>

<template>
  <div class="games">
    <aside class="list">
      <div class="list-head">
        <h2>{{ t('games.title') }}</h2>
        <p>{{ t('games.subtitle') }}</p>
      </div>
      <div class="items">
        <button
          v-for="g in games"
          :key="g.name"
          class="item"
          :class="{ on: selected === g.name }"
          @click="selected = g.name"
        >
          <div class="row1">
            <span class="name">{{ prettyGameName(g.name) }}</span>
            <span v-if="g.live" class="live mono"><Led state="rec" pulse :size="6" />{{ t('games.live') }}</span>
          </div>
          <div class="row2">
            <div class="fps">
              <!-- First session of a game: show the live numbers until one is saved. -->
              <DotMatrix v-if="g.last3 ?? g.live" :text="Math.round((g.last3 ?? g.live)!.fpsAvg)" :dot="4" />
              <span class="label">FPS</span>
            </div>
            <div v-if="g.last3 ?? g.live" class="low num">
              <span class="label">{{ t('games.m.low1') }}</span>
              <b>{{ fmtNum((g.last3 ?? g.live)!.low1Fps) }}</b>
            </div>
          </div>
          <div class="row3 mono">
            <span>{{ t(g.totalSessions === 1 ? 'games.sessions.one' : 'games.sessions', { count: g.totalSessions }) }}</span>
            <span>{{ fmtPlaytime(g.totalPlaySec) }}</span>
            <span class="when">{{ fmtRelative(g.lastPlayedTs, now) }}</span>
          </div>
        </button>
      </div>
    </aside>

    <section class="detail">
      <GameDetail v-if="current" :key="current.name" :game="current" @forget="forget" />
      <div v-else-if="loaded && !games.length" class="empty-state">
        <Icon name="game" :size="34" :stroke="1.2" />
        <h3>{{ t('games.empty.title') }}</h3>
        <p>{{ t('games.empty.body') }}</p>
      </div>
      <div v-else class="placeholder">{{ loaded ? t('games.select') : '' }}</div>
    </section>
  </div>
</template>

<style scoped>
.games {
  height: 100%;
  display: grid;
  grid-template-columns: 330px minmax(0, 1fr);
}
@media (max-width: 1100px) {
  .games {
    grid-template-columns: 280px minmax(0, 1fr);
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
  padding: 22px 18px 14px 22px;
}
h2 {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.02em;
}
.list-head p {
  margin: 4px 0 0;
  font-size: 12.5px;
  color: var(--text-3);
}
.items {
  flex: 1;
  overflow-y: auto;
  padding: 4px 10px 20px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.item {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px 14px 12px;
  border: 1px solid transparent;
  border-radius: 14px;
  background: transparent;
  text-align: left;
}
.item:hover {
  background: var(--surface-2);
}
.item.on {
  background: var(--surface);
  border-color: var(--line-strong);
}
.row1 {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.name {
  font-size: 14px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.live {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--red);
}
.row2 {
  display: flex;
  align-items: flex-end;
  gap: 18px;
}
.fps {
  display: flex;
  align-items: flex-end;
  gap: 8px;
}
.fps .label {
  line-height: 1;
}
.low {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.low b {
  font-size: 14px;
  font-weight: 500;
}
.row3 {
  display: flex;
  gap: 10px;
  font-size: 10.5px;
  color: var(--text-3);
}
.row3 .when {
  margin-left: auto;
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
.empty-state {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  gap: 8px;
  padding: 32px;
  color: var(--text-3);
}
.empty-state h3 {
  margin: 10px 0 0;
  font-size: 17px;
  font-weight: 600;
  color: var(--text);
}
.empty-state p {
  margin: 0;
  max-width: 420px;
  font-size: 13px;
  line-height: 1.6;
}
</style>
