import { writable } from 'svelte/store'
import { weather_set_sectors } from '../wasm/onlinerpg_shared'
import { getTerrainApiUrl } from '../utils/networkUtils'

/** `WeatherSync` payload: enough to evaluate rain anywhere, any time. */
export interface ServerWeather {
  seed: number
  bias: number
}

export const weather = writable<ServerWeather | null>(null)
/** True once the baked sector list is parsed into wasm. */
export const weatherSectorsReady = writable(false)

/** Rain 0..1 and light dimming 0..1 at the local player, sampled each frame. */
export interface LocalWeather {
  rain: number
  cloud: number
}

export const NO_WEATHER: LocalWeather = { rain: 0, cloud: 0 }
export const localWeather = writable<LocalWeather>(NO_WEATHER)

let sectorsRequested = false

export function setWeather(next: ServerWeather) {
  weather.set(next)
  if (!sectorsRequested) {
    sectorsRequested = true
    void loadSectors()
  }
}

export function clearWeather() {
  weather.set(null)
  localWeather.set(NO_WEATHER)
  resetWeatherSectors()
}

/** Reconnect: keep the last seed so rain, light, and music hold steady until
 *  the rejoin's `WeatherSync` lands, but re-fetch the list in case the world
 *  was re-baked in between (the server revalidates it, so this is cheap). */
export function resetWeatherSectors() {
  sectorsRequested = false
}

/** The list is one small file per bake; a failed fetch retries on the next
 *  `WeatherSync` (every 30 s) rather than hammering the server. */
async function loadSectors() {
  try {
    const resp = await fetch(
      `${getTerrainApiUrl()}/api/terrain/weather-sectors`
    )
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
    const count = weather_set_sectors(await resp.text())
    weatherSectorsReady.set(count > 0)
  } catch (err) {
    sectorsRequested = false
    console.warn('weather: sectors unavailable', err)
  }
}
