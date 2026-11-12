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
  /** `DoorState` id. */
  door: string
  y: number
  ax: number
  az: number
  bx: number
  bz: number
}

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

/** Door id of a clicked object among the loaded doorways. */
export function objectDoorOf(objectId: number): string | null {
  return doorways.find((door) => door.objectId === objectId)?.door ?? null
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
