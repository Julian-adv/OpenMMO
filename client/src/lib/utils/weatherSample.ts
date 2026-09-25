import {
  weather_cloud_factor,
  weather_day_start_minutes,
  weather_rain_at,
} from '../wasm/onlinerpg_shared'
import type { LocalWeather } from '../stores/weatherStore'
import type { CalendarDate } from './celestialSimulation'

/** Game minutes since the epoch, using the shared calendar. */
export function gameMinutesAt(date: CalendarDate, gameHour: number): number {
  return (
    weather_day_start_minutes(date.year, date.month, date.day) + gameHour * 60
  )
}

export function sampleLocalWeather(
  seed: number,
  bias: number,
  date: CalendarDate,
  gameHour: number,
  x: number,
  z: number,
  rainOverride: number | null = null
): LocalWeather {
  const rain =
    rainOverride ??
    weather_rain_at(seed, bias, gameMinutesAt(date, gameHour), x, z)
  return { rain, cloud: weather_cloud_factor(rain) }
}
