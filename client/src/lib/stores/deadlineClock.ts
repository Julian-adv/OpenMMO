import { derived, type Readable } from 'svelte/store'

/** `Date.now()` that ticks every 100 ms only until the store's deadline. */
export function deadlineClock<T>(
  store: Readable<T>,
  deadline: (value: T) => number
): Readable<number> {
  return derived(
    store,
    (value, set) => {
      const until = deadline(value)
      set(Date.now())
      if (until <= Date.now()) return
      const timer = setInterval(() => {
        const now = Date.now()
        set(now)
        if (until <= now) clearInterval(timer)
      }, 100)
      return () => clearInterval(timer)
    },
    Date.now()
  )
}
