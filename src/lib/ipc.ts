import type { Commands } from './contract'

/** True inside the Tauri WebView; false in a plain browser (mock mode). */
export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export interface Backend {
  invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>
  listen(event: string, handler: (payload: unknown) => void): Promise<() => void>
}

let backend: Promise<Backend> | null = null

function load(): Promise<Backend> {
  if (!backend) {
    backend = isTauri
      ? Promise.all([import('@tauri-apps/api/core'), import('@tauri-apps/api/event')]).then(([core, ev]) => ({
          invoke: (cmd, args) => core.invoke(cmd, args),
          listen: (event, handler) => ev.listen(event, (e) => handler(e.payload)),
        }))
      : import('./mock').then((m) => m.backend)
  }
  return backend
}

/** Typed command call. Argument objects follow the contract (`{ id }`, `{ settings }`…). */
export async function call<K extends keyof Commands>(
  cmd: K,
  ...args: Parameters<Commands[K]>
): Promise<ReturnType<Commands[K]>> {
  const b = await load()
  return (await b.invoke(cmd, args[0] as Record<string, unknown> | undefined)) as ReturnType<Commands[K]>
}

/** Subscribe to an engine event. Resolves to the unsubscribe function. */
export async function on<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  const b = await load()
  return b.listen(event, handler as (p: unknown) => void)
}
