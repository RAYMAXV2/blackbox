<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    value: number // 0..100
    tone?: 'auto' | 'neutral' | 'kernel' | 'drivers' | 'hardware' | 'apps' | 'red'
    height?: number
    warnAt?: number
    badAt?: number
  }>(),
  { tone: 'neutral', height: 4, warnAt: 75, badAt: 92 },
)

const width = computed(() => `${Math.max(0, Math.min(100, props.value))}%`)
const color = computed(() => {
  if (props.tone === 'auto') {
    if (props.value >= props.badAt) return 'var(--red)'
    if (props.value >= props.warnAt) return 'var(--amber)'
    return 'var(--text)'
  }
  if (props.tone === 'neutral') return 'var(--text)'
  if (props.tone === 'red') return 'var(--red)'
  return `var(--l-${props.tone})`
})
</script>

<template>
  <div class="meter" :style="{ height: `${height}px` }">
    <div class="fill" :style="{ width, background: color }" />
  </div>
</template>

<style scoped>
.meter {
  position: relative;
  width: 100%;
  border-radius: 99px;
  background: var(--surface-3);
  overflow: hidden;
}
.fill {
  height: 100%;
  border-radius: 99px;
  transition: width 0.6s var(--ease);
}
</style>
