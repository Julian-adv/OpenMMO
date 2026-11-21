import { JOINABLE_DANCE_ANIMS } from './stores/emoteStore'
import {
  join_dance_max_rise_m,
  join_dance_range_m,
} from './wasm/onlinerpg_shared'
import { shortestWrappedDeltaX } from './terrain/world-wrap'

/** Beats closer than this, in ms, belong to one group. Generous, since a
 *  throttled tab can stamp a group's beats tens of ms apart. */
export const IN_STEP_MS = 250

interface Spot {
  x: number
  y: number
  z: number
}

export interface DanceCandidate {
  id: number
  name: string
  anim: string | undefined
  alive: boolean
  petrified: boolean
  floor: number
  position: Spot
}

export interface DanceOffer {
  id: number
  name: string
  dance: string
}

export function isJoinableDancer(candidate: DanceCandidate): boolean {
  return (
    JOINABLE_DANCE_ANIMS.has(candidate.anim ?? '') &&
    candidate.alive &&
    !candidate.petrified
  )
}

/** Dancers the player `meId` could join, nearest first: on their floor and
 *  within reach, not already in step with them, and none in a dungeon.
 *  `beats` maps player ids to `danceAnchors` values. */
export function joinableDancers(
  meId: number,
  floor: number,
  position: Spot,
  candidates: Iterable<DanceCandidate>,
  beats: ReadonlyMap<number, number>
): DanceOffer[] {
  if (floor < 0) return []
  const myBeat = beats.get(meId)
  const reachSq = join_dance_range_m() ** 2
  const maxRise = join_dance_max_rise_m()
  const offers: (DanceOffer & { distanceSq: number })[] = []
  for (const candidate of candidates) {
    if (
      !isJoinableDancer(candidate) ||
      candidate.floor !== floor ||
      Math.abs(candidate.position.y - position.y) > maxRise
    )
      continue
    const beat = beats.get(candidate.id)
    if (
      myBeat !== undefined &&
      beat !== undefined &&
      Math.abs(beat - myBeat) < IN_STEP_MS
    )
      continue
    const dx = shortestWrappedDeltaX(position.x, candidate.position.x)
    const dz = candidate.position.z - position.z
    const distanceSq = dx * dx + dz * dz
    if (distanceSq <= reachSq)
      offers.push({
        id: candidate.id,
        name: candidate.name,
        dance: candidate.anim ?? '',
        distanceSq,
      })
  }
  return offers
    .sort((a, b) => a.distanceSq - b.distanceSq)
    .map(({ id, name, dance }) => ({ id, name, dance }))
}
