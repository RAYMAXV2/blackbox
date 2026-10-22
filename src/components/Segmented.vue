<script setup lang="ts" generic="T extends string | number">
defineProps<{ modelValue: T; options: { value: T; label: string }[]; label?: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: T] }>()
</script>

<template>
  <div class="seg" role="radiogroup" :aria-label="label">
    <button
      v-for="o in options"
      :key="String(o.value)"
      type="button"
      role="radio"
      :aria-checked="o.value === modelValue"
      :class="{ on: o.value === modelValue }"
      @click="emit('update:modelValue', o.value)"
    >
      {{ o.label }}
    </button>
  </div>
</template>

<style scoped>
.seg {
  display: inline-flex;
  padding: 3px;
  gap: 2px;
  border-radius: var(--pill);
  border: 1px solid var(--line-strong);
  background: var(--surface);
}
button {
  height: 28px;
  padding: 0 14px;
  border: 0;
  border-radius: var(--pill);
  background: transparent;
  color: var(--text-2);
  font-size: 12.5px;
  transition: background 0.15s var(--ease), color 0.15s var(--ease);
}
button:hover {
  color: var(--text);
}
button.on {
  background: var(--text);
  color: var(--bg);
}
</style>
