import { SvelteMap } from 'svelte/reactivity'
import { hmrSingleton } from '../utils/hmr'
import { clearNameHover } from '../stores/gameStore'
import type { ServerStall } from '../network/networkTypes'

/** Laid-out stalls by id. Surface-only, like campfires. */
class StallManager {
  stalls = new SvelteMap<number, ServerStall>()

  spawn(stall: ServerStall) {
    this.stalls.set(stall.id, { ...stall })
  }

  setSign(id: number, sign: string) {
    const stall = this.stalls.get(id)
    if (stall) this.stalls.set(id, { ...stall, sign })
  }

  remove(id: number) {
    clearNameHover()
    this.stalls.delete(id)
  }

  reset() {
    this.stalls.clear()
  }
}

export const stallManager = hmrSingleton(
  'stallManager',
  () => new StallManager()
)
