import type { ObjectDef, ObjectPlacement } from '../stores/editorStore'
import type { Position } from '../network/networkTypes'
import { shortestWrappedDeltaX } from '../terrain/world-wrap'

export function normalizeYaw(yaw: number): number {
  return ((yaw % 360) + 360) % 360
}

/** World segment across the entrance, as the server's `shelter::doorway`. */
export function doorwayOf(
  placement: ObjectPlacement,
  perimeter: NonNullable<ObjectDef['perimeter']>
): { ax: number; az: number; bx: number; bz: number } {
  const yaw = (placement.rotation * Math.PI) / 180
  const cos = Math.cos(yaw)
  const sin = Math.sin(yaw)
  const hw = perimeter.entranceWidth / 2
  const hd = perimeter.depth / 2
  return {
    ax: placement.x - hw * cos + hd * sin,
    az: placement.z + hw * sin + hd * cos,
    bx: placement.x + hw * cos + hd * sin,
    bz: placement.z - hw * sin + hd * cos,
  }
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
