<script setup lang="ts">
// Where the time went: gameplay / menus / loading / away, as one thin stacked bar.
import { computed } from 'vue'
import type { PhaseTotals } from '../../lib/contract'
import { t } from '../../i18n'
import { fmtPct, fmtPlaytime } from '../../lib/format'
import { phaseTotal } from '../../lib/games'

const props = defineProps<{ phases: PhaseTotals | null | undefined }>()

const parts = computed(() => {
  const p = props.phases
  const total = phaseTotal(p)
  if (!p || total <= 0) return []
  return (
    [
      ['gameplay', p.gameplaySec],
      ['menu', p.menuSec],
      ['loading', p.loadingSec],
      ['away', p.awaySec],
    ] as const
  ).map(([id, sec]) => ({ id, sec, pct: (sec / total) * 100 }))
})
</script>

<template>
  <div v-if="parts.length" class="phases">
    <div class="head">
      <span class="label">{{ t('games.time') }}</span>
    </div>
    <div class="bar" role="img" :aria-label="t('games.time')">
      <span
        v-for="p in parts"
        v-show="p.pct > 0"
        :key="p.id"
        class="seg"
        :class="p.id"
        :style="{ flexGrow: p.sec }"
        :title="`${t('games.phase.' + p.id)} · ${fmtPct(p.pct)}`"
      />
    </div>
    <ul class="legend">
      <li v-for="p in parts" :key="p.id" :title="t('games.phase.' + p.id + '.hint')">
        <i class="sw" :class="p.id" />
        <span class="name">{{ t('games.phase.' + p.id) }}</span>
        <span class="num">{{ fmtPlaytime(p.sec) }}</span>
        <span class="num dim">{{ fmtPct(p.pct) }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.phases {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.bar {
  display: flex;
  height: 6px;
  border-radius: var(--pill);
  overflow: hidden;
  background: var(--surface-3);
  gap: 2px;
}
.seg {
  flex-basis: 0;
  min-width: 2px;
}
.gameplay {
  background: var(--text);
}
.menu {
  background: var(--text-3);
}
.loading {
  background: var(--amber);
}
.away {
  background: var(--line-strong);
}
.legend {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-wrap: wrap;
  gap: 6px 18px;
  font-size: 12px;
}
.legend li {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
}
.sw {
  width: 8px;
  height: 8px;
  border-radius: 2px;
}
.name {
  color: var(--text-2);
}
.legend .num {
  font-size: 11.5px;
}
</style>
