import type { ObjectDef, ObjectPlacement } from '../stores/editorStore'
import type { Position } from '../network/networkTypes'
import { shortestWrappedDeltaX } from '../terrain/world-wrap'

export function normalizeYaw(yaw: number): number {
  return ((yaw % 360) + 360) % 360
}

export interface Doorway {
  /** Door index: 0 is the entrance, side doors follow. */
  door: number
  leaves: { node: string; openDeg: number }[]
  ax: number
  az: number
  bx: number
  bz: number
}

/** World segments of an object's doors, as the server's `shelter::doorways`. */
export function doorwaysOf(
  placement: ObjectPlacement,
  def: Pick<ObjectDef, 'perimeter' | 'swingDoor' | 'sideDoors'>
): Doorway[] {
  const perimeter = def.perimeter
  if (!perimeter) return []
  const yaw = (placement.rotation * Math.PI) / 180
  const cos = Math.cos(yaw)
  const sin = Math.sin(yaw)
  const hw = perimeter.entranceWidth / 2
  const hd = perimeter.depth / 2
  const doors = [
    ...(def.swingDoor
      ? [{ door: 0, segment: [-hw, hd, hw, hd], leaves: def.swingDoor.leaves }]
      : []),
    ...(def.sideDoors ?? []).map((side, index) => ({
      door: index + 1,
      segment: side.segment,
      leaves: side.leaves,
    })),
  ]
  return doors.map(({ door, segment: [ax, az, bx, bz], leaves }) => ({
    door,
    leaves,
    ax: placement.x + ax * cos + az * sin,
    az: placement.z - ax * sin + az * cos,
    bx: placement.x + bx * cos + bz * sin,
    bz: placement.z - bx * sin + bz * cos,
  }))
}

export function isInsideShelter(
  point: Position,
  placement: ObjectPlacement,
  perimeter: NonNullable<ObjectDef['perimeter']>,
  annexes: NonNullable<ObjectDef['annexes']> = []
): boolean {
  const yaw = (placement.rotation * Math.PI) / 180
  const dx = shortestWrappedDeltaX(placement.x, point.x)
  const dz = point.z - placement.z
  const x = dx * Math.cos(yaw) - dz * Math.sin(yaw)
  const z = dx * Math.sin(yaw) + dz * Math.cos(yaw)
  const y = point.y - placement.y
  if (y <= -0.3) return false
  return (
    (Math.abs(x) < perimeter.width / 2 &&
      Math.abs(z) < perimeter.depth / 2 &&
      y < perimeter.height) ||
    annexes.some(
      (a) =>
        x > a.x[0] &&
        x < a.x[1] &&
        z > a.z[0] &&
        z < a.z[1] &&
        y < (a.height ?? perimeter.height)
    )
  )
}
