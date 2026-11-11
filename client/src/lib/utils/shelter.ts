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

export interface OccluderBox {
  x: [number, number]
  z: [number, number]
  height: number
}

/** Whether a local box stands between `point` and the fixed south-west camera,
 *  by the same ray test houses use. */
export function boxesOccludePoint(
  point: Position,
  placement: ObjectPlacement,
  boxes: OccluderBox[],
  minDepth = 0.9
): boolean {
  const yaw = (placement.rotation * Math.PI) / 180
  const cos = Math.cos(yaw)
  const sin = Math.sin(yaw)
  const px = shortestWrappedDeltaX(placement.x, point.x)
  const pz = point.z - placement.z
  const py = point.y - placement.y
  for (const box of boxes) {
    const sHigh = box.height - py
    if (sHigh <= 0) continue
    let minX = Infinity
    let maxX = -Infinity
    let minZ = Infinity
    let maxZ = -Infinity
    for (const x of box.x) {
      for (const z of box.z) {
        const wx = x * cos + z * sin
        const wz = -x * sin + z * cos
        minX = Math.min(minX, wx)
        maxX = Math.max(maxX, wx)
        minZ = Math.min(minZ, wz)
        maxZ = Math.max(maxZ, wz)
      }
    }
    const sMin = Math.max(px - maxX, minZ - pz, Math.max(-py, 0))
    const sMax = Math.min(px - minX, maxZ - pz, sHigh)
    if (sMax - sMin > minDepth) return true
  }
  return false
}

export function isInsideShelter(
  point: Position,
  placement: ObjectPlacement,
  perimeter: NonNullable<ObjectDef['perimeter']>
): boolean {
  const yaw = (placement.rotation * Math.PI) / 180
  const dx = shortestWrappedDeltaX(placement.x, point.x)
  const dz = point.z - placement.z
  const x = dx * Math.cos(yaw) - dz * Math.sin(yaw)
  const z = dx * Math.sin(yaw) + dz * Math.cos(yaw)
  const y = point.y - placement.y
  return (
    Math.abs(x) < perimeter.width / 2 &&
    Math.abs(z) < perimeter.depth / 2 &&
    y > -0.3 &&
    y < perimeter.height
  )
}
