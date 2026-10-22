<script setup lang="ts">
import { computed } from 'vue'
import ProcTable from '../ProcTable.vue'
import { status } from '../../store'

const procs = computed(() => status.value?.processes ?? [])
</script>

<template>
  <div class="card wrap">
    <ProcTable :rows="procs" compact />
  </div>
</template>

<style scoped>
.wrap {
  overflow: hidden;
}
</style>
