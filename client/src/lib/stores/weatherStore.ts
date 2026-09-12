import { writable } from 'svelte/store'
import { weather_set_sectors } from '../wasm/onlinerpg_shared'
import { getTerrainApiUrl } from '../utils/networkUtils'

/** `WeatherSync` payload: enough to evaluate rain anywhere, any time. */
export interface ServerWeather {
  seed: number
  bias: number
  /** Names the sector list the server loaded; the list is fetched per tag. */
  sectorsTag: string
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

let loadedTag: string | null = null
let requestedTag: string | null = null

/** A reconnect or the 30 s sync with an unchanged tag fetches nothing; the
 *  list held in wasm stays valid, so rain does not blink during a rejoin. */
export function setWeather(next: ServerWeather) {
  weather.set(next)
  if (next.sectorsTag !== loadedTag && next.sectorsTag !== requestedTag) {
    requestedTag = next.sectorsTag
    void loadSectors(next.sectorsTag)
  }
}

export function clearWeather() {
  weather.set(null)
  localWeather.set(NO_WEATHER)
  loadedTag = null
  requestedTag = null
}

/** The URL carries the tag so the response can be cached as immutable; a
 *  failed fetch retries on the next `WeatherSync` rather than hammering. */
async function loadSectors(tag: string) {
  try {
    const resp = await fetch(
      `${getTerrainApiUrl()}/api/terrain/weather-sectors?v=${encodeURIComponent(tag)}`
    )
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
    const text = await resp.text()
    if (requestedTag !== tag) return
    const count = weather_set_sectors(text)
    loadedTag = tag
    weatherSectorsReady.set(count > 0)
  } catch (err) {
    if (requestedTag === tag) requestedTag = null
    console.warn('weather: sectors unavailable', err)
  }
}
