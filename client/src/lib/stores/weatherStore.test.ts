import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'

vi.mock('../wasm/onlinerpg_shared', () => ({
  weather_set_sectors: vi.fn(() => 3),
}))

import { weather_set_sectors } from '../wasm/onlinerpg_shared'
import {
  clearWeather,
  localWeather,
  resetWeatherSectors,
  setWeather,
  weather,
  weatherSectorsReady,
} from './weatherStore'

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('weatherStore', () => {
  beforeEach(() => {
    clearWeather()
    weatherSectorsReady.set(false)
    vi.clearAllMocks()
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('stores the sync payload and loads the sector list once', async () => {
    const fetchMock = vi.fn(async () => ({
      ok: true,
      text: async () => '{"sectors":[]}',
    }))
    vi.stubGlobal('fetch', fetchMock)

    setWeather({ seed: 42, bias: 1 })
    setWeather({ seed: 42, bias: 0.5 })
    await flush()

    expect(get(weather)).toEqual({ seed: 42, bias: 0.5 })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringMatching(/\/api\/terrain\/weather-sectors$/)
    )
    expect(weather_set_sectors).toHaveBeenCalledWith('{"sectors":[]}')
    expect(get(weatherSectorsReady)).toBe(true)

    clearWeather()
    expect(get(weather)).toBeNull()
  })

  it('retries the sector fetch on the next sync after a failure', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce({ ok: false, status: 404 })
      .mockResolvedValueOnce({ ok: true, text: async () => '{"sectors":[]}' })
    vi.stubGlobal('fetch', fetchMock)
    vi.spyOn(console, 'warn').mockImplementation(() => {})

    setWeather({ seed: 1, bias: 1 })
    await flush()
    expect(get(weatherSectorsReady)).toBe(false)

    setWeather({ seed: 1, bias: 1 })
    await flush()
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(get(weatherSectorsReady)).toBe(true)
  })

  it('keeps the seed and local sample across a reconnect but re-fetches the list', async () => {
    const fetchMock = vi.fn(async () => ({
      ok: true,
      text: async () => '{"sectors":[]}',
    }))
    vi.stubGlobal('fetch', fetchMock)

    setWeather({ seed: 42, bias: 1 })
    await flush()
    localWeather.set({ rain: 1, cloud: 1 })

    resetWeatherSectors()
    expect(get(weather)).toEqual({ seed: 42, bias: 1 })
    expect(get(localWeather)).toEqual({ rain: 1, cloud: 1 })
    expect(get(weatherSectorsReady)).toBe(true)

    setWeather({ seed: 42, bias: 1 })
    await flush()
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })
})
