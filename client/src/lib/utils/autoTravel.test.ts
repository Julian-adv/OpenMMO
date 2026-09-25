import { describe, expect, it, vi } from 'vitest'
import {
  isTravelDestinationValid,
  planTravelLeg,
  travelDistance,
  TRAVEL_LEG_DISTANCE,
  type TravelDestination,
} from './autoTravel'
import { WORLD_MAX_X, WORLD_MIN_X, wrapWorldX } from '../terrain/world-wrap'

const loaded = () => true
const directPath = (target: TravelDestination) => ({
  found: true,
  waypoints: [{ ...target, floor: 0 }],
})

describe('automatic travel', () => {
  it('continues over many local routes until a distant destination is reached', () => {
    let position = { x: 0, z: 0 }
    const destination = { x: 3000, z: 4000 }
    let legs = 0
    while (legs < 200) {
      const plan = planTravelLeg(position, destination, loaded, directPath)
      if (plan.kind === 'arrived') break
      expect(plan.kind).toBe('move')
      if (plan.kind !== 'move') throw new Error('Travel stopped early')
      expect(travelDistance(position, plan.target)).toBeLessThanOrEqual(
        TRAVEL_LEG_DISTANCE + 1e-8
      )
      position = plan.waypoints.at(-1)!
      legs++
    }
    expect(legs).toBeGreaterThan(100)
    expect(travelDistance(position, destination)).toBeLessThanOrEqual(1)
  })

  it.each([1, -1])(
    'takes the short route across the world seam (%s)',
    (direction) => {
      const position = {
        x: direction === 1 ? WORLD_MAX_X - 10 : WORLD_MIN_X + 10,
        z: 0,
      }
      const destination = { x: wrapWorldX(position.x + direction * 100), z: 0 }
      const findPath = vi.fn(directPath)
      const plan = planTravelLeg(position, destination, loaded, findPath)
      expect(plan.kind).toBe('move')
      expect(findPath).toHaveBeenCalledWith({
        x: position.x + direction * TRAVEL_LEG_DISTANCE,
        z: 0,
      })
      if (plan.kind !== 'move') throw new Error('Missing route')
      expect(plan.waypoints[0].x).toBe(
        wrapWorldX(position.x + direction * TRAVEL_LEG_DISTANCE)
      )
    }
  )

  it('waits for terrain before querying a route, then resumes when loaded', () => {
    const findPath = vi.fn(directPath)
    const from = { x: 0, z: 0 }
    const destination = { x: 100, z: 0 }
    expect(planTravelLeg(from, destination, () => false, findPath).kind).toBe(
      'waiting'
    )
    expect(findPath).not.toHaveBeenCalled()
    expect(planTravelLeg(from, destination, loaded, findPath).kind).toBe('move')
  })

  it('waits when a detour crosses terrain that has not loaded', () => {
    const plan = planTravelLeg(
      { x: 0, z: 0 },
      { x: 100, z: 0 },
      (_x, z) => z < 16,
      (target) => ({
        found: true,
        waypoints: [
          { x: 0, z: 32, floor: 0 },
          { ...target, floor: 0 },
        ],
      })
    )
    expect(plan.kind).toBe('waiting')
  })

  it('uses obstacle detours from the pathfinder', () => {
    const waypoints = [
      { x: 0, z: 12, floor: 0 },
      { x: 48, z: 0, floor: 0 },
    ]
    const plan = planTravelLeg(
      { x: 0, z: 0 },
      { x: 100, z: 0 },
      loaded,
      () => ({ found: true, waypoints })
    )
    expect(plan).toMatchObject({ kind: 'move', waypoints })
  })

  it('continues a partial route only if it advances toward the destination', () => {
    const from = { x: 0, z: 0 }
    const destination = { x: 100, z: 0 }
    for (const [x, kind] of [
      [20, 'move'],
      [0, 'blocked'],
      [-10, 'blocked'],
    ] as const) {
      expect(
        planTravelLeg(from, destination, loaded, () => ({
          found: false,
          waypoints: [{ x, z: 0, floor: 0 }],
        })).kind
      ).toBe(kind)
    }
  })

  it('does not fall back to walking through a wall when no path exists', () => {
    expect(
      planTravelLeg({ x: 0, z: 0 }, { x: 100, z: 0 }, loaded, () => ({
        found: false,
        waypoints: [],
      })).kind
    ).toBe('blocked')
  })

  it('stops a snapped route that cannot advance', () => {
    expect(
      planTravelLeg({ x: 0, z: 0 }, { x: 10, z: 0 }, loaded, () =>
        directPath({ x: 0, z: 0 })
      ).kind
    ).toBe('blocked')
  })

  it('does not send another move after arrival', () => {
    const findPath = vi.fn(directPath)
    expect(
      planTravelLeg({ x: 99.5, z: 0 }, { x: 100, z: 0 }, loaded, findPath).kind
    ).toBe('arrived')
    expect(findPath).not.toHaveBeenCalled()
  })

  it('rejects invalid points and north/south world overflow', () => {
    expect(isTravelDestinationValid({ x: WORLD_MAX_X + 100, z: 0 })).toBe(true)
    for (const point of [
      { x: NaN, z: 0 },
      { x: 0, z: Infinity },
      { x: 0, z: WORLD_MIN_X - 1 },
      { x: 0, z: WORLD_MAX_X },
    ]) {
      expect(isTravelDestinationValid(point)).toBe(false)
    }
  })
})
