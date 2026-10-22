<script setup lang="ts">
withDefaults(
  defineProps<{
    state?: 'ok' | 'warn' | 'bad' | 'idle' | 'rec' | 'off' | 'info'
    pulse?: boolean
    size?: number
  }>(),
  { state: 'ok', pulse: false, size: 8 },
)
</script>

<template>
  <span class="led" :class="[`led-${state}`, { pulse }]" :style="{ width: `${size}px`, height: `${size}px` }" aria-hidden="true" />
</template>

<style scoped>
.led {
  display: inline-block;
  flex: none;
  border-radius: 50%;
  background: var(--text-3);
}
.led-ok {
  background: var(--green);
  box-shadow: 0 0 0 3px var(--green-soft);
}
.led-warn {
  background: var(--amber);
  box-shadow: 0 0 0 3px var(--amber-soft);
}
.led-bad,
.led-rec {
  background: var(--red);
  box-shadow: 0 0 0 3px var(--red-soft);
}
.led-info {
  background: var(--text-2);
}
.led-idle,
.led-off {
  background: var(--text-3);
}
.pulse {
  animation: pulse 2.4s ease-in-out infinite;
}
@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.35;
  }
}
</style>
