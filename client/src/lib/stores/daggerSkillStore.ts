import { get, writable } from 'svelte/store'
import { deadlineClock } from './deadlineClock'

export const daggerSkillState = writable({
  queued: false,
  cooldownUntil: 0,
})
export const daggerSkillCasts = writable(new Map<number, number>())

export const daggerSkillClock = deadlineClock(
  daggerSkillState,
  (state) => state.cooldownUntil
)

export function queueDaggerSkill(now = Date.now()) {
  const state = get(daggerSkillState)
  if (state.cooldownUntil > now) return false
  daggerSkillState.set({ ...state, queued: !state.queued })
  return true
}

export function playDaggerSkill(playerId: number, now = Date.now()) {
  daggerSkillCasts.update((casts) => {
    const next = new Map([...casts].filter(([, at]) => now - at < 2000))
    next.set(playerId, now)
    return next
  })
}

export function clearDaggerCast(playerId: number) {
  if (!get(daggerSkillCasts).has(playerId)) return
  daggerSkillCasts.update((casts) => {
    const next = new Map(casts)
    next.delete(playerId)
    return next
  })
}

export function acknowledgeDaggerSkill(cooldownMs: number, now = Date.now()) {
  daggerSkillState.set({ queued: false, cooldownUntil: now + cooldownMs })
}

export function syncDaggerSkillCooldown(cooldownMs: number, now = Date.now()) {
  daggerSkillState.update((state) => ({
    ...state,
    queued: cooldownMs > 0 ? false : state.queued,
    cooldownUntil: now + Math.max(0, cooldownMs),
  }))
}

export function resetDaggerSkill() {
  daggerSkillState.set({ queued: false, cooldownUntil: 0 })
  daggerSkillCasts.set(new Map())
}
