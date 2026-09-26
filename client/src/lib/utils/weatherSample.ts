import {
  weather_cloud_factor,
  weather_day_start_minutes,
  weather_precip_at,
} from '../wasm/onlinerpg_shared'
import {
  forcedPrecip,
  type LocalWeather,
  type Precip,
  type ServerWeather,
} from '../stores/weatherStore'
import type { CalendarDate } from './celestialSimulation'

/** Game minutes since the epoch, using the shared calendar. */
export function gameMinutesAt(date: CalendarDate, gameHour: number): number {
  return (
    weather_day_start_minutes(date.year, date.month, date.day) + gameHour * 60
  )
}

/** Rain and snow at a point and time, honouring the admin override. */
export function precipAt(
  w: ServerWeather,
  tMin: number,
  x: number,
  z: number
): Precip {
  const forced = forcedPrecip(w)
  if (forced) return forced
  const [rain, snow] = weather_precip_at(w.seed, w.bias, tMin, x, z)
  return { rain, snow }
}

export function sampleLocalWeather(
  w: ServerWeather,
  date: CalendarDate,
  gameHour: number,
  x: number,
  z: number
): LocalWeather {
  const { rain, snow } = precipAt(w, gameMinutesAt(date, gameHour), x, z)
  const precip = rain + snow
  return { rain, snow, precip, cloud: weather_cloud_factor(precip) }
}
