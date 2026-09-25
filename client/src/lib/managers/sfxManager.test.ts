import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  cancelPendingFishingSounds,
  playDungeonDripSound,
  playFishingSound,
  preloadDungeonSounds,
  sfxMuted,
  sfxVolume,
  stopDungeonDripSounds,
} from './sfxManager'

// Audio pools persist across tests; playback counts do not.
let plays = new Map<string, number>()
let playbacks: { audio: FakeAudio; volume: number; rate: number }[] = []
const loadedUrls: string[] = []

class FakeAudio {
  preload = ''
  volume = 1
  currentTime = 0
  playbackRate = 1
  paused = true
  constructor(public url: string) {}
  load() {
    loadedUrls.push(this.url)
  }
  play() {
    this.paused = false
    playbacks.push({
      audio: this,
      volume: this.volume,
      rate: this.playbackRate,
    })
    plays.set(this.url, (plays.get(this.url) ?? 0) + 1)
    return Promise.resolve()
  }
  pause() {
    this.paused = true
  }
}

function playedCount(urlPart: string) {
  let sum = 0
  for (const [url, count] of plays) {
    if (url.includes(urlPart)) sum += count
  }
  return sum
}

describe('delayed fishing sounds', () => {
  beforeEach(() => {
    plays = new Map()
    vi.useFakeTimers()
    vi.stubGlobal('Audio', FakeAudio)
    vi.stubGlobal('window', {
      setTimeout: setTimeout.bind(globalThis),
      clearTimeout: clearTimeout.bind(globalThis),
    })
  })

  afterEach(() => {
    cancelPendingFishingSounds()
    vi.unstubAllGlobals()
    vi.useRealTimers()
  })

  it('a delayed splash plays once the flight time elapses', () => {
    playFishingSound('splash', 1400)

    expect(playedCount('splash')).toBe(0)
    vi.advanceTimersByTime(1400)
    expect(playedCount('splash')).toBe(1)
  })

  it('an aborted cast cancels the pending splash — no splash after the line is back in', () => {
    playFishingSound('splash', 1400)

    cancelPendingFishingSounds()

    vi.advanceTimersByTime(5000)
    expect(playedCount('splash')).toBe(0)
  })

  it('cancels every pending timer, whoosh included, when aborted inside the swing delay', () => {
    playFishingSound('cast', 200)
    playFishingSound('splash', 1400)

    cancelPendingFishingSounds()

    vi.advanceTimersByTime(5000)
    expect(playedCount('cast')).toBe(0)
    expect(playedCount('splash')).toBe(0)
  })

  it('cancel does not touch sounds that already played', () => {
    playFishingSound('splash', 1400)
    vi.advanceTimersByTime(1400)

    cancelPendingFishingSounds()

    expect(playedCount('splash')).toBe(1)
  })

  it('immediate sounds are unaffected by a pending-timer cancel', () => {
    cancelPendingFishingSounds()
    playFishingSound('plop')

    expect(playedCount('plop')).toBe(1)
  })
})

describe('dungeon drip audio', () => {
  beforeEach(() => {
    playbacks = []
    vi.stubGlobal('Audio', FakeAudio)
    sfxVolume.set(0.5)
    sfxMuted.set(false)
  })

  afterEach(() => {
    stopDungeonDripSounds()
    sfxVolume.set(0.5)
    sfxMuted.set(false)
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('preloads both variants without playing them or recreating their pools', () => {
    preloadDungeonSounds()
    for (const url of [
      '/sounds/dungeon-drip.ogg',
      '/sounds/dungeon-drip-2.ogg',
    ]) {
      expect(loadedUrls.filter((loaded) => loaded === url)).toHaveLength(4)
    }
    expect(playbacks).toHaveLength(0)

    const loadCount = loadedUrls.length
    preloadDungeonSounds()
    expect(loadedUrls).toHaveLength(loadCount)
  })

  it('randomly chooses either recording on each impact, including repeats', () => {
    vi.spyOn(Math, 'random')
      .mockReturnValueOnce(0)
      .mockReturnValueOnce(0.5)
      .mockReturnValueOnce(0.9999)

    for (let i = 0; i < 3; i++) playDungeonDripSound(0, 1.5)

    expect(playbacks.map(({ audio }) => audio.url)).toEqual([
      '/sounds/dungeon-drip.ogg',
      '/sounds/dungeon-drip-2.ogg',
      '/sounds/dungeon-drip-2.ogg',
    ])
    expect(playbacks.map(({ volume }) => volume)).toEqual([0.35, 0.35, 0.35])
  })

  it('fades with distance and skips drops outside hearing range', () => {
    playDungeonDripSound(0, 1.5)
    playDungeonDripSound(5, 1.5)
    playDungeonDripSound(10, 1.5)
    playDungeonDripSound(25, 1.5)
    expect(playbacks).toHaveLength(2)
    expect(playbacks[0].volume).toBeCloseTo(0.35)
    expect(playbacks[1].volume).toBeCloseTo(playbacks[0].volume / 4)
  })

  it('honors the SFX volume and mute settings', () => {
    sfxMuted.set(true)
    playDungeonDripSound(0, 1)
    sfxMuted.set(false)
    sfxVolume.set(0)
    playDungeonDripSound(0, 1)
    expect(playbacks).toHaveLength(0)
    sfxVolume.set(0.2)
    playDungeonDripSound(0, 1)
    expect(playbacks[0].volume).toBeCloseTo(0.14)
  })

  it('varies the pitch between puddles', () => {
    playDungeonDripSound(0, 1.1)
    playDungeonDripSound(0, 1.9)
    expect(playbacks[0].rate).not.toBe(playbacks[1].rate)
  })

  it('stops both drip variants on floor exit without stopping other effects', () => {
    vi.spyOn(Math, 'random').mockReturnValueOnce(0).mockReturnValueOnce(0.9)
    playDungeonDripSound(0, 1)
    playDungeonDripSound(0, 1)
    playFishingSound('plop')
    stopDungeonDripSounds()
    expect(playbacks[0].audio.paused).toBe(true)
    expect(playbacks[1].audio.paused).toBe(true)
    expect(playbacks[2].audio.paused).toBe(false)
  })
})
