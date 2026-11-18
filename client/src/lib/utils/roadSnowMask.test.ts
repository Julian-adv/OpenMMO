import { describe, expect, it } from 'vitest'
import {
  clearStrength,
  paintRoadSnowMask,
  SNOW_CLEAR_LASTS_MIN,
} from './roadSnowMask'

const RES = 64
const SIZE = 64
const at = (mask: Uint8Array, x: number, z: number) =>
  mask[Math.floor(z + SIZE / 2) * RES + Math.floor(x + SIZE / 2)]

describe('road snow mask', () => {
  it('holds a shovelled stretch clear, then lets the snow back', () => {
    expect(clearStrength(0)).toBe(1)
    expect(clearStrength(SNOW_CLEAR_LASTS_MIN * 0.5)).toBe(1)
    expect(clearStrength(SNOW_CLEAR_LASTS_MIN * 0.85)).toBeCloseTo(0.5)
    expect(clearStrength(SNOW_CLEAR_LASTS_MIN)).toBe(0)
  })

  it('clears a strip along the road and nothing beside it', () => {
    const mask = new Uint8Array(RES * RES)
    const road = {
      segments: [[0, -20, 0, 20]] as [number, number, number, number][],
      at_min: 100,
    }
    paintRoadSnowMask(mask, RES, SIZE, 0, 0, [road], 100)
    expect(at(mask, 0, 0)).toBe(255)
    expect(at(mask, 2, 10)).toBe(255)
    expect(at(mask, 8, 0)).toBe(0)
    expect(at(mask, 0, 28)).toBe(0)
  })

  it('keeps the outer ring snowy and drops stretches past their time', () => {
    const mask = new Uint8Array(RES * RES)
    const road = {
      segments: [[-40, 0, 40, 0]] as [number, number, number, number][],
      at_min: 0,
    }
    paintRoadSnowMask(mask, RES, SIZE, 0, 0, [road], 10)
    expect(mask[32 * RES]).toBe(0)
    expect(mask[32 * RES + RES - 1]).toBe(0)
    expect(mask[32 * RES + 1]).toBe(255)
    paintRoadSnowMask(mask, RES, SIZE, 0, 0, [road], SNOW_CLEAR_LASTS_MIN + 1)
    expect(mask.every((value) => value === 0)).toBe(true)
  })
})
