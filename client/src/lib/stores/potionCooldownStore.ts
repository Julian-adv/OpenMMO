import { derived, get, writable } from 'svelte/store'
import { HEALING_POTION_COOLDOWN_MS } from '../data/combatTiming'
import { deadlineClock } from './deadlineClock'

// The server times the gap on arrival, so a send exactly one cooldown after
// the last can land early when latency drops.
const SEND_MARGIN_MS = 100

const cooldownUntil = writable(0)
const clock = deadlineClock(cooldownUntil, (until) => until)

export const potionCooldownRemaining = derived(
  [cooldownUntil, clock],
  ([until, now]) => Math.max(0, until - now)
)

/** Claims the cooldown for a drink; false while the last one still runs. */
export function claimPotionCooldown(now = Date.now()) {
  if (get(cooldownUntil) + SEND_MARGIN_MS > now) return false
  cooldownUntil.set(now + HEALING_POTION_COOLDOWN_MS)
  return true
}

export function resetPotionCooldown() {
  cooldownUntil.set(0)
}
