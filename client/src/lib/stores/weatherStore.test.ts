import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'

vi.mock('../wasm/onlinerpg_shared', () => ({
  weather_set_sectors: vi.fn(),
}))

import { weather_set_sectors } from '../wasm/onlinerpg_shared'
import { clearWeather, setWeather, weather } from './weatherStore'

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))
const okList = { ok: true, text: async () => '{"sectors":[]}' }

describe('weatherStore', () => {
  beforeEach(() => {
    clearWeather()
    vi.clearAllMocks()
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('stores the sync payload and loads the sector list once per tag', async () => {
    const fetchMock = vi.fn(async () => okList)
    vi.stubGlobal('fetch', fetchMock)

    setWeather({ seed: 42, bias: 1, sectorsTag: 'aa' })
    setWeather({ seed: 42, bias: 0.5, sectorsTag: 'aa' })
    await flush()
    setWeather({ seed: 42, bias: 0.5, sectorsTag: 'aa' })
    await flush()

    expect(get(weather)).toEqual({ seed: 42, bias: 0.5, sectorsTag: 'aa' })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringMatching(/\/api\/terrain\/weather-sectors\?v=aa$/)
    )
    expect(weather_set_sectors).toHaveBeenCalledWith('{"sectors":[]}')

    clearWeather()
    expect(get(weather)).toBeNull()
  })

  it('retries the sector fetch on the next sync after a failure', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce({ ok: false, status: 404 })
      .mockResolvedValueOnce(okList)
    vi.stubGlobal('fetch', fetchMock)
    vi.spyOn(console, 'warn').mockImplementation(() => {})

    setWeather({ seed: 1, bias: 1, sectorsTag: 'aa' })
    await flush()
    expect(weather_set_sectors).not.toHaveBeenCalled()

    setWeather({ seed: 1, bias: 1, sectorsTag: 'aa' })
    await flush()
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(weather_set_sectors).toHaveBeenCalledTimes(1)
  })

  it('re-fetches only when the tag changes', async () => {
    const fetchMock = vi.fn(async () => okList)
    vi.stubGlobal('fetch', fetchMock)

    setWeather({ seed: 42, bias: 1, sectorsTag: 'aa' })
    await flush()
    setWeather({ seed: 42, bias: 1, sectorsTag: 'aa' })
    await flush()
    expect(fetchMock).toHaveBeenCalledTimes(1)

    setWeather({ seed: 42, bias: 1, sectorsTag: 'bb' })
    await flush()
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(fetchMock).toHaveBeenLastCalledWith(expect.stringMatching(/\?v=bb$/))
  })

  it('ignores a stale response when the tag moved on while it was in flight', async () => {
    let resolveOld: (v: unknown) => void = () => {}
    const fetchMock = vi
      .fn()
      .mockImplementationOnce(
        () => new Promise((resolve) => (resolveOld = resolve))
      )
      .mockResolvedValueOnce(okList)
    vi.stubGlobal('fetch', fetchMock)

    setWeather({ seed: 1, bias: 1, sectorsTag: 'old' })
    setWeather({ seed: 1, bias: 1, sectorsTag: 'new' })
    await flush()
    expect(weather_set_sectors).toHaveBeenCalledTimes(1)

    resolveOld({ ok: true, text: async () => '{"sectors":[{"stale":1}]}' })
    await flush()
    expect(weather_set_sectors).toHaveBeenCalledTimes(1)
    expect(weather_set_sectors).toHaveBeenCalledWith('{"sectors":[]}')
  })
})
