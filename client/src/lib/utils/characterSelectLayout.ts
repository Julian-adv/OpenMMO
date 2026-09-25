import { Vector3, type Camera } from 'three'

export interface CharacterSlotLayout {
  x: number
  y: number
  width: number
}

export function projectCharacterSlots(
  camera: Camera,
  positions: readonly number[],
  depth: number,
  viewport: { width: number; height: number }
): CharacterSlotLayout[] {
  const slots = positions.map((x) => {
    const point = new Vector3(x, -0.9, depth).project(camera)
    return {
      x: ((point.x + 1) * viewport.width) / 2,
      y: ((1 - point.y) * viewport.height) / 2,
    }
  })
  const gap = Math.min(
    viewport.width / Math.max(1, slots.length),
    ...slots.slice(1).map((slot, i) => slot.x - slots[i].x)
  )
  const width = Math.max(1, Math.min(240, gap - 16))
  return slots.map((slot) => ({ ...slot, width }))
}
