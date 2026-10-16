import { useSyncExternalStore } from 'react'

const TICK_MS = 30000

let now = Date.now()
const listeners = new Set<() => void>()
let timer: ReturnType<typeof setInterval> | null = null

function subscribe(fn: () => void) {
  listeners.add(fn)
  if (!timer) {
    // the clock may have been idle for a while; React re-renders if the snapshot moved
    now = Date.now()
    timer = setInterval(() => {
      now = Date.now()
      listeners.forEach((l) => l())
    }, TICK_MS)
  }
  return () => {
    listeners.delete(fn)
    if (!listeners.size && timer) {
      clearInterval(timer)
      timer = null
    }
  }
}

const getNow = () => now

/** Wall clock in ms for render-time age checks; one shared 30 s ticker instead of Date.now() per render. */
export function useNow(): number {
  return useSyncExternalStore(subscribe, getNow, getNow)
}
