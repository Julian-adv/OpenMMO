import { describe, expect, it, vi } from 'vitest'
import { WorldView, type WorldUpdate } from './worldView'

vi.mock('../wasm/onlinerpg_shared', () => ({
  world_constants: () => ({ eventDeliveryRadius: 32 }),
}))

function update(sequence: number, generation = 1, reset = false): WorldUpdate {
  return {
    world_epoch: 'world',
    ready: true,
    generation,
    sequence,
    reset,
    position: { x: 0, y: 0, z: 0 },
    floor_level: 0,
    events: [],
  }
}

describe('world stream', () => {
  it('waits for tile application before moving through the confirmed area', () => {
    const view = new WorldView()
    view.accept(update(1, 1, true))
    expect(view.covers(30, 0)).toBe(true)
    expect(view.covers(33, 0)).toBe(false)
    view.pendingTerrain.add('0,0')
    expect(view.covers(1, 0)).toBe(false)
    view.pendingTerrain.clear()
    expect(view.covers(1, 0)).toBe(true)
    view.staticReady = false
    expect(view.covers(1, 0)).toBe(false)
    view.staticReady = true
    view.accept({ ...update(2), ready: false })
    expect(view.covers(1, 0)).toBe(false)
  })

  it('ignores duplicate packets and retired epochs without removing current subjects', () => {
    const view = new WorldView()
    view.accept(update(1, 1, true))
    expect(view.accept(update(1, 1, true))).toBe(false)
    expect(view.synchronized).toBe(true)
    view.accept({ ...update(1, 1, true), world_epoch: 'new' })
    expect(view.accept(update(1, 2, true))).toBe(false)
    expect(view.epoch).toBe('new')
    expect(view.synchronized).toBe(true)
  })

  it('rejects missing packets until a new snapshot completes', () => {
    const view = new WorldView()
    expect(view.accept(update(1, 1, true))).toBe(true)
    expect(view.accept(update(3))).toBe(false)
    expect(view.synchronized).toBe(false)
    expect(view.accept(update(2))).toBe(false)
    expect(view.accept(update(1, 2, true))).toBe(true)
  })

  it('does not let an old leave remove a new subscription', () => {
    const view = new WorldView()
    const entered = update(1, 2, true)
    entered.events.push({
      subject: 'house:a',
      revision: 3,
      change: 'Enter',
      messages: [],
    })
    expect(view.accept(entered)).toBe(true)
    const stale = update(2)
    stale.events.push({
      subject: 'house:a',
      revision: 2,
      change: 'Leave',
      messages: [],
    })
    expect(view.accept(stale)).toBe(false)
    expect(view.subjects.get('house:a')).toBe(3)
  })
})
