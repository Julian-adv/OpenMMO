import { derived, get, writable } from 'svelte/store'
import type { AbilityId, AbilityTimer } from '../data/abilities'
import type { Position } from '../network/networkTypes'

type Timers = Partial<Record<AbilityId, number>>
export const abilityCooldowns = writable<Timers>({})
export const activeBuffs = writable<Timers>({})
export const abilityPending = writable<Timers>({})

export const abilityClock = derived(
  [abilityCooldowns, activeBuffs, abilityPending],
  (states, set) => {
    set(Date.now())
    if (
      !states.some((state) =>
        Object.values(state).some((until) => until > Date.now())
      )
    )
      return
    const timer = setInterval(() => {
      const now = Date.now()
      set(now)
      if (
        !states.some((state) =>
          Object.values(state).some((until) => until > now)
        )
      )
        clearInterval(timer)
    }, 100)
    return () => clearInterval(timer)
  },
  Date.now()
)

export function timerSnapshot(
  timers: AbilityTimer[],
  now = Date.now()
): Timers {
  return Object.fromEntries(
    timers.map((timer) => [
      timer.ability,
      now + Math.max(0, timer.remaining_ms),
    ])
  )
}

export function beginAbility(id: AbilityId, now = Date.now()) {
  if (
    (get(abilityCooldowns)[id] ?? 0) > now ||
    (get(abilityPending)[id] ?? 0) > now
  )
    return false
  abilityPending.update((state) => ({ ...state, [id]: now + 3000 }))
  return true
}

export type AbilityEffectEvent = {
  ability: AbilityId
  player_id: number
  position: Position
  floor_level: number
  targets: number[]
  startedAt: number
}

let effects: AbilityEffectEvent[] = []
export function queueAbilityEffect(
  event: Omit<AbilityEffectEvent, 'startedAt'>
) {
  effects.push({ ...event, startedAt: Date.now() })
}

export function takeAbilityEffects() {
  const next = effects
  effects = []
  return next
}

export function resetAbilities() {
  abilityCooldowns.set({})
  activeBuffs.set({})
  abilityPending.set({})
  effects = []
}
