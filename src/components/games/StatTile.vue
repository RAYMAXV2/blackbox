<script setup lang="ts">
import type { Tone } from '../../lib/games'

withDefaults(defineProps<{ label: string; value: string; unit?: string; sub?: string; tone?: Tone }>(), {
  unit: '',
  sub: '',
  tone: '',
})
</script>

<template>
  <div class="tile">
    <span class="label">{{ label }}</span>
    <span class="v num" :class="tone">
      {{ value }}<small v-if="unit">{{ unit }}</small>
    </span>
    <span v-if="sub" class="sub mono">{{ sub }}</span>
  </div>
</template>

<style scoped>
.tile {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  padding: 12px 14px;
  border-radius: 12px;
  background: var(--surface-2);
  border: 1px solid var(--line);
}
.v {
  font-size: 18px;
  font-weight: 500;
  line-height: 1.2;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.v small {
  margin-left: 4px;
  font-size: 10.5px;
  color: var(--text-3);
  font-weight: 400;
}
.v.better {
  color: var(--green);
}
.v.worse {
  color: var(--red);
}
.sub {
  font-size: 10.5px;
  color: var(--text-3);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
