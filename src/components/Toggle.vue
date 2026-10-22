<script setup lang="ts">
defineProps<{ modelValue: boolean; label?: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()
</script>

<template>
  <button
    type="button"
    role="switch"
    class="toggle"
    :class="{ on: modelValue }"
    :aria-checked="modelValue"
    :aria-label="label"
    @click="emit('update:modelValue', !modelValue)"
  >
    <span class="knob" />
  </button>
</template>

<style scoped>
.toggle {
  position: relative;
  width: 44px;
  height: 24px;
  flex: none;
  border-radius: 99px;
  border: 1px solid var(--line-strong);
  background: var(--surface-3);
  transition: background 0.2s var(--ease), border-color 0.2s var(--ease);
}
.knob {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--text-2);
  transition: transform 0.2s var(--ease), background 0.2s var(--ease);
}
.toggle.on {
  background: var(--red);
  border-color: var(--red);
}
.toggle.on .knob {
  transform: translateX(20px);
  background: #fff;
}
</style>
