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

/** Rain 0..1 and light dimming 0..1 at the local player. */
export interface LocalWeather {
  rain: number
  cloud: number
}

export const NO_WEATHER: LocalWeather = { rain: 0, cloud: 0 }

/** Tag of the list requested or loaded; a sync repeating it fetches nothing,
 *  so rain holds steady through a rejoin. Null after a failed fetch so the
 *  next sync (every 30 s) retries. */
let sectorsTag: string | null = null

export function setWeather(next: ServerWeather) {
  weather.set(next)
  if (next.sectorsTag !== sectorsTag) {
    sectorsTag = next.sectorsTag
    void loadSectors(next.sectorsTag)
  }
}

export function clearWeather() {
  weather.set(null)
  sectorsTag = null
}

async function loadSectors(tag: string) {
  try {
    const resp = await fetch(
      `${getTerrainApiUrl()}/api/terrain/weather-sectors?v=${encodeURIComponent(tag)}`
    )
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
    const text = await resp.text()
    if (sectorsTag === tag) weather_set_sectors(text)
  } catch (err) {
    if (sectorsTag === tag) sectorsTag = null
    console.warn('weather: sectors unavailable', err)
  }
}
