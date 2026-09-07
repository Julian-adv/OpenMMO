import { describe, expect, it } from 'vitest'
import { WORLD_WIDTH_X } from '../terrain/world-wrap'
import { drawRainCells, forecastLabel } from './weatherOverlay'

function stubCtx() {
  const arcs: { x: number; y: number; r: number }[] = []
  const stops: number[][] = []
  const ctx = {
    fillStyle: null as unknown,
    save() {},
    restore() {},
    beginPath() {},
    fill() {},
    arc(x: number, y: number, r: number) {
      arcs.push({ x, y, r })
    },
    createRadialGradient() {
      const alphas: number[] = []
      stops.push(alphas)
      return {
        addColorStop(_offset: number, color: string) {
          alphas.push(Number(color.match(/,\s*([\d.]+)\)$/)![1]))
        },
      }
    },
  }
  return { ctx: ctx as unknown as CanvasRenderingContext2D, arcs, stops }
}

const transform = { centerX: 0, viewLeft: -1000, viewTop: -1000, scale: 0.1 }

describe('drawRainCells', () => {
  it('places a cell by the atlas transform and scales its radius', () => {
    const { ctx, arcs, stops } = stubCtx()
    drawRainCells(
      ctx,
      [{ x: 500, z: -200, radius_m: 2000, env: 0.5, stage: 'raining' }],
      transform
    )
    expect(arcs).toEqual([{ x: 150, y: 80, r: 200 }])
    expect(stops[0][0]).toBeCloseTo(0.36)
    expect(stops[0][2]).toBe(0)
  })

  it('unwraps a cell across the world seam toward the view centre', () => {
    const { ctx, arcs } = stubCtx()
    drawRainCells(
      ctx,
      [
        {
          x: 500 + WORLD_WIDTH_X,
          z: 0,
          radius_m: 1500,
          env: 1,
          stage: 'raining',
        },
      ],
      transform
    )
    expect(arcs[0].x).toBe(150)
  })

  it('skips dead cells and cells too small to see', () => {
    const { ctx, arcs } = stubCtx()
    drawRainCells(
      ctx,
      [
        { x: 0, z: 0, radius_m: 2000, env: 0, stage: 'ended' },
        { x: 0, z: 0, radius_m: 5, env: 1, stage: 'raining' },
      ],
      transform
    )
    expect(arcs).toEqual([])
  })
})

describe('forecastLabel', () => {
  it('formats the look-ahead in game hours and minutes', () => {
    expect(forecastLabel(0)).toBe('Now')
    expect(forecastLabel(30)).toBe('+30m')
    expect(forecastLabel(120)).toBe('+2h')
    expect(forecastLabel(150)).toBe('+2h 30m')
  })
})
