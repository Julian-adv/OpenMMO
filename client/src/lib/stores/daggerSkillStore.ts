import { derived, get, writable } from 'svelte/store'

export const daggerSkillState = writable({
  queued: false,
  pending: false,
  cooldownUntil: 0,
  requestAt: 0,
})
export const daggerSkillCasts = writable(new Map<number, number>())

export const daggerSkillClock = derived(
  daggerSkillState,
  (state, set) => {
    set(Date.now())
    if (state.cooldownUntil <= Date.now()) return
    const timer = setInterval(() => {
      const now = Date.now()
      set(now)
      if (state.cooldownUntil <= now) clearInterval(timer)
    }, 100)
    return () => clearInterval(timer)
  },
  Date.now()
)

export function queueDaggerSkill(now = Date.now()) {
  const state = get(daggerSkillState)
  if (state.pending || state.cooldownUntil > now) return false
  daggerSkillState.set({ ...state, queued: !state.queued })
  return true
}

export function consumeDaggerSkill(now = Date.now()) {
  const state = get(daggerSkillState)
  if (!state.queued || state.pending || state.cooldownUntil > now) return false
  daggerSkillState.set({
    ...state,
    queued: false,
    pending: true,
    requestAt: now,
  })
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
  daggerSkillCasts.update((casts) => {
    const next = new Map(casts)
    next.delete(playerId)
    return next
  })
}

export function acknowledgeDaggerSkill(cooldownMs: number, now = Date.now()) {
  daggerSkillState.set({
    queued: false,
    pending: false,
    cooldownUntil: now + cooldownMs,
    requestAt: 0,
  })
}

export function resetDaggerSkill() {
  daggerSkillState.set({
    queued: false,
    pending: false,
    cooldownUntil: 0,
    requestAt: 0,
  })
  daggerSkillCasts.set(new Map())
}
