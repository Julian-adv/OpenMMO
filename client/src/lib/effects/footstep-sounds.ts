import { shortestWrappedDeltaX } from '../terrain/world-wrap'
import type { WalkSurface } from '../utils/housing-textures'
import type { MovementMode } from '../utils/movementUtils'

/** Distance covered between steps. */
const STRIDE_M = 0.75
/** The walk clip plants a foot every 0.52 s at the same 3 m/s. */
const WALK_STRIDE_M = 1.55
/** A frame moving farther than this is a teleport, not a stride. */
const TELEPORT_M = 5
export const FOOTSTEP_HEARING_RANGE_M = 12

interface Stride {
  x: number
  z: number
  walked: number
}

export interface Walker {
  x: number
  y: number
  z: number
  /** On foot and moving under their own power. */
  walking: boolean
  mode?: MovementMode
}

/**
 * Step sounds for the local player and walkers within earshot. The surface is
 * looked up once per stride, not per frame.
 */
export class FootstepSounds {
  private local: Stride | null = null
  private remote = new Map<number, Stride>()
  private spare = new Map<number, Stride>()

  constructor(
    private surfaceAt: (x: number, y: number, z: number) => WalkSurface | null,
    private play: (surface: WalkSurface, distance: number) => void
  ) {}

  update(self: Walker | null, others?: Iterable<[number, Walker]>) {
    if (!self) {
      this.local = null
      this.remote.clear()
      return
    }
    this.local = this.step(this.local, self, 0)

    const next = this.spare
    next.clear()
    for (const [id, other] of others ?? []) {
      const distance = Math.hypot(
        shortestWrappedDeltaX(self.x, other.x),
        other.z - self.z,
        other.y - self.y
      )
      if (distance >= FOOTSTEP_HEARING_RANGE_M) continue
      const stride = this.step(this.remote.get(id) ?? null, other, distance)
      if (stride) next.set(id, stride)
    }
    this.spare = this.remote
    this.remote = next
  }

  private step(
    last: Stride | null,
    w: Walker,
    distance: number
  ): Stride | null {
    if (!w.walking) return null
    if (!last) return { x: w.x, z: w.z, walked: 0 }
    const moved = Math.hypot(shortestWrappedDeltaX(last.x, w.x), w.z - last.z)
    last.x = w.x
    last.z = w.z
    if (moved > TELEPORT_M) {
      last.walked = 0
      return last
    }
    last.walked += moved
    const stride = w.mode === 'walk' ? WALK_STRIDE_M : STRIDE_M
    if (last.walked < stride) return last
    last.walked = 0
    const surface = this.surfaceAt(w.x, w.y, w.z)
    if (surface) this.play(surface, distance)
    return last
  }
}
