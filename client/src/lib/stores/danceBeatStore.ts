import { SvelteMap } from 'svelte/reactivity'

/** Player id → `performance.now()` at which their looping emote's beat
 *  started, from `PlayerDanceSync`. Kept apart from `PlayerState`, which the
 *  remote movement loop rebuilds every frame. The local player is keyed by
 *  their own id like everyone else. */
export const danceAnchors = new SvelteMap<number, number>()

/** Record a beat `elapsedMs` into the loop as of `now`. */
export function setDanceBeat(
  playerId: number,
  elapsedMs: number,
  now = performance.now()
) {
  danceAnchors.set(playerId, now - elapsedMs)
}

/** A new pose (or none) ends the old beat; a new loop sends its own. */
export function clearDanceBeat(playerId: number) {
  danceAnchors.delete(playerId)
}

/** Seconds into a clip `duration` long that a beat anchored at `anchor` has
 *  reached at `now` — where every viewer seeks the same loop. */
export function danceBeatTime(
  anchor: number,
  duration: number,
  now = performance.now()
): number {
  if (!(duration > 0)) return 0
  const t = ((now - anchor) / 1000) % duration
  return t < 0 ? t + duration : t
}
