import { get, writable } from 'svelte/store'
import type { Position } from '../network/networkTypes'
import type { Emblem } from '../utils/familyEmblem'
import { createEvent } from '../network/networkEvents'

export const warHornBlown =
  createEvent<(playerId: number, gain: number) => void>()

/** Server `EstateDefenseState` (doc/ESTATE_DEFENSE.md). */
export type EstateDefenseState = {
  estate_id: number
  banner: Position
  wave: number
  next_wave_in_ms?: number | null
  /** Where the next wave rises; null for the last, from every side. */
  next_from?: Position | null
  wave_total: number
  banner_hp: number
  banner_max: number
  ends_in_ms: number
  monsters: string[]
  emblem?: Emblem | null
}

export type EstateDefenseOutcome =
  | 'held'
  | 'banner_fell'
  | 'wiped'
  | 'timed_out'

/** The defense on screen, with the `Date.now()` its state arrived. */
export const estateDefense = writable<{
  state: EstateDefenseState
  receivedAt: number
} | null>(null)

/** Screen angles (radians, counterclockwise from right) the next wave comes
 *  from, as the scene layer projects them. */
export const incomingAngles = writable<readonly number[]>([])

/** A summoning pillar: a raider rises in it, or a held banner's chest. */
export type DefensePillar = {
  position: Position
  startedAt: number
  riseMs: number
  /** A chest that opens `riseMs` in. */
  chest?: boolean
  /** The wave's leader, who shakes the ground as it rises. */
  leader?: boolean
}

const NONE: readonly DefensePillar[] = []
let pending: DefensePillar[] = []

/** Polled every frame, so the usual empty answer allocates nothing. */
export function takeDefensePillars(): readonly DefensePillar[] {
  return pending.length ? pending.splice(0) : NONE
}

/** When the banner last took a blow, by `Date.now()`. */
export let bannerStruckAt = 0

/** True when the banner just took a blow. */
export function handleEstateDefenseStatus(state: EstateDefenseState): boolean {
  const before = get(estateDefense)?.state
  const struck =
    before?.estate_id === state.estate_id && state.banner_hp < before.banner_hp
  if (struck) bannerStruckAt = Date.now()
  estateDefense.set({ state, receivedAt: Date.now() })
  return struck
}

export function handleEstateDefenseWave(
  spawns: Position[],
  riseMs: number,
  leader: number | null | undefined
) {
  const startedAt = Date.now()
  pending.push(
    ...spawns.map((position, i) => ({
      position,
      startedAt,
      riseMs,
      leader: i === leader,
    }))
  )
}

/** How long the pillars that take the last raiders rise before they go. */
const VANISH_RISE_MS = 700

/** A lost banner falling where it stood, from `at` (`Date.now()`). */
export let bannerFall: { position: Position; at: number } | null = null

export function handleEstateDefenseEnded(
  estateId: number,
  banner: Position,
  outcome: EstateDefenseOutcome,
  openInMs: number,
  vanished: Position[],
  vanishInMs: number
) {
  estateDefense.update((current) =>
    current?.state.estate_id === estateId ? null : current
  )
  const now = Date.now()
  pending.push(
    ...vanished.map((position) => ({
      position,
      startedAt: now + vanishInMs - VANISH_RISE_MS,
      riseMs: VANISH_RISE_MS,
    }))
  )
  if (outcome === 'held')
    pending.push({
      position: banner,
      startedAt: now,
      riseMs: openInMs,
      chest: true,
    })
  else bannerFall = { position: banner, at: now }
}

export function resetEstateDefense() {
  estateDefense.set(null)
  pending = []
  bannerStruckAt = 0
  bannerFall = null
}
