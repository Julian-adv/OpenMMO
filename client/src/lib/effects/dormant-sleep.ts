import { MathUtils, type Object3D } from 'three'

/** A boss going back to sleep stands idle this long, its fight over, before
 *  it sinks into the ground over `SLEEP_SINK_MS`. Times below run from the
 *  start of the sink. */
export const SLEEP_CALM_MS = 3000
export const SLEEP_SINK_MS = 2400
/** Its stand-in climbs back out over this window, inside the dust. */
export const SLEEP_RISE_FROM_MS = 1300
const SLEEP_RISE_TO_MS = 3300

/** How far (0–1) a sleeping boss has sunk `elapsedMs` in. */
export function sleepSink(elapsedMs: number) {
  return MathUtils.smoothstep(elapsedMs, 0, SLEEP_SINK_MS)
}

/** How far (0–1) the stand-in has risen back `elapsedMs` in. */
export function sleepRise(elapsedMs: number) {
  const t = MathUtils.clamp(
    (elapsedMs - SLEEP_RISE_FROM_MS) / (SLEEP_RISE_TO_MS - SLEEP_RISE_FROM_MS),
    0,
    1
  )
  return 1 - (1 - t) ** 3
}

/** A waking statue shakes off the stone this long (server `WAKE_MS`). */
export const STATUE_WAKE_MS = 800

/** The tremor a dormant boss and its stand-in share. */
export function shake(object: Object3D, t: number, amp: number) {
  object.position.x += Math.sin(t * 53) * amp
  object.position.z += Math.sin(t * 47 + 1.3) * amp
}
