import { get } from 'svelte/store'
import { sfxMuted, sfxVolume } from './sfxManager'

// Looping ambience + scheduled one-shots need WebAudio (gain automation,
// seamless loop), so this runs beside sfxManager but obeys the same
// SFX volume/mute settings.
const RAIN_URL = '/sounds/rain-loop.ogg'
const THUNDER_URL = '/sounds/thunder-distant.ogg'
const RAIN_BASE_GAIN = 1.0
const INDOOR_FACTOR = 0.35
const SMOOTH_RATE = 1.2
const THUNDER_MIN_INTENSITY = 0.35
const THUNDER_FIRST_DELAY = [8, 30]
const THUNDER_INTERVAL = [25, 70]

let audioCtx: AudioContext | null = null
let rainGain: GainNode | null = null
let thunderBuffer: AudioBuffer | null = null
let current = 0
let thunderIn: number | null = null

function randBetween([lo, hi]: number[]): number {
  return lo + Math.random() * (hi - lo)
}

function init() {
  audioCtx = new AudioContext()
  rainGain = audioCtx.createGain()
  rainGain.gain.value = 0
  rainGain.connect(audioCtx.destination)

  const load = async () => {
    const [rainData, thunderData] = await Promise.all([
      fetch(RAIN_URL).then((r) => r.arrayBuffer()),
      fetch(THUNDER_URL).then((r) => r.arrayBuffer()),
    ])
    const rainBuffer = await audioCtx!.decodeAudioData(rainData)
    thunderBuffer = await audioCtx!.decodeAudioData(thunderData)
    const src = audioCtx!.createBufferSource()
    src.buffer = rainBuffer
    src.loop = true
    src.connect(rainGain!)
    src.start()
  }
  // Missing audio files must never break the game loop
  void load().catch(() => {})
}

/** Soft distant rumble: random pitch, muffling, and volume every time. */
function playThunder(indoor: boolean) {
  if (!audioCtx || !thunderBuffer) return
  const volume = get(sfxMuted) ? 0 : get(sfxVolume)
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

/** Called every frame; smooths toward the target so rain fades in/out. */
export function updateRainAmbience(
  intensity: number,
  indoor: boolean,
  dtSec: number
) {
  const target = intensity * (indoor ? INDOOR_FACTOR : 1)
  if (!audioCtx) {
    if (target <= 0.01) return
    init()
  }
  if (!audioCtx || !rainGain) return
  if (audioCtx.state === 'suspended') void audioCtx.resume()

  const k = Math.min(1, SMOOTH_RATE * dtSec)
  current += (target - current) * k

  const volume = get(sfxMuted) ? 0 : get(sfxVolume)
  rainGain.gain.value = current * RAIN_BASE_GAIN * volume

  if (intensity > THUNDER_MIN_INTENSITY) {
    if (thunderIn === null) {
      thunderIn = randBetween(THUNDER_FIRST_DELAY)
    } else {
      thunderIn -= dtSec
      if (thunderIn <= 0) {
        playThunder(indoor)
        thunderIn = randBetween(THUNDER_INTERVAL)
      }
    }
  } else {
    thunderIn = null
  }
}

export function stopRainAmbience() {
  if (!audioCtx) return
  void audioCtx.close()
  audioCtx = null
  rainGain = null
  thunderBuffer = null
  current = 0
  thunderIn = null
}
