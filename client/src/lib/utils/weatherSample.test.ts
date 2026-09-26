import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../wasm/onlinerpg_shared', () => ({
  weather_day_start_minutes: vi.fn(
    (year: number, month: number, day: number) =>
      (year - 217) * 360 * 1440 + (month - 1) * 30 * 1440 + (day - 1) * 1440
  ),
  weather_precip_at: vi.fn(() => [0.4, 0.2]),
  weather_cloud_factor: vi.fn((precip: number) => precip * 0.5),
  weather_set_sectors: vi.fn(),
}))

import { weather_precip_at } from '../wasm/onlinerpg_shared'
import type { ServerWeather } from '../stores/weatherStore'
import { gameMinutesAt, sampleLocalWeather } from './weatherSample'

const natural: ServerWeather = {
  seed: 42,
  bias: 1,
  sectorsTag: 'aa',
  rainOverride: null,
  snowOverride: false,
}

describe('weatherSample', () => {
  beforeEach(() => vi.clearAllMocks())

  it('adds the fractional game hour to the day from wasm', () => {
    expect(gameMinutesAt({ year: 217, month: 1, day: 1 }, 0)).toBe(0)
    expect(gameMinutesAt({ year: 217, month: 1, day: 2 }, 1.5)).toBe(1440 + 90)
  })

  it('samples rain and snow at the player and dims by their total', () => {
    const sample = sampleLocalWeather(
      natural,
      { year: 217, month: 1, day: 1 },
      12,
      100,
      -50
    )
    expect(weather_precip_at).toHaveBeenCalledWith(42, 1, 720, 100, -50)
    expect(sample.rain).toBeCloseTo(0.4)
    expect(sample.snow).toBeCloseTo(0.2)
    expect(sample.precip).toBeCloseTo(0.6)
    expect(sample.cloud).toBeCloseTo(0.3)
  })

  it.each([0, 0.4, 1])(
    'overrides natural weather with %s until auto resumes',
    (amount) => {
      const date = { year: 217, month: 7, day: 1 }
      const forced = { ...natural, rainOverride: amount }
      expect(sampleLocalWeather(forced, date, 12, 100, -50)).toEqual({
        rain: amount,
        snow: 0,
        precip: amount,
        cloud: amount * 0.5,
      })
      expect(
        sampleLocalWeather({ ...forced, snowOverride: true }, date, 12, 0, 0)
      ).toEqual({
        rain: 0,
        snow: amount,
        precip: amount,
        cloud: amount * 0.5,
      })
      expect(weather_precip_at).not.toHaveBeenCalled()

      sampleLocalWeather(natural, date, 12, 100, -50)
      expect(weather_precip_at).toHaveBeenCalledOnce()
    }
  )
})
