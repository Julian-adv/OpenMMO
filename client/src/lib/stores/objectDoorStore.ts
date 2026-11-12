import { writable } from 'svelte/store'
import type { Position } from '../network/networkTypes'
import { shortestWrappedDeltaX } from '../terrain/world-wrap'

/** Ids of open object doors as sent in `DoorState`; every other one is shut. */
export const openDoors = writable<ReadonlySet<string>>(new Set())

export function setDoorState(door: string, isOpen: boolean) {
  openDoors.update((doors) => {
    if (doors.has(door) === isOpen) return doors
    const next = new Set(doors)
    if (isOpen) next.add(door)
    else next.delete(door)
    return next
  })
}

export function resetDoorStates() {
  openDoors.set(new Set())
}

export interface ObjectDoorway {
  objectId: number
  /** 0 is the object's entrance; side doors follow. */
  index: number
  /** `DoorState` id. */
  door: string
  y: number
  ax: number
  az: number
  bx: number
  bz: number
}

/** As the agent's door approach in `agent-client/src/driver/walk.rs`. */
const DOORWAY_SIDE_OFFSET = 0.8

let doorways: ObjectDoorway[] = []

export function setObjectDoorways(next: ObjectDoorway[]) {
  doorways = next
}

function doorwayDistance(point: Position, door: ObjectDoorway): number {
  if (Math.abs(point.y - door.y) > 3) return Infinity
  const dx = shortestWrappedDeltaX(door.ax, point.x)
  const dz = point.z - door.az
  const vx = shortestWrappedDeltaX(door.ax, door.bx)
  const vz = door.bz - door.az
  const t = Math.min(
    1,
    Math.max(0, (dx * vx + dz * vz) / Math.max(vx * vx + vz * vz, 1e-6))
  )
  return Math.hypot(dx - vx * t, dz - vz * t)
}

/** Doorway of a clicked leaf among the loaded ones. */
export function objectDoorwayOf(
  objectId: number,
  index: number
): ObjectDoorway | null {
  return (
    doorways.find(
      (door) => door.objectId === objectId && door.index === index
    ) ?? null
  )
}

/** Standing points either side of a doorway, nearest to `from` first. */
export function doorwaySides(door: ObjectDoorway, from: Position): Position[] {
  const vx = shortestWrappedDeltaX(door.ax, door.bx)
  const vz = door.bz - door.az
  const length = Math.max(Math.hypot(vx, vz), 1e-3)
  const reach = DOORWAY_SIDE_OFFSET / length
  const x = door.ax + vx / 2
  const z = (door.az + door.bz) / 2
  const distance = (p: Position) =>
    Math.hypot(shortestWrappedDeltaX(from.x, p.x), p.z - from.z)
  return [1, -1]
    .map((sign) => ({
      x: x + sign * vz * reach,
      y: door.y,
      z: z - sign * vx * reach,
    }))
    .sort((a, b) => distance(a) - distance(b))
}

export function findNearestObjectDoor(
  point: Position,
  range: number
): string | null {
  let best: string | null = null
  let bestDistance = range
  for (const door of doorways) {
    const distance = doorwayDistance(point, door)
    if (distance <= bestDistance) {
      bestDistance = distance
      best = door.door
    }
  }
  return best
}
