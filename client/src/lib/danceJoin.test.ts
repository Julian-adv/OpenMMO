import { describe, expect, it } from 'vitest'
import { type DanceCandidate, IN_STEP_MS, joinableDancers } from './danceJoin'
import { JOIN_DANCE_MAX_RISE_M, JOIN_DANCE_RANGE_M } from './stores/emoteStore'

const ME = 1
const HERE = { x: 0, y: 0, z: 0 }

function dancer(
  id: number,
  x: number,
  overrides: Partial<DanceCandidate> = {}
): DanceCandidate {
  return {
    id,
    name: `p${id}`,
    anim: 'twist',
    alive: true,
    petrified: false,
    floor: 0,
    position: { x, y: 0, z: 0 },
    ...overrides,
  }
}

function offers(
  candidates: DanceCandidate[],
  beats = new Map<number, number>(),
  floor = 0
) {
  return joinableDancers(ME, floor, HERE, candidates, beats).map((o) => o.id)
}

describe('joinableDancers', () => {
  it('lists joinable dancers in reach, nearest first', () => {
    expect(
      offers([
        dancer(2, 3),
        dancer(3, 2),
        dancer(4, 1, { anim: 'clap' }),
        dancer(5, JOIN_DANCE_RANGE_M + 0.1),
      ])
    ).toEqual([3, 2])
  })

  it('names the dance on offer', () => {
    expect(
      joinableDancers(
        ME,
        0,
        HERE,
        [dancer(2, 1, { anim: 'macarena' })],
        new Map()
      )
    ).toEqual([{ id: 2, name: 'p2', dance: 'macarena' }])
  })

  it('skips dancers who cannot be joined', () => {
    const skipped = [
      dancer(2, 1, { anim: 'stand_pose2' }),
      dancer(3, 1, { alive: false }),
      dancer(4, 1, { petrified: true }),
      dancer(5, 1, { floor: 1 }),
      dancer(6, 1, {
        position: { x: 1, y: JOIN_DANCE_MAX_RISE_M + 0.1, z: 0 },
      }),
    ]
    expect(offers(skipped)).toEqual([])
  })

  it('offers nothing in a dungeon', () => {
    expect(offers([dancer(2, 1, { floor: -1 })], undefined, -1)).toEqual([])
  })

  it('offers another group while dancing, but not my own', () => {
    const beats = new Map([
      [ME, 1000],
      [2, 1000 + IN_STEP_MS / 2],
      [3, 5000],
    ])
    expect(offers([dancer(2, 1), dancer(3, 2)], beats)).toEqual([3])
    expect(offers([dancer(2, 1)])).toEqual([2])
  })
})
