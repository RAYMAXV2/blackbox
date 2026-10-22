<script setup lang="ts">
import DotMatrix from '../DotMatrix.vue'

defineProps<{ index: string; tone: 'kernel' | 'drivers' | 'hardware' | 'apps'; title: string; sub: string; last?: boolean }>()
</script>

<template>
  <section class="layer" :class="{ last }">
    <header class="side">
      <div class="idx">
        <DotMatrix :text="index" :dot="3" :color="`var(--l-${tone})`" />
      </div>
      <h3>{{ title }}</h3>
      <p>{{ sub }}</p>
    </header>
    <div class="content">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.layer {
  position: relative;
  display: grid;
  grid-template-columns: 180px minmax(0, 1fr);
  gap: 20px;
  padding-bottom: 22px;
}
.side {
  position: relative;
  padding-top: 4px;
}
.side::before {
  content: '';
  position: absolute;
  left: 14px;
  top: 34px;
  bottom: -18px;
  border-left: 1px dashed var(--line-strong);
}
.layer.last .side::before {
  display: none;
}
.idx {
  height: 24px;
  display: flex;
  align-items: center;
  position: relative;
  z-index: 1;
  background: transparent;
}
h3 {
  margin: 12px 0 2px 28px;
  font-size: 15px;
  font-weight: 600;
  letter-spacing: -0.01em;
}
p {
  margin: 0 0 0 28px;
  font-size: 12px;
  color: var(--text-3);
  line-height: 1.4;
}
.content {
  min-width: 0;
}
@media (max-width: 1100px) {
  .layer {
    grid-template-columns: 1fr;
    gap: 10px;
  }
  .side::before {
    display: none;
  }
  h3,
  p {
    margin-left: 0;
  }
  .idx {
    display: none;
  }
}
</style>
