import { get } from 'svelte/store'
import { lightningEnabled } from '../stores/effectSettings'
import { getSfxMultiplier } from './sfxManager'

const DROPS_URL = '/sounds/rain-drops-loop.ogg'
const RAIN_URL = '/sounds/rain-loop.ogg'
const THUNDER_URL = '/sounds/thunder-distant.ogg'
const RAIN_VOLUME = 0.5
const HEAVY_RAIN_START = 0.45
const INDOOR_FACTOR = 0.35
const SMOOTH_RATE = 1.2
const THUNDER_MIN_INTENSITY = 0.35
const LIGHTNING_FIRST_DELAY = [16, 60]
const LIGHTNING_INTERVAL = [50, 140]
const THUNDER_DELAY = [2, 5]
const LIGHTNING_HOLD_SECONDS = 0.05
const LIGHTNING_FADE_SECONDS = 0.45

let lightningStrength = 0
let lightningAge = 0
const lightningDirection = { x: 0, y: 1, z: 0 }

lightningEnabled.subscribe((enabled) => {
  if (!enabled) lightningStrength = 0
})

export function getLightningStrength(): number {
  const fade = Math.max(
    0,
    Math.min(
      1,
      (lightningAge - LIGHTNING_HOLD_SECONDS) / LIGHTNING_FADE_SECONDS
    )
  )
  return lightningStrength * (1 - fade) ** 2
}

export function getLightningDirection(): Readonly<typeof lightningDirection> {
  return lightningDirection
}

let audioCtx: AudioContext | null = null
let dropsGain: GainNode | null = null
let rainGain: GainNode | null = null
let thunderBuffer: AudioBuffer | null = null
let current = 0
let currentDrops = 0
let lightningIn: number | null = null
let thunderIn: number | null = null
let resuming = false

function randBetween([lo, hi]: number[]): number {
  return lo + Math.random() * (hi - lo)
}

function init() {
  const ctx = new AudioContext()
  const drops = ctx.createGain()
  drops.gain.value = 0
  drops.connect(ctx.destination)
  const gain = ctx.createGain()
  gain.gain.value = 0
  gain.connect(ctx.destination)
  audioCtx = ctx
  dropsGain = drops
  rainGain = gain

  const loadBuffer = async (url: string) => {
    const response = await fetch(url)
    if (!response.ok)
      throw new Error(`Failed to load ${url}: ${response.status}`)
    const data = await response.arrayBuffer()
    if (audioCtx !== ctx) return null
    const buffer = await ctx.decodeAudioData(data)
    return audioCtx === ctx ? buffer : null
  }
  const loadLoop = async (url: string, output: GainNode) => {
    const buffer = await loadBuffer(url)
    if (!buffer || audioCtx !== ctx) return
    const src = ctx.createBufferSource()
    src.buffer = buffer
    src.loop = true
    src.connect(output)
    src.start()
  }
  void loadLoop(DROPS_URL, drops).catch(() => {})
  void loadLoop(RAIN_URL, gain).catch(() => {})
  void loadBuffer(THUNDER_URL)
    .then((buffer) => {
      if (audioCtx === ctx) thunderBuffer = buffer
    })
    .catch(() => {})
}

function playThunder(indoor: boolean) {
  if (!audioCtx || !thunderBuffer) return
  const volume = getSfxMultiplier()
  if (volume <= 0) return

  const src = audioCtx.createBufferSource()
  src.buffer = thunderBuffer
  src.playbackRate.value = 0.8 + Math.random() * 0.3

  const muffle = audioCtx.createBiquadFilter()
  muffle.type = 'lowpass'
  muffle.frequency.value = 500 + Math.random() * 900

  const gain = audioCtx.createGain()
  gain.gain.value =
    (0.25 + Math.random() * 0.3) * volume * (indoor ? INDOOR_FACTOR : 1)

  src.connect(muffle)
  muffle.connect(gain)
  gain.connect(audioCtx.destination)
  src.start()
}

export function updateRainAmbience(
  intensity: number,
  indoor: boolean,
  dtSec: number
) {
  lightningAge += dtSec
  const rain = Math.max(0, Math.min(1, intensity))
  const target = rain * (indoor ? INDOOR_FACTOR : 1)
  if (!audioCtx) {
    if (target <= 0.01) return
    init()
  }
  if (!audioCtx || !dropsGain || !rainGain) return
  if (audioCtx.state === 'suspended' && !resuming) {
    resuming = true
    audioCtx.resume().finally(() => (resuming = false))
  }

  const k = Math.min(1, SMOOTH_RATE * dtSec)
  const heavy = Math.max(0, (rain - HEAVY_RAIN_START) / (1 - HEAVY_RAIN_START))
  const blend = heavy * heavy * (3 - 2 * heavy)
  currentDrops += (target * (1 - blend) - currentDrops) * k
  current += (target * blend - current) * k
  const volume = getSfxMultiplier() * RAIN_VOLUME
  dropsGain.gain.value = currentDrops * volume
  rainGain.gain.value = current * volume

  if (!(intensity > THUNDER_MIN_INTENSITY)) {
    lightningIn = null
    thunderIn = null
    lightningStrength = 0
    return
  }

  if (thunderIn !== null) {
    thunderIn -= dtSec
    if (thunderIn <= 0) {
      playThunder(indoor)
      thunderIn = null
      lightningStrength = 0
    }
  }
  if (lightningIn === null) {
    lightningIn = randBetween(LIGHTNING_FIRST_DELAY)
    return
  }

  lightningIn -= dtSec
  if (lightningIn > 0) return

  if (get(lightningEnabled)) {
    lightningStrength = indoor ? INDOOR_FACTOR : 1
    lightningAge = 0
    const azimuth = Math.random() * Math.PI * 2
    const elevation = randBetween([Math.PI / 6, (Math.PI * 5) / 12])
    const horizontal = Math.cos(elevation)
    lightningDirection.x = Math.cos(azimuth) * horizontal
    lightningDirection.y = Math.sin(elevation)
    lightningDirection.z = Math.sin(azimuth) * horizontal
  }
  thunderIn = randBetween(THUNDER_DELAY)
  lightningIn = randBetween(LIGHTNING_INTERVAL)
}

export function stopRainAmbience() {
  void audioCtx?.close()
  audioCtx = null
  dropsGain = null
  rainGain = null
  thunderBuffer = null
  current = 0
  currentDrops = 0
  lightningIn = null
  thunderIn = null
  lightningStrength = 0
  lightningAge = 0
  resuming = false
}
