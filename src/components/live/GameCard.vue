<script setup lang="ts">
import { computed } from 'vue'
import DotMatrix from '../DotMatrix.vue'
import FrameGraph from '../FrameGraph.vue'
import Icon from '../Icon.vue'
import { status } from '../../store'
import { t } from '../../i18n'
import { fmtDuration, fmtMs, fmtNum } from '../../lib/format'

const game = computed(() => status.value?.game ?? null)
</script>

<template>
  <article class="card game">
    <template v-if="game">
      <div class="head">
        <div>
          <span class="label">{{ t('live.game.title') }}</span>
          <div class="name mono">{{ game.name }}</div>
        </div>
        <div class="session">
          <span class="label">{{ t('live.session') }}</span>
          <span class="num">{{ fmtDuration(game.sessionSec) }}</span>
        </div>
      </div>

      <div class="main">
        <div class="fps">
          <DotMatrix :text="Math.round(game.fps)" :dot="8" ghost />
          <span class="label">{{ t('live.fps') }}</span>
        </div>
        <div class="graph">
          <FrameGraph :frames="game.frames" />
        </div>
      </div>

      <div class="stats">
        <div>
          <span class="label">{{ t('live.frameTime') }}</span>
          <b class="num">{{ fmtMs(game.frameTimeMs, 2) }}</b>
        </div>
        <div>
          <span class="label">{{ t('live.p99') }}</span>
          <b class="num">{{ fmtMs(game.p99Ms, 2) }}</b>
        </div>
        <div>
          <span class="label">{{ t('live.low1') }}</span>
          <b class="num">{{ fmtNum(game.low1Fps) }} <small>FPS</small></b>
        </div>
        <div>
          <span class="label">{{ t('live.hitches') }}</span>
          <b class="num" :class="{ red: game.hitches > 0 }">{{ fmtNum(game.hitches) }}</b>
        </div>
      </div>
    </template>

    <div v-else class="none">
      <Icon name="game" :size="28" :stroke="1.3" />
      <h3>{{ t('live.game.none') }}</h3>
      <p>{{ t('live.game.noneHint') }}</p>
    </div>
  </article>
</template>

<style scoped>
.game {
  padding: 22px 24px;
  display: flex;
  flex-direction: column;
  gap: 18px;
  min-height: 300px;
}
.head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
}
.name {
  margin-top: 4px;
  font-size: 15px;
}
.session {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 4px;
  font-size: 13px;
}
.main {
  flex: 1;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 24px;
  align-items: stretch;
}
.fps {
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  gap: 10px;
  min-width: 150px;
}
.graph {
  min-height: 120px;
  border-left: 1px solid var(--line);
  padding-left: 16px;
}
.stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 12px;
  padding-top: 16px;
  border-top: 1px solid var(--line);
}
.stats > div {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.stats b {
  font-size: 17px;
  font-weight: 500;
}
.stats small {
  font-size: 10px;
  color: var(--text-3);
}
.red {
  color: var(--red);
}
.none {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  gap: 6px;
  color: var(--text-3);
  border: 1px dashed var(--line-strong);
  border-radius: 14px;
  padding: 24px;
}
.none h3 {
  margin: 8px 0 0;
  font-size: 15px;
  color: var(--text);
  font-weight: 600;
}
.none p {
  margin: 0;
  max-width: 340px;
  font-size: 12.5px;
}
</style>
