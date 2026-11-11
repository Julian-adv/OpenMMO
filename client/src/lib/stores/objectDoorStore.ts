import { writable } from 'svelte/store'
import type { Position } from '../network/networkTypes'
import { shortestWrappedDeltaX } from '../terrain/world-wrap'

/** Ids of open doors as sent in `DoorState`; every other door is shut. */
export const openDoors = writable<ReadonlySet<string>>(new Set())

export function objectDoorId(rx: number, rz: number, id: number): string {
  return `object:${rx}:${rz}:${id}`
}

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

export function findNearestObjectDoor(
  point: Position,
  range: number
): number | null {
  let best: number | null = null
  let bestDistance = range
  for (const door of doorways) {
    const distance = doorwayDistance(point, door)
    if (distance <= bestDistance) {
      bestDistance = distance
      best = door.objectId
    }
  }
  return best
}
