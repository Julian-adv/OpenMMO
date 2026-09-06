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

let sectorsRequested = false

export function setWeather(next: ServerWeather) {
  weather.set(next)
  if (!sectorsRequested) {
    sectorsRequested = true
    void loadSectors()
  }
}

/** Disconnect: the next session re-fetches the list, which may have been
 *  re-baked in between (the server revalidates it, so this is cheap). */
export function clearWeather() {
  weather.set(null)
  sectorsRequested = false
}

/** The list is one small file per bake; a failed fetch retries on the next
 *  `WeatherSync` (every 30 s) rather than hammering the server. */
async function loadSectors() {
  try {
    const resp = await fetch(`${getTerrainApiUrl()}/api/terrain/weather-sectors`)
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
    const count = weather_set_sectors(await resp.text())
    weatherSectorsReady.set(count > 0)
  } catch (err) {
    sectorsRequested = false
    console.warn('weather: sectors unavailable', err)
  }
}
