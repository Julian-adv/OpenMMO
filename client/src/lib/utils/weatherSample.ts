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

export function sampleLocalWeather(
  seed: number,
  date: CalendarDate,
  gameHour: number,
  x: number,
  z: number
): LocalWeather {
  const rain = weather_rain_at(seed, gameMinutesAt(date, gameHour), x, z)
  return { rain, cloud: weather_cloud_factor(rain) }
}
