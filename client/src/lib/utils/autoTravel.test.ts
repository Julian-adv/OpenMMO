import { describe, expect, it } from 'vitest'
import {
  planTravelLeg,
  travelDistance,
  TRAVEL_LEG_DISTANCE,
} from './autoTravel'
import { WORLD_MAX_X, WORLD_MIN_X } from '../terrain/world-wrap'

describe('server travel goals', () => {
  it('splits long journeys into bounded goals', () => {
    let position = { x: 0, z: 0 }
    const destination = { x: 800, z: 500 }
    for (let step = 0; step < 50; step++) {
      const leg = planTravelLeg(position, destination, () => true)
      if (leg.kind === 'arrived') break
      expect(leg.kind).toBe('move')
      if (leg.kind !== 'move') throw Error('missing goal')
      expect(travelDistance(position, leg.target)).toBeLessThanOrEqual(
        TRAVEL_LEG_DISTANCE + 1e-6
      )
      position = leg.target
    }
    expect(travelDistance(position, destination)).toBeLessThan(1)
  })

  it('uses the short route across the world seam', () => {
    const leg = planTravelLeg(
      { x: WORLD_MAX_X - 2, z: 0 },
      { x: WORLD_MIN_X + 2, z: 0 },
      () => true
    )
    expect(leg).toEqual({ kind: 'move', target: { x: WORLD_MIN_X + 2, z: 0 } })
  })

  it('waits for terrain and refuses invalid destinations', () => {
    expect(
      planTravelLeg({ x: 0, z: 0 }, { x: 100, z: 0 }, () => false).kind
    ).toBe('waiting')
    expect(
      planTravelLeg({ x: 0, z: 0 }, { x: NaN, z: 0 }, () => true).kind
    ).toBe('blocked')
    expect(
      planTravelLeg({ x: 0, z: 0 }, { x: 0.5, z: 0 }, () => true).kind
    ).toBe('arrived')
  })
})
