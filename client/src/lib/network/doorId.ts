import type { WallDirection } from '../utils/house-geometry'

/** A door as named in `ToggleDoor` and `DoorState` (shared `door::DoorId`). */
export type DoorRef =
  | {
      kind: 'house'
      houseId: string
      roomIndex: number
      wallDir: WallDirection
      segmentIndex: number
    }
  | { kind: 'dungeon'; entranceId: string; depth: number; doorId: number }
  | { kind: 'object'; rx: number; rz: number; objectId: number }

const WALL_DIRS: readonly string[] = ['north', 'south', 'east', 'west']

export function houseDoorId(
  houseId: string,
  roomIndex: number,
  wallDir: WallDirection,
  segmentIndex: number
): string {
  return `house:${houseId}:${roomIndex}:${wallDir}:${segmentIndex}`
}

export function dungeonDoorId(
  entranceId: string,
  depth: number,
  doorId: number
): string {
  return `dungeon:${entranceId}:${depth}:${doorId}`
}

export function objectDoorId(rx: number, rz: number, id: number): string {
  return `object:${rx}:${rz}:${id}`
}

function int(text: string | undefined): number | null {
  return text !== undefined && /^-?\d+$/.test(text) ? Number(text) : null
}

/** House and entrance ids are free text, so the numbers are read from the right. */
export function parseDoorId(door: string): DoorRef | null {
  const parts = door.split(':')
  const kind = parts.shift()
  if (kind === 'object') {
    const [rx, rz, objectId] = parts.map(int)
    if (parts.length !== 3 || rx == null || rz == null || objectId == null)
      return null
    return objectId < 0 ? null : { kind, rx, rz, objectId }
  }
  if (kind === 'dungeon') {
    const doorId = int(parts.pop())
    const depth = int(parts.pop())
    const entranceId = parts.join(':')
    if (doorId == null || depth == null || doorId < 0 || depth < 0) return null
    return entranceId ? { kind, entranceId, depth, doorId } : null
  }
  if (kind === 'house') {
    const segmentIndex = int(parts.pop())
    const wallDir = parts.pop()
    const roomIndex = int(parts.pop())
    const houseId = parts.join(':')
    if (segmentIndex == null || roomIndex == null) return null
    if (segmentIndex < 0 || roomIndex < 0 || !houseId) return null
    if (wallDir === undefined || !WALL_DIRS.includes(wallDir)) return null
    return {
      kind,
      houseId,
      roomIndex,
      wallDir: wallDir as WallDirection,
      segmentIndex,
    }
  }
  return null
}
