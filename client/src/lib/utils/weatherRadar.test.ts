import { beforeEach, describe, expect, it, vi } from 'vitest'

/** Rain at a fixed spot: dry, then a shower from t=63 to t=69. */
const SHOWER_START = 63
const SHOWER_END = 69

vi.mock('../wasm/onlinerpg_shared', () => ({
  weather_cells_at: vi.fn(() => []),
  weather_rain_at: vi.fn((_seed: number, _bias: number, t: number) =>
    t >= SHOWER_START && t < SHOWER_END ? 0.8 : 0
  ),
}))

import {
  CONTINENT_VIEW,
  canvasHeightFor,
  canvasToWorld,
  cellInView,
  formatGameMinutes,
  formatRealMinutes,
  minutesUntilRain,
  viewWrappedX,
  worldToCanvas,
  zoneName,
} from './weatherRadar'

describe('weatherRadar projection', () => {
  const width = 1100

  it('keeps the view aspect so cells stay round', () => {
    const height = canvasHeightFor(CONTINENT_VIEW, width)
    const sx = width / (CONTINENT_VIEW.x1 - CONTINENT_VIEW.x0)
    const sz = height / (CONTINENT_VIEW.z1 - CONTINENT_VIEW.z0)
    expect(Math.abs(sx - sz)).toBeLessThan(0.001)
  })

  it('maps the view corners onto the canvas corners', () => {
    const height = canvasHeightFor(CONTINENT_VIEW, width)
    expect(
      worldToCanvas(CONTINENT_VIEW.x0, CONTINENT_VIEW.z0, CONTINENT_VIEW, width)
    ).toEqual({ x: 0, y: 0 })
    const far = worldToCanvas(
      CONTINENT_VIEW.x1,
      CONTINENT_VIEW.z1,
      CONTINENT_VIEW,
      width
    )
    expect(far.x).toBeCloseTo(width, 6)
    expect(far.y).toBeCloseTo(height, 0)
  })

  it('round-trips a world position through the canvas', () => {
    const point = { x: -1475, z: 4742 }
    const px = worldToCanvas(point.x, point.z, CONTINENT_VIEW, width)
    const back = canvasToWorld(px.x, px.y, CONTINENT_VIEW, width)
    expect(back.x).toBeCloseTo(point.x, 6)
    expect(back.z).toBeCloseTo(point.z, 6)
  })
})

describe('the world seam', () => {
  it('leaves a position inside the view alone', () => {
    expect(viewWrappedX(-1475, CONTINENT_VIEW)).toBeCloseTo(-1475, 6)
    expect(viewWrappedX(12000, CONTINENT_VIEW)).toBeCloseTo(12000, 6)
  })

  it('brings a cell just west of the seam round to the east of the view', () => {
    // Sector 63 in the shipped bake sits at x -15,728; its rain reaches the
    // east edge of the window the short way round, as rain_at measures it.
    expect(viewWrappedX(-15728, CONTINENT_VIEW)).toBeCloseTo(17040, 6)
  })

  it('counts a seam-crossing cell as in view when its disc reaches the window', () => {
    const seamCell = { x: -15728, z: 1264, radiusM: 5000 }
    expect(cellInView(seamCell, CONTINENT_VIEW)).toBe(true)
    expect(cellInView({ ...seamCell, radiusM: 1000 }, CONTINENT_VIEW)).toBe(
      false
    )
  })

  it('drops a cell that rains nowhere near the window', () => {
    expect(
      cellInView({ x: -1475, z: 40000, radiusM: 5000 }, CONTINENT_VIEW)
    ).toBe(false)
  })

  it('keeps a cell whose centre is outside but whose edge reaches in', () => {
    expect(
      cellInView(
        { x: CONTINENT_VIEW.x0 - 2000, z: 5000, radiusM: 3000 },
        CONTINENT_VIEW
      )
    ).toBe(true)
  })
})

describe('minutesUntilRain', () => {
  beforeEach(() => vi.clearAllMocks())

  it('reports 0 while it is already raining', () => {
    expect(minutesUntilRain(42, 1, SHOWER_START + 1, 0, 0)).toBe(0)
  })

  it('finds the onset minute itself, not a rounded-up one', () => {
    expect(minutesUntilRain(42, 1, 0, 0, 0)).toBe(SHOWER_START)
  })

  it('returns null when the horizon stays dry', () => {
    expect(minutesUntilRain(42, 1, SHOWER_END, 0, 0, 40)).toBeNull()
  })

  it('catches a shower shorter than a coarse stride would be', () => {
    // 6 game minutes long. Scanning every minute has to see it; this is the
    // case that made the default step 1 instead of a stride plus refine.
    expect(minutesUntilRain(42, 1, 0, 0, 0)).toBe(SHOWER_START)
    expect(minutesUntilRain(42, 1, 0, 0, 0, 720, 10)).toBeNull()
  })
})

describe('formatting', () => {
  it('formats game minutes by magnitude', () => {
    expect(formatGameMinutes(0)).toBe('0m')
    expect(formatGameMinutes(45)).toBe('45m')
    expect(formatGameMinutes(125)).toBe('2h 05m')
    expect(formatGameMinutes(1620)).toBe('1d 03h')
  })

  it('converts game minutes to real time at 7.5 s each', () => {
    expect(formatRealMinutes(160)).toBe('20 real min')
    expect(formatRealMinutes(4)).toBe('<1 real min')
  })

  it('names climate zones and falls back for unknown ones', () => {
    expect(zoneName(1)).toBe('Wet coast')
    expect(zoneName(9)).toBe('Zone 9')
  })
})
