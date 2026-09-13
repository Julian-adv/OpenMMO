import { derived, get, writable } from 'svelte/store'
import {
  BUFF_ABILITIES,
  DOUBLE_SLASH,
  type AbilityId,
  type AbilityTimer,
} from '../data/abilities'
import type { Position } from '../network/networkTypes'
import { syncDaggerSkillCooldown } from './daggerSkillStore'

type Timers = Partial<Record<AbilityTimer['ability'], number>>
export const abilityCooldowns = writable<Timers>({})
export const activeBuffs = writable<Timers>({})
export const abilityPending = writable<Timers>({})
export const bowMark = writable<{
  monsterId: string
  startedAt: number
  until: number
} | null>(null)

export function updateBowMark(
  monsterId: string | null,
  remainingMs: number,
  now = Date.now()
) {
  if (monsterId && remainingMs > 0) {
    bowMark.update((mark) => ({
      monsterId,
      startedAt:
        mark?.monsterId === monsterId && mark.until > now
          ? mark.startedAt
          : now,
      until: now + remainingMs,
    }))
  } else {
    bowMark.update((mark) =>
      mark ? { ...mark, until: Math.min(mark.until, now) } : null
    )
  }
}

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

export const visibleAbilityBuffs = derived(
  [activeBuffs, abilityClock],
  ([buffs, now]) =>
    BUFF_ABILITIES.flatMap((ability) => {
      const remaining = (buffs[ability.id] ?? 0) - now
      return remaining > 0 ? [{ ...ability, remaining }] : []
    })
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

export function applyAbilityCooldowns(
  timers: AbilityTimer[],
  now = Date.now()
) {
  abilityCooldowns.set(timerSnapshot(timers, now))
  const dagger = timers.find((timer) => timer.ability === DOUBLE_SLASH.id)
  if (dagger) syncDaggerSkillCooldown(dagger.remaining_ms, now)
  abilityPending.set({})
}

export type AbilityEffectEvent = {
  ability: Exclude<AbilityId, 'bow_mark'>
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
  bowMark.set(null)
  abilityCooldowns.set({})
  activeBuffs.set({})
  abilityPending.set({})
  effects = []
}
