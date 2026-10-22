import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// Tauri expects a fixed dev port and a static build in dist/.
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'] },
  },
  build: {
    outDir: 'dist',
    target: ['es2022', 'chrome120'],
    cssTarget: 'chrome120',
    reportCompressedSize: true,
    chunkSizeWarningLimit: 300,
  },
})
