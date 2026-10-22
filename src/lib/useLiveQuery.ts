import { onMounted, shallowRef, watch } from 'vue'
import { status, visible } from '../store'

/**
 * Re-runs a command each time a status tick arrives (the engine pushes ticks
 * only while the UI is visible), at most once every `minMs`. No timers.
 */
export function useLiveQuery<T>(fetch: () => Promise<T>, initial: T, minMs = 2000) {
  const data = shallowRef<T>(initial)
  let last = 0
  let busy = false

  const run = async () => {
    if (busy || !visible.value) return
    busy = true
    last = Date.now()
    try {
      data.value = await fetch()
    } catch (e) {
      console.error(e)
    } finally {
      busy = false
    }
  }

  onMounted(run)
  watch(status, () => {
    if (Date.now() - last >= minMs) run()
  })

  return { data, refresh: run }
}
