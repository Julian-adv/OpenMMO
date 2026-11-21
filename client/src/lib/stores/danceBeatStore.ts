/** Player id → `performance.now()` at which their looping emote's beat
 *  started. Not reactive: readers poll it per frame or per scan. */
export const danceAnchors = new Map<number, number>()

export function setDanceBeat(
  playerId: number,
  elapsedMs: number,
  now = performance.now()
) {
  danceAnchors.set(playerId, now - elapsedMs)
}

export function clearDanceBeat(playerId: number) {
  danceAnchors.delete(playerId)
}

/** Seconds into a clip `duration` long that a beat anchored at `anchor` has
 *  reached at `now`. */
export function danceBeatTime(
  anchor: number,
  duration: number,
  now = performance.now()
): number {
  if (!(duration > 0)) return 0
  const t = ((now - anchor) / 1000) % duration
  return t < 0 ? t + duration : t
}
