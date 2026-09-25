import type { EstateFurniturePlacement } from './estatePlacement'

const offsets = {
  ArrowUp: { x: 0, z: -1 },
  ArrowDown: { x: 0, z: 1 },
  ArrowLeft: { x: -1, z: 0 },
  ArrowRight: { x: 1, z: 0 },
}

type NudgeKey = keyof typeof offsets

export function isFurnitureNudgeKey(code: string): code is NudgeKey {
  return Object.hasOwn(offsets, code)
}

export class EstateFurniturePlacementCursor {
  pointer: { x: number; y: number } | null = null
  position: EstateFurniturePlacement['position'] | null = null

  movePointer(x: number, y: number) {
    if (this.pointer?.x === x && this.pointer.y === y) return
    this.pointer = { x, y }
    this.position = null
  }

  prepareSave(x: number, y: number) {
    if (!this.position) this.pointer = { x, y }
  }

  nudge(key: NudgeKey, position: EstateFurniturePlacement['position']) {
    const offset = offsets[key]
    this.position = {
      x: Math.round(position.x * 20 + offset.x) / 20,
      y: position.y,
      z: Math.round(position.z * 20 + offset.z) / 20,
    }
  }
}
