<script setup lang="ts">
import HealthCard from '../components/live/HealthCard.vue'
import GameCard from '../components/live/GameCard.vue'
import RecentIncidents from '../components/live/RecentIncidents.vue'
import Layer from '../components/live/Layer.vue'
import KernelLayer from '../components/live/KernelLayer.vue'
import DriversLayer from '../components/live/DriversLayer.vue'
import HardwareLayer from '../components/live/HardwareLayer.vue'
import AppsLayer from '../components/live/AppsLayer.vue'
import { t } from '../i18n'
</script>

<template>
  <div class="page">
    <section class="hero">
      <HealthCard />
      <GameCard />
    </section>

    <RecentIncidents />

    <div class="stack-head">
      <span class="label">{{ t('live.stack') }}</span>
      <span class="rule" />
    </div>

    <Layer index="01" tone="kernel" :title="t('layer.kernel')" :sub="t('layer.kernel.sub')">
      <KernelLayer />
    </Layer>
    <Layer index="02" tone="drivers" :title="t('layer.drivers')" :sub="t('layer.drivers.sub')">
      <DriversLayer />
    </Layer>
    <Layer index="03" tone="hardware" :title="t('layer.hardware')" :sub="t('layer.hardware.sub')">
      <HardwareLayer />
    </Layer>
    <Layer index="04" tone="apps" :title="t('layer.apps')" :sub="t('layer.apps.sub')" last>
      <AppsLayer />
    </Layer>
  </div>
</template>

<style scoped>
.hero {
  display: grid;
  grid-template-columns: minmax(280px, 0.85fr) minmax(0, 1.6fr);
  gap: 14px;
}
@media (max-width: 860px) {
  .hero {
    grid-template-columns: 1fr;
  }
}
.stack-head {
  display: flex;
  align-items: center;
  gap: 14px;
  margin-bottom: 18px;
}
.rule {
  flex: 1;
  border-top: 1px solid var(--line);
}
</style>
