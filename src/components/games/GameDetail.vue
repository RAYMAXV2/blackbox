<script setup lang="ts">
import { computed } from 'vue'
import DotMatrix from '../DotMatrix.vue'
import Icon from '../Icon.vue'
import Led from '../Led.vue'
import SessionStatsGrid from './SessionStatsGrid.vue'
import SessionTrend from './SessionTrend.vue'
import SessionsTable from './SessionsTable.vue'
import type { GameRecord } from '../../lib/contract'
import { t } from '../../i18n'
import { fmtNum, fmtPlaytime, fmtRelative } from '../../lib/format'
import { prettyGameName } from '../../lib/games'
import { now } from '../../store'

const props = defineProps<{ game: GameRecord }>()
const emit = defineEmits<{ forget: [name: string] }>()

const name = computed(() => prettyGameName(props.game.name))
const avg = computed(() => props.game.last3)
const avgTitle = computed(() => {
  const n = avg.value?.sessions ?? 0
  if (n >= 3) return t('games.avg3')
  if (n === 1) return t('games.avg1')
  return t('games.avgN', { count: n })
})

function forget() {
  if (window.confirm(t('games.forgetConfirm', { game: name.value, count: props.game.totalSessions }))) emit('forget', props.game.name)
}
</script>

<template>
  <article class="pad detail">
    <header class="top">
      <div>
        <div class="title-row">
          <h2>{{ name }}</h2>
          <span v-if="game.live" class="chip red"><Led state="rec" pulse :size="6" />{{ t('games.live') }}</span>
        </div>
        <div class="meta mono">
          <span>{{ game.name }}</span>
          <span>·</span>
          <span>{{ t(game.totalSessions === 1 ? 'games.sessions.one' : 'games.sessions', { count: game.totalSessions }) }}</span>
          <span>·</span>
          <span>{{ t('games.playtime', { time: fmtPlaytime(game.totalPlaySec) }) }}</span>
          <span>·</span>
          <span>{{ t('games.lastPlayed', { when: fmtRelative(game.lastPlayedTs, now) }) }}</span>
        </div>
      </div>
      <button class="btn small danger" @click="forget"><Icon name="trash" :size="14" />{{ t('games.forget') }}</button>
    </header>

    <section v-if="game.live" class="card block live">
      <div class="block-head">
        <span class="label"><Led state="rec" pulse :size="6" /> {{ t('games.liveSession') }}</span>
        <span class="mono dim">{{ fmtPlaytime(game.live.durationSec) }}</span>
      </div>
      <SessionStatsGrid :stats="game.live" :game="game.name" compact />
    </section>

    <section v-if="avg" class="card block">
      <div class="block-head avg-head">
        <div>
          <span class="label">{{ avgTitle }}</span>
          <div class="hero">
            <DotMatrix :text="Math.round(avg.fpsAvg)" :dot="7" ghost />
            <div class="hero-side">
              <span class="label">FPS</span>
              <span class="num low">{{ t('games.m.low1') }} <b>{{ fmtNum(avg.low1Fps) }}</b></span>
            </div>
          </div>
        </div>
        <div v-if="game.sessions.length > 1" class="trend">
          <SessionTrend :sessions="game.sessions" />
        </div>
      </div>
      <SessionStatsGrid :stats="avg" :game="game.name" />
    </section>

    <section v-if="game.sessions.length" class="card block">
      <div class="block-head">
        <span class="label">{{ t('games.table') }}</span>
        <span class="mono dim">{{ t('games.table.hint') }}</span>
      </div>
      <SessionsTable :sessions="game.sessions" :reference="avg" :game="game.name" />
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
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}
.title-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
h2 {
  margin: 0;
  font-size: 26px;
  font-weight: 600;
  letter-spacing: -0.02em;
}
.meta {
  margin-top: 6px;
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  font-size: 11.5px;
  color: var(--text-3);
}
.block {
  padding: 18px 20px 20px;
}
.block-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 14px;
}
.block-head .label {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}
.live {
  border-color: var(--line-strong);
}
.avg-head {
  align-items: flex-end;
  gap: 32px;
  margin-bottom: 22px;
}
.hero {
  display: flex;
  align-items: flex-end;
  gap: 14px;
  margin-top: 12px;
}
.hero-side {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-bottom: 2px;
}
.low {
  font-size: 12.5px;
  color: var(--text-2);
  white-space: nowrap;
}
.low b {
  color: var(--text);
  font-weight: 500;
}
.trend {
  flex: 1;
  max-width: 560px;
  min-width: 240px;
}
</style>
