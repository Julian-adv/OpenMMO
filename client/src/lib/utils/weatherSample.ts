import {
  weather_cloud_factor,
  weather_game_minutes,
  weather_rain_at,
} from '../wasm/onlinerpg_shared'
import type { LocalWeather } from '../stores/weatherStore'
import type { CalendarDate } from './celestialSimulation'

/** Game minutes since the calendar epoch at a fractional hour of `date`.
 *  The day index comes from wasm so it cannot drift from the server's. */
export function gameMinutesAt(date: CalendarDate, gameHour: number): number {
  return (
    weather_game_minutes(date.year, date.month, date.day, 0, 0) + gameHour * 60
  )
}

const PUBLISH_STEP = 0.005

/** Store writes are rate-limited by a deadband, but idle must always land:
 *  a value decaying into the band would otherwise never reach zero. */
export function weatherChanged(
  prev: LocalWeather,
  next: LocalWeather
): boolean {
  return (
    Math.abs(next.rain - prev.rain) > PUBLISH_STEP ||
    Math.abs(next.cloud - prev.cloud) > PUBLISH_STEP ||
    (next.rain === 0 && prev.rain !== 0)
  )
}

export function sampleLocalWeather(
  seed: number,
  bias: number,
  date: CalendarDate,
  gameHour: number,
  x: number,
  z: number
): LocalWeather {
  const rain = weather_rain_at(seed, bias, gameMinutesAt(date, gameHour), x, z)
  return { rain, cloud: weather_cloud_factor(rain) }
}
