import { writable } from 'svelte/store'
import type { Position } from '../network/networkTypes'

/** Server `EstateDefenseState` (doc/ESTATE_DEFENSE.md). */
export type EstateDefenseState = {
  estate_id: number
  banner: Position
  wave: number
  next_wave_in_ms: number | null
  /** 0 up, 1 right, 2 down, 3 left on screen; null for every side. */
  next_side: number | null
  wave_total: number
  banner_hp: number
  banner_max: number
  ends_in_ms: number
  monsters: string[]
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

/** A summoning pillar: a raider rises in it, or a held banner's chest. */
export type DefensePillar = {
  position: Position
  startedAt: number
  riseMs: number
  /** A chest that opens `riseMs` in. */
  chest?: boolean
}

const NONE: readonly DefensePillar[] = []
let pending: DefensePillar[] = []

/** Polled every frame, so the usual empty answer allocates nothing. */
export function takeDefensePillars(): readonly DefensePillar[] {
  return pending.length ? pending.splice(0) : NONE
}

export function handleEstateDefenseStatus(state: EstateDefenseState) {
  estateDefense.set({ state, receivedAt: Date.now() })
}

export function handleEstateDefenseWave(spawns: Position[], riseMs: number) {
  const startedAt = Date.now()
  pending.push(...spawns.map((position) => ({ position, startedAt, riseMs })))
}

export function handleEstateDefenseEnded(
  estateId: number,
  banner: Position,
  outcome: EstateDefenseOutcome,
  openInMs: number
) {
  estateDefense.update((current) =>
    current?.state.estate_id === estateId ? null : current
  )
  if (outcome === 'held')
    pending.push({
      position: banner,
      startedAt: Date.now(),
      riseMs: openInMs,
      chest: true,
    })
}

export function resetEstateDefense() {
  estateDefense.set(null)
  pending = []
}
