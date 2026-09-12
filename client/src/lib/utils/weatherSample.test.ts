import { describe, expect, it, vi } from 'vitest'

vi.mock('../wasm/onlinerpg_shared', () => ({
  weather_game_minutes: vi.fn(
    (year: number, month: number, day: number, hour: number, minute: number) =>
      (year - 217) * 360 * 1440 +
      (month - 1) * 30 * 1440 +
      (day - 1) * 1440 +
      hour * 60 +
      minute
  ),
  weather_rain_at: vi.fn(() => 0.6),
  weather_cloud_factor: vi.fn((rain: number) => rain * 0.5),
}))

import { weather_rain_at } from '../wasm/onlinerpg_shared'
import {
  gameMinutesAt,
  sampleLocalWeather,
  weatherChanged,
} from './weatherSample'

describe('weatherSample', () => {
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

  it('publishes past the deadband and always lands on idle', () => {
    expect(
      weatherChanged({ rain: 0.5, cloud: 0 }, { rain: 0.503, cloud: 0 })
    ).toBe(false)
    expect(
      weatherChanged({ rain: 0.5, cloud: 0 }, { rain: 0.51, cloud: 0 })
    ).toBe(true)
    expect(
      weatherChanged({ rain: 0.004, cloud: 0 }, { rain: 0, cloud: 0 })
    ).toBe(true)
    expect(weatherChanged({ rain: 0, cloud: 0 }, { rain: 0, cloud: 0 })).toBe(
      false
    )
  })
})
