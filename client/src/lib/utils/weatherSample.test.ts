import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../wasm/onlinerpg_shared', () => ({
  weather_day_start_minutes: vi.fn(
    (year: number, month: number, day: number) =>
      (year - 217) * 360 * 1440 + (month - 1) * 30 * 1440 + (day - 1) * 1440
  ),
  weather_rain_at: vi.fn(() => 0.6),
  weather_cloud_factor: vi.fn((rain: number) => rain * 0.5),
}))

import { weather_rain_at } from '../wasm/onlinerpg_shared'
import { gameMinutesAt, sampleLocalWeather } from './weatherSample'

describe('weatherSample', () => {
  beforeEach(() => vi.clearAllMocks())

  it('adds the fractional game hour to the day from wasm', () => {
    expect(gameMinutesAt({ year: 217, month: 1, day: 1 }, 0)).toBe(0)
    expect(gameMinutesAt({ year: 217, month: 1, day: 2 }, 1.5)).toBe(1440 + 90)
  })

  it('samples rain at the player and derives the cloud factor', () => {
    const sample = sampleLocalWeather(
      42,
      1,
      { year: 217, month: 1, day: 1 },
      12,
      100,
      -50
    )
    expect(weather_rain_at).toHaveBeenCalledWith(42, 1, 720, 100, -50)
    expect(sample).toEqual({ rain: 0.6, cloud: 0.3 })
  })

  it.each([0, 0.4, 1])(
    'overrides natural rain with %s until auto resumes',
    (rain) => {
      const date = { year: 217, month: 7, day: 1 }
      expect(sampleLocalWeather(42, 1, date, 12, 100, -50, rain)).toEqual({
        rain,
        cloud: rain * 0.5,
      })
      expect(weather_rain_at).not.toHaveBeenCalled()

      expect(sampleLocalWeather(42, 1, date, 12, 100, -50, null)).toEqual({
        rain: 0.6,
        cloud: 0.3,
      })
      expect(weather_rain_at).toHaveBeenCalledOnce()
    }
  )
})
