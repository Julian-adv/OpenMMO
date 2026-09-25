import { describe, expect, it } from 'vitest'
import {
  EstateFurniturePlacementCursor,
  isFurnitureNudgeKey,
} from './estateFurniturePlacementCursor'

describe('estate furniture keyboard placement', () => {
  it.each([
    ['ArrowUp', 10, 19.95],
    ['ArrowDown', 10, 20.05],
    ['ArrowLeft', 9.95, 20],
    ['ArrowRight', 10.05, 20],
  ])('nudges %s by five centimeters on the world axes', (key, x, z) => {
    const cursor = new EstateFurniturePlacementCursor()
    const position = { x: 10, y: 3.25, z: 20 }
    if (!isFurnitureNudgeKey(key)) throw new Error('Expected an arrow key')
    cursor.nudge(key, position)
    expect(cursor.position).toEqual({ x, y: 3.25, z })
    expect(position).toEqual({ x: 10, y: 3.25, z: 20 })
  })

  it('keeps a precise placement through a save click and stationary pointer events', () => {
    const cursor = new EstateFurniturePlacementCursor()
    cursor.movePointer(100, 200)
    cursor.nudge('ArrowRight', { x: 10, y: 3, z: 20 })
    cursor.movePointer(100, 200)
    cursor.prepareSave(100, 200)
    expect(cursor.position).toEqual({ x: 10.05, y: 3, z: 20 })

    cursor.nudge('ArrowUp', cursor.position!)
    cursor.prepareSave(100, 200)
    expect(cursor.position).toEqual({ x: 10.05, y: 3, z: 19.95 })
  })

  it('can nudge and save immediately after selection without moving the mouse', () => {
    const cursor = new EstateFurniturePlacementCursor()
    cursor.nudge('ArrowLeft', { x: 10, y: 3, z: 20 })
    cursor.prepareSave(100, 200)
    expect(cursor.position).toEqual({ x: 9.95, y: 3, z: 20 })
  })

  it('returns to pointer placement when the mouse moves', () => {
    const cursor = new EstateFurniturePlacementCursor()
    cursor.movePointer(100, 200)
    cursor.nudge('ArrowRight', { x: 10, y: 3, z: 20 })
    cursor.movePointer(101, 200)
    expect(cursor.position).toBeNull()
    expect(cursor.pointer).toEqual({ x: 101, y: 200 })
    cursor.prepareSave(102, 201)
    expect(cursor.pointer).toEqual({ x: 102, y: 201 })
  })

  it('keeps repeated nudges on the five-centimeter grid', () => {
    const cursor = new EstateFurniturePlacementCursor()
    for (let i = 0; i < 100; i++)
      cursor.nudge('ArrowRight', cursor.position ?? { x: 10, y: 3, z: 20 })
    expect(cursor.position).toEqual({ x: 15, y: 3, z: 20 })
    for (let i = 0; i < 100; i++) cursor.nudge('ArrowLeft', cursor.position!)
    expect(cursor.position).toEqual({ x: 10, y: 3, z: 20 })
  })
})
