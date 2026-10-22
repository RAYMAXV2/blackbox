import { onBeforeUnmount, onMounted, watch, type Ref, type WatchSource } from 'vue'
import { themeTick, visible } from '../store'

/**
 * Redraws a canvas only when needed: on resize, on theme change, when one of
 * `sources` changes, and never while the window is hidden. Draws are
 * coalesced into a single animation frame.
 */
export function useCanvas(host: Ref<HTMLElement | undefined>, draw: (width: number, height: number) => void, sources: WatchSource[] = []) {
  let frame = 0
  let dirty = false
  let ro: ResizeObserver | null = null

  const schedule = () => {
    if (!visible.value) {
      dirty = true
      return
    }
    if (frame) return
    frame = requestAnimationFrame(() => {
      frame = 0
      dirty = false
      const el = host.value
      if (!el) return
      const { width, height } = el.getBoundingClientRect()
      if (width > 0 && height > 0) draw(width, height)
    })
  }

  onMounted(() => {
    ro = new ResizeObserver(schedule)
    if (host.value) ro.observe(host.value)
    schedule()
  })
  onBeforeUnmount(() => {
    ro?.disconnect()
    if (frame) cancelAnimationFrame(frame)
  })
  watch([themeTick, ...sources], schedule)
  watch(visible, (v) => {
    if (v && dirty) schedule()
  })

  return { redraw: schedule }
}
