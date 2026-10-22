import { createApp } from 'vue'
import App from './App.vue'
import { boot } from './store'
import './styles/main.css'

createApp(App).mount('#app')
boot()

// The UI is an app, not a web page: no browser context menu outside inputs.
document.addEventListener('contextmenu', (e) => {
  const el = e.target as HTMLElement
  if (!el.closest('input, textarea')) e.preventDefault()
})
