import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('./sfxManager', () => ({
  getSfxMultiplier: () => 1,
}))

import { stopRainAmbience, updateRainAmbience } from './rainAmbienceManager'

interface FakeContext {
  started: number
  pending: ((value: unknown) => void)[]
  release: () => void
}

const contexts: FakeContext[] = []

/** Each context's rain fetch stays pending until the test releases it. */
function installFakeAudio() {
  class FakeAudioContext {
    state = 'running'
    destination = {}
    started = 0
    pending: ((value: unknown) => void)[] = []
    release = () => {
      for (const resolve of this.pending)
        resolve({ arrayBuffer: async () => new ArrayBuffer(4) })
    }
    createGain() {
      return { gain: { value: 0 }, connect() {} }
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
  vi.stubGlobal('fetch', () => {
    const ctx = contexts[contexts.length - 1]
    return new Promise((resolve) => ctx.pending.push(resolve))
  })
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('rainAmbienceManager', () => {
  beforeEach(() => {
    contexts.length = 0
    installFakeAudio()
  })
  afterEach(() => {
    stopRainAmbience()
    vi.unstubAllGlobals()
  })

  it('starts one loop on the context the load began for', async () => {
    updateRainAmbience(1, false, 0.016)
    expect(contexts).toHaveLength(1)
    contexts[0].release()
    await flush()
    expect(contexts[0].started).toBe(1)
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
    expect(contexts[1].started).toBe(1)
  })
})
