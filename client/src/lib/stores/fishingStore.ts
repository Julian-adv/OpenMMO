import { get, writable } from 'svelte/store'
import type {
  FishingAction,
  FishingOutcome,
  FishState,
  Position,
} from '../network/networkTypes'

export const FISHING_CATCH_DURATION = 3.6
export type FishingCatch = {
  fish: Extract<FishingOutcome, { Caught: unknown }>['Caught']
  waterPosition: Position
  startedAt: number
}
let catches = new Map<number, FishingCatch>()
const catchTimers = new Map<number, ReturnType<typeof setTimeout>>()
export const fishingCatches = writable(catches)

export function removeFishingCatch(playerId: number) {
  clearTimeout(catchTimers.get(playerId))
  catchTimers.delete(playerId)
  if (!catches.has(playerId)) return
  catches = new Map(catches)
  catches.delete(playerId)
  fishingCatches.set(catches)
}

export function landFishingCatch(playerId: number, fish: FishingCatch['fish']) {
  const bobber = bobbers.get(playerId)
  removeBobber(playerId)
  if (!bobber) return
  catches = new Map(catches)
  catches.set(playerId, {
    fish,
    waterPosition: { ...bobber.position },
    startedAt: Date.now(),
  })
  fishingCatches.set(catches)
  catchTimers.set(
    playerId,
    setTimeout(
      () => removeFishingCatch(playerId),
      FISHING_CATCH_DURATION * 1000
    )
  )
}

export type FishingStance = Exclude<FishingAction, 'hook'>
let localStance: FishingStance = 'hold'

/** The local player's live fight readout, refreshed by each `FishingFight`
 *  beat (4 Hz). The simulation is server-authoritative. */
export type FightStatus = {
  fishState: FishState
  /** Line tension 0–100; the line snaps at 100. */
  tension: number
  /** Fish stamina 0–100; at 0 it goes exhausted and can be landed. */
  stamina: number
  trophy: boolean
}

/** The local player's place in the fishing loop — one value, so phase and
 *  fight readout can never disagree. `casting` covers the whole
 *  cast-through-wait stretch (only the server knows when the wait ends);
 *  `bite` is the act-now window. */
export type MyFishing =
  | { phase: 'idle' | 'casting' | 'bite' }
  | { phase: 'fight'; fight: FightStatus }

export const myFishing = writable<MyFishing>({ phase: 'idle' })
export const fishingTargeting = writable(false)

export function cancelFishingTargeting() {
  fishingTargeting.set(false)
}

export function queueFishingTarget() {
  if (get(myFishing).phase !== 'idle') return
  fishingTargeting.update((active) => !active)
}

/** Apply a `FishingFight` beat. Opens the fight phase from `bite` (the first
 *  beat follows the hook) but never resurrects one from `idle`/`casting` —
 *  a beat racing a `FishingEnded` must stay dead. */
export function applyFightUpdate(
  fishState: FishState,
  tension: number,
  stamina: number,
  trophy = false
) {
  myFishing.update((f) => {
    if (f.phase === 'fight' || f.phase === 'bite') {
      if (f.phase === 'bite') localStance = 'hold'
      return { phase: 'fight', fight: { fishState, tension, stamina, trophy } }
    }
    return f
  })
}

/** A fighting fish's public readout, rendered as bobber motion and splash
 *  for everyone nearby (broadcasts carry it — agent parity). */
export type BobberFight = {
  fishState: FishState
  stamina: number
  stance: FishingStance
}

export type BobberState = {
  position: Position
  /** Flight time left before the cast lands: the renderer keeps the float
   *  hidden (and the line undrawn) until this many ms have elapsed. */
  landsInMs: number
  /** True once the fish bit — the bobber renders its dip. */
  bite: boolean
  /** Set while the hooked fight runs; drives splash and drag motion. */
  fight?: BobberFight
}

/** Every visible bobber, keyed by owning player id (broadcasts are
 *  radius-gated server-side, so this map is already "nearby only").
 *
 *  Reactivity contract: the store notifies only on add/remove — the 4 Hz
 *  in-fight fields (`position`, `bite`, `fight`) are mutated in place and
 *  read imperatively per frame by `FishingBobber`'s task, never from
 *  templates. That keeps a beat from re-rendering every bobber row. */
let bobbers = new Map<number, BobberState>()
export const fishingBobbers = writable<Map<number, BobberState>>(bobbers)

export function upsertBobber(
  playerId: number,
  position: Position,
  landsInMs = 0
) {
  removeFishingCatch(playerId)
  bobbers = new Map(bobbers)
  bobbers.set(playerId, { position, landsInMs, bite: false })
  fishingBobbers.set(bobbers)
}

export function markBobberBite(playerId: number) {
  const existing = bobbers.get(playerId)
  if (existing) existing.bite = true
}

/** A fight beat moves the bobber with the fish and updates its readout. */
export function updateBobberFight(
  playerId: number,
  position: Position,
  fishState: FishState,
  stamina: number,
  stance: FishingStance = 'hold'
) {
  const existing = bobbers.get(playerId)
  if (existing) {
    existing.position = position
    existing.bite = false
    existing.fight = { fishState, stamina, stance }
  }
}

export function setLocalFishingAction(action: FishingAction) {
  if (action !== 'hook' && get(myFishing).phase === 'fight')
    localStance = action
}

export function fishingReelStance(playerId?: number): FishingStance | null {
  if (playerId !== undefined)
    return bobbers.get(playerId)?.fight?.stance ?? null
  return get(myFishing).phase === 'fight' ? localStance : null
}

export function removeBobber(playerId: number) {
  removeFishingCatch(playerId)
  if (!bobbers.has(playerId)) return
  bobbers = new Map(bobbers)
  bobbers.delete(playerId)
  fishingBobbers.set(bobbers)
}

export function resetFishingStore() {
  cancelFishingTargeting()
  for (const timer of catchTimers.values()) clearTimeout(timer)
  catchTimers.clear()
  catches = new Map()
  fishingCatches.set(catches)
  localStance = 'hold'
  myFishing.set({ phase: 'idle' })
  bobbers = new Map()
  fishingBobbers.set(bobbers)
}
