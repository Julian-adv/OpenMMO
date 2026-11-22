import { describe, it, expect } from 'vitest'
import { FootstepSounds, type Walker } from './footstep-sounds'

const walker = (x: number, over: Partial<Walker> = {}): Walker => ({
  x,
  y: 0,
  z: 0,
  walking: true,
  ...over,
})

function setup(wood = true) {
  const played: number[] = []
  const lookups: number[] = []
  const steps = new FootstepSounds(
    (x) => {
      lookups.push(x)
      return wood ? 'wood' : null
    },
    (_, distance) => played.push(distance)
  )
  return { steps, played, lookups }
}

describe('FootstepSounds', () => {
  it('sounds one step per stride and looks the surface up only then', () => {
    const { steps, played, lookups } = setup()
    for (let x = 0; x <= 3.2; x += 0.05) steps.update(walker(x))
    expect(played).toEqual([0, 0, 0, 0])
    expect(lookups).toHaveLength(4)
  })

  it('takes longer strides in walk mode', () => {
    const { steps, played } = setup()
    for (let x = 0; x <= 3.2; x += 0.05) {
      steps.update(walker(x, { mode: 'walk' }))
    }
    expect(played).toHaveLength(2)
  })

  it('stays silent off wood', () => {
    const { steps, played } = setup(false)
    for (let x = 0; x <= 3; x += 0.05) steps.update(walker(x))
    expect(played).toEqual([])
  })

  it('ignores movement while not walking and teleports', () => {
    const { steps, played } = setup()
    for (let x = 0; x <= 3; x += 0.05) {
      steps.update(walker(x, { walking: false }))
    }
    steps.update(walker(0))
    steps.update(walker(50))
    expect(played).toEqual([])
  })

  it('reports how far a nearby walker is and drops distant ones', () => {
    const { steps, played } = setup()
    const self = walker(0, { walking: false })
    for (let z = 0; z <= 0.8; z += 0.05) {
      steps.update(self, [
        [1, walker(6, { z })],
        [2, walker(30, { z })],
      ])
    }
    expect(played).toHaveLength(1)
    expect(played[0]).toBeCloseTo(6, 0)
  })
})
