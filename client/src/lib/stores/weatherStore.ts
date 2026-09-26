import { writable } from 'svelte/store'
import { weather_set_sectors } from '../wasm/onlinerpg_shared'
import { getTerrainApiUrl } from '../utils/networkUtils'

/** `WeatherSync` payload: enough to evaluate rain anywhere, any time. */
export interface ServerWeather {
  seed: number
  bias: number
  rainOverride: number | null
  /** The forced precipitation falls as snow. */
  snowOverride: boolean
  /** Names the sector list the server loaded; the list is fetched per tag. */
  sectorsTag: string
}

export const weather = writable<ServerWeather | null>(null)
export const weatherSectorsReady = writable(false)

/** Rain and snow parts of the precipitation, each 0..1. */
export interface Precip {
  rain: number
  snow: number
}

/** The admin override split into rain and snow; null while weather runs on
 *  its own. */
export function forcedPrecip(w: ServerWeather): Precip | null {
  if (w.rainOverride === null) return null
  return w.snowOverride
    ? { rain: 0, snow: w.rainOverride }
    : { rain: w.rainOverride, snow: 0 }
}

/** Precipitation at the local player: its parts, their total, and light
 *  dimming 0..1. */
export interface LocalWeather extends Precip {
  precip: number
  cloud: number
}

export const NO_WEATHER: LocalWeather = {
  rain: 0,
  snow: 0,
  precip: 0,
  cloud: 0,
}

/** Null after a failed fetch so the next sync retries. */
let sectorsTag: string | null = null

export function setWeather(next: ServerWeather) {
  weather.set(next)
  if (next.sectorsTag !== sectorsTag) {
    sectorsTag = next.sectorsTag
    weatherSectorsReady.set(false)
    void loadSectors(next.sectorsTag)
  }
}

export function clearWeather() {
  weather.set(null)
  weatherSectorsReady.set(false)
  sectorsTag = null
}

async function loadSectors(tag: string) {
  try {
    const resp = await fetch(
      `${getTerrainApiUrl()}/api/terrain/weather-sectors?v=${encodeURIComponent(tag)}`
    )
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
    const text = await resp.text()
    if (sectorsTag === tag) {
      weather_set_sectors(text)
      weatherSectorsReady.set(true)
    }
  } catch (err) {
    if (sectorsTag === tag) sectorsTag = null
    console.warn('weather: sectors unavailable', err)
  }
}
