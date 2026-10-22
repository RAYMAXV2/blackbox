<script setup lang="ts">
// Hand-drawn 24×24 stroke icons. Kept tiny on purpose: no icon pack.
const PATHS: Record<string, string> = {
  live: 'M3 12h4l2-6 4 12 2-6h6',
  incidents: 'M12 4 2.8 19.5h18.4L12 4zM12 10v4.5M12 17.2v.3',
  drivers: 'M7 7h10v10H7zM10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4',
  processes: 'M4 6h16M4 12h16M4 18h10',
  system: 'M3 5h18v11H3zM8 20h8M12 16v4',
  causes: 'M5 4h10a4 4 0 0 1 4 4v12H9a4 4 0 0 1-4-4zM5 16a4 4 0 0 1 4-4h10',
  settings: 'M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4',
  bell: 'M6 16V11a6 6 0 1 1 12 0v5l1.5 2h-15zM10 20.5h4',
  minimize: 'M6 12h12',
  maximize: 'M6.5 6.5h11v11h-11z',
  close: 'M6.5 6.5l11 11M17.5 6.5l-11 11',
  check: 'M5 12.5l4.5 4.5L19 7.5',
  x: 'M7 7l10 10M17 7 7 17',
  arrow: 'M5 12h14M13 6l6 6-6 6',
  export: 'M12 4v11M7.5 10.5 12 15l4.5-4.5M5 19h14',
  trash: 'M5 7h14M10 7V4.5h4V7M7 7l1 13h8l1-13',
  search: 'M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.5 15.5 20 20',
  shield: 'M12 3l7 3v5.5c0 4.5-3 8-7 9.5-4-1.5-7-5-7-9.5V6z',
  pause: 'M9 5v14M15 5v14',
  play: 'M8 5l11 7-11 7z',
  folder: 'M3 6.5h6l2 2h10V19H3z',
  read: 'M4 12.5l4 4L18 6.5M12 16.5l1 1 7-8',
  game: 'M7 9v4M5 11h4M15 10.5h.01M18 12.5h.01M8 6h8a5 5 0 0 1 5 5v2.5a3.5 3.5 0 0 1-6.3 2.1L13.5 14h-3l-1.2 1.6A3.5 3.5 0 0 1 3 13.5V11a5 5 0 0 1 5-5z',
  sun: 'M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4',
  info: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5.5M12 7.8v.4',
}

withDefaults(defineProps<{ name: string; size?: number; stroke?: number }>(), { size: 18, stroke: 1.6 })
</script>

<template>
  <svg
    class="icon"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    :stroke-width="stroke"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path :d="PATHS[name] ?? PATHS.info" />
  </svg>
</template>

<style scoped>
.icon {
  flex: none;
  display: block;
}
</style>
