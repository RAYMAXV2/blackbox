<script setup lang="ts">
// Latency histogram (<50, <100, <250, <500, <1000, <2000, >=2000 µs) on a log scale.
import { computed } from 'vue'

const props = defineProps<{ values: number[] }>()
const LABELS = ['<50', '<100', '<250', '<500', '<1k', '<2k', '2k+']

const bars = computed(() => {
  const logs = props.values.map((v) => (v > 0 ? Math.log10(v + 1) : 0))
  const max = Math.max(1, ...logs)
  return logs.map((l, i) => ({ h: Math.max(l > 0 ? 2 : 0, (l / max) * 18), i, n: props.values[i] }))
})
</script>

<template>
  <svg class="hist" width="70" height="20" viewBox="0 0 70 20" role="img">
    <title>{{ bars.map((b) => `${LABELS[b.i]} µs: ${b.n}`).join('\n') }}</title>
    <rect
      v-for="b in bars"
      :key="b.i"
      :x="b.i * 10"
      :y="20 - b.h"
      width="7"
      :height="b.h"
      rx="1.5"
      :class="b.i >= 5 ? 'bad' : b.i >= 4 ? 'warn' : 'ok'"
    />
  </svg>
</template>

<style scoped>
.hist {
  display: block;
}
.ok {
  fill: var(--text-3);
}
.warn {
  fill: var(--amber);
}
.bad {
  fill: var(--red);
}
</style>
