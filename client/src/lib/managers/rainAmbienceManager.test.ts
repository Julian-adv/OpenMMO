import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'

vi.mock('./sfxManager', () => ({
  getSfxMultiplier: vi.fn(() => 1),
}))

import {
  getLightningDirection,
  getLightningStrength,
  stopRainAmbience,
  updateRainAmbience,
} from './rainAmbienceManager'
import { getSfxMultiplier } from './sfxManager'
import { lightningEnabled } from '../stores/effectSettings'

interface FakeGain {
  gain: { value: number }
}

interface FakeContext {
  started: number
  gains: FakeGain[]
  pending: { url: string; resolve: (value: unknown) => void }[]
  release: (failedUrl?: string) => void
}

const contexts: FakeContext[] = []

function installFakeAudio() {
  class FakeAudioContext {
    state = 'running'
    destination = {}
    started = 0
    gains: FakeGain[] = []
    pending: FakeContext['pending'] = []
    release = (failedUrl?: string) => {
      for (const { url, resolve } of this.pending)
        resolve({
          ok: url !== failedUrl,
          status: url === failedUrl ? 404 : 200,
          arrayBuffer: async () => new ArrayBuffer(4),
        })
    }
    createGain() {
      const gain = { gain: { value: 0 }, connect() {} }
      this.gains.push(gain)
      return gain
    }
    createBufferSource() {
      return {
        buffer: null,
        loop: false,
        playbackRate: { value: 1 },
        connect() {},
        start: () => {
          this.started++
        },
      }
    }
    createBiquadFilter() {
      return { type: '', frequency: { value: 0 }, connect() {} }
    }
    decodeAudioData = async (data: ArrayBuffer) => data
    resume = async () => {}
    close = async () => {}
    constructor() {
      contexts.push(this)
    }
  }
  vi.stubGlobal('AudioContext', FakeAudioContext)
  vi.stubGlobal('fetch', (url: string) => {
    const ctx = contexts[contexts.length - 1]
    return new Promise((resolve) => ctx.pending.push({ url, resolve }))
  })
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('rainAmbienceManager', () => {
  beforeEach(() => {
    contexts.length = 0
    lightningEnabled.set(true)
    vi.mocked(getSfxMultiplier).mockReturnValue(1)
    vi.spyOn(Math, 'random').mockReturnValue(0)
    installFakeAudio()
  })
  afterEach(() => {
    stopRainAmbience()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('does not initialize audio in dry weather', () => {
    updateRainAmbience(0, false, 1)
    expect(contexts).toHaveLength(0)
  })

  it('starts both loops on the context the load began for', async () => {
    updateRainAmbience(1, false, 0.016)
    expect(contexts).toHaveLength(1)
    contexts[0].release()
    await flush()
    expect(contexts[0].started).toBe(2)
    expect(contexts[0].pending.map(({ url }) => url)).toEqual([
      '/sounds/rain-drops-loop.ogg',
      '/sounds/rain-loop.ogg',
      '/sounds/thunder-distant.ogg',
    ])
  })

  it.each([0.05, 0.2, 0.45])('plays only droplets at intensity %s', (rain) => {
    updateRainAmbience(rain, false, 1)
    const [drops, heavy] = contexts[0].gains
    expect(drops.gain.value).toBeCloseTo(rain * 0.5)
    expect(heavy.gain.value).toBe(0)
  })

  it('blends toward the original recording as rain strengthens', () => {
    updateRainAmbience(0.7, false, 1)
    const [drops, heavy] = contexts[0].gains
    expect(drops.gain.value).toBeGreaterThan(0)
    expect(heavy.gain.value).toBeGreaterThan(0)
    expect(drops.gain.value + heavy.gain.value).toBeCloseTo(0.7 * 0.5)

    updateRainAmbience(1, false, 1)
    expect(drops.gain.value).toBe(0)
    expect(heavy.gain.value).toBeCloseTo(0.5)
  })

  it('fades between layers and back to silence', () => {
    updateRainAmbience(1, false, 1)
    updateRainAmbience(0.2, false, 0.1)
    const [drops, heavy] = contexts[0].gains
    expect(drops.gain.value).toBeGreaterThan(0)
    expect(drops.gain.value).toBeLessThan(0.1)
    expect(heavy.gain.value).toBeGreaterThan(0)
    expect(heavy.gain.value).toBeLessThan(0.5)

    for (let frame = 0; frame < 600; frame++)
      updateRainAmbience(0, false, 1 / 60)
    expect(drops.gain.value).toBeLessThan(0.00001)
    expect(heavy.gain.value).toBeLessThan(0.00001)
  })

  it('attenuates both layers indoors without changing their balance', () => {
    updateRainAmbience(0.7, false, 1)
    const outdoors = contexts[0].gains.map(({ gain }) => gain.value)
    updateRainAmbience(0.7, true, 1)
    contexts[0].gains.forEach(({ gain }, i) => {
      expect(gain.value).toBeCloseTo(outdoors[i] * 0.35)
    })
  })

  it('applies SFX volume and immediate mute to both layers', () => {
    updateRainAmbience(0.7, false, 1)
    const fullVolume = contexts[0].gains.map(({ gain }) => gain.value)
    vi.mocked(getSfxMultiplier).mockReturnValue(0.4)
    updateRainAmbience(0.7, false, 0.016)
    contexts[0].gains.forEach(({ gain }, i) => {
      expect(gain.value).toBeCloseTo(fullVolume[i] * 0.4)
    })

    vi.mocked(getSfxMultiplier).mockReturnValue(0)
    updateRainAmbience(0.7, false, 0.016)
    expect(contexts[0].gains.map(({ gain }) => gain.value)).toEqual([0, 0])
  })

  it.each([
    ['/sounds/rain-drops-loop.ogg', 1],
    ['/sounds/rain-loop.ogg', 1],
    ['/sounds/thunder-distant.ogg', 2],
  ])('keeps other layers playing when %s fails', async (url, started) => {
    updateRainAmbience(0.7, false, 0.016)
    contexts[0].release(url)
    await flush()
    expect(contexts[0].started).toBe(started)
  })

  it.each([0, 0.5, 0.999])(
    'flashes before thunder with random timing %s',
    async (random) => {
      vi.mocked(Math.random).mockReturnValue(random)
      updateRainAmbience(1, false, 0.016)
      contexts[0].release()
      await flush()

      updateRainAmbience(1, false, 8 + random * 22)
      expect(getLightningStrength()).toBe(0)
      expect(contexts[0].started).toBe(2)

      updateRainAmbience(1, false, 8 + random * 22)
      expect(getLightningStrength()).toBe(1)
      expect(contexts[0].started).toBe(2)

      updateRainAmbience(1, false, 2 + random * 3 - 0.1)
      expect(contexts[0].started).toBe(2)
      expect(getLightningStrength()).toBe(0)

      updateRainAmbience(1, false, 0.101)
      expect(contexts[0].started).toBe(3)
      expect(getLightningStrength()).toBe(0)
    }
  )

  it('starts a new flash after the storm interval', async () => {
    updateRainAmbience(1, false, 0.016)
    contexts[0].release()
    await flush()
    updateRainAmbience(1, false, 16)
    expect(getLightningStrength()).toBe(1)
    updateRainAmbience(1, false, 2)
    updateRainAmbience(1, false, 47.9)
    expect(getLightningStrength()).toBe(0)

    updateRainAmbience(1, false, 0.11)
    expect(getLightningStrength()).toBeGreaterThan(0)
    expect(contexts[0].started).toBe(3)
    updateRainAmbience(1, false, 2)
    expect(contexts[0].started).toBe(4)
  })

  it('keeps lightning visible while SFX are muted', async () => {
    vi.mocked(getSfxMultiplier).mockReturnValue(0)
    updateRainAmbience(1, false, 0.016)
    contexts[0].release()
    await flush()
    updateRainAmbience(1, false, 16)
    expect(getLightningStrength()).toBeGreaterThan(0)
    updateRainAmbience(1, false, 2)
    expect(contexts[0].started).toBe(2)
  })

  it('attenuates lightning indoors', () => {
    updateRainAmbience(1, true, 0.016)
    updateRainAmbience(1, true, 16)
    expect(getLightningStrength()).toBeCloseTo(0.35)
  })

  it('picks a sky direction per strike and keeps it stable during the fade', () => {
    updateRainAmbience(1, false, 0.016)
    updateRainAmbience(1, false, 16)
    const first = { ...getLightningDirection() }
    expect(Math.hypot(first.x, first.y, first.z)).toBeCloseTo(1)
    expect(first.y).toBeGreaterThan(0)

    vi.mocked(Math.random).mockReturnValue(0.5)
    updateRainAmbience(1, false, 0.3)
    expect(getLightningDirection()).toEqual(first)
    updateRainAmbience(1, false, 49.7)
    const next = getLightningDirection()
    expect(next).not.toEqual(first)
    expect(Math.hypot(next.x, next.y, next.z)).toBeCloseTo(1)
    expect(next.y).toBeGreaterThan(0)
    expect(getLightningStrength()).toBe(1)
  })

  it('holds the light briefly and fades out before thunder', async () => {
    updateRainAmbience(1, false, 0.016)
    contexts[0].release()
    await flush()
    updateRainAmbience(1, false, 16)
    expect(getLightningStrength()).toBe(1)
    updateRainAmbience(1, false, 0.05)
    expect(getLightningStrength()).toBe(1)
    updateRainAmbience(1, false, 0.225)
    expect(getLightningStrength()).toBeCloseTo(0.25)
    updateRainAmbience(1, false, 0.225)
    expect(getLightningStrength()).toBeCloseTo(0)
    expect(contexts[0].started).toBe(2)
  })

  it('keeps rain and thunder playing with lightning disabled', async () => {
    lightningEnabled.set(false)
    updateRainAmbience(1, false, 0.016)
    contexts[0].release()
    await flush()
    updateRainAmbience(1, false, 16)
    expect(getLightningStrength()).toBe(0)
    expect(contexts[0].started).toBe(2)
    expect(contexts[0].gains[1].gain.value).toBeCloseTo(0.5)

    updateRainAmbience(1, false, 2)
    expect(contexts[0].started).toBe(3)
    stopRainAmbience()
    updateRainAmbience(1, false, 0.016)
    updateRainAmbience(1, false, 16)
    expect(get(lightningEnabled)).toBe(false)
    expect(getLightningStrength()).toBe(0)
  })

  it('clears an active flash immediately and resumes at the next strike', async () => {
    updateRainAmbience(1, false, 0.016)
    contexts[0].release()
    await flush()
    updateRainAmbience(1, false, 16)
    expect(getLightningStrength()).toBeGreaterThan(0)

    lightningEnabled.set(false)
    expect(getLightningStrength()).toBe(0)
    lightningEnabled.set(true)
    expect(getLightningStrength()).toBe(0)

    updateRainAmbience(1, false, 2)
    expect(contexts[0].started).toBe(3)
    expect(getLightningStrength()).toBe(0)
    updateRainAmbience(1, false, 48)
    expect(getLightningStrength()).toBeGreaterThan(0)
  })

  it.each([0, 0.35])(
    'cancels pending thunder when rain drops to %s',
    async (rain) => {
      updateRainAmbience(1, false, 0.016)
      contexts[0].release()
      await flush()
      updateRainAmbience(1, false, 16)
      expect(getLightningStrength()).toBeGreaterThan(0)

      updateRainAmbience(rain, false, 0.016)
      expect(getLightningStrength()).toBe(0)
      updateRainAmbience(1, false, 0.016)
      updateRainAmbience(1, false, 2)
      expect(contexts[0].started).toBe(2)
      expect(getLightningStrength()).toBe(0)
    }
  )

  it('clears lightning and pending thunder on restart', async () => {
    updateRainAmbience(1, false, 0.016)
    updateRainAmbience(1, false, 16)
    expect(getLightningStrength()).toBeGreaterThan(0)
    stopRainAmbience()
    expect(getLightningStrength()).toBe(0)

    updateRainAmbience(1, false, 0.016)
    contexts[1].release()
    await flush()
    updateRainAmbience(1, false, 2)
    expect(contexts[1].started).toBe(2)
    expect(getLightningStrength()).toBe(0)
  })

  it('drops a load whose context was stopped before the files arrived', async () => {
    updateRainAmbience(1, false, 0.016)
    stopRainAmbience()
    updateRainAmbience(1, false, 0.016)
    expect(contexts).toHaveLength(2)

    contexts[0].release()
    contexts[1].release()
    await flush()

    expect(contexts[0].started).toBe(0)
    expect(contexts[1].started).toBe(2)
  })

  it('clears the previous storm mix on restart', () => {
    updateRainAmbience(1, false, 1)
    stopRainAmbience()
    updateRainAmbience(0.2, false, 0.016)
    const [drops, heavy] = contexts[1].gains
    expect(drops.gain.value).toBeGreaterThan(0)
    expect(heavy.gain.value).toBe(0)
  })
})
