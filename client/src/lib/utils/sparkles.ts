import * as THREE from 'three'

export interface SparkSeed {
  ox: number
  oy: number
  oz: number
  dx: number
  dz: number
  phase: number
  twinkle: number
}

export interface SparkLook {
  /** Seconds for one rise. */
  lifetime: number
  /** Metres climbed over a lifetime. */
  rise: number
  /** Sprite world size at full twinkle. */
  size: number
}

export interface Spark {
  x: number
  y: number
  z: number
  opacity: number
  scale: number
}

const DRIFT = 0.06
const TWINKLE_FREQ = 9

let texture: THREE.CanvasTexture | null = null

/** Soft radial glow shared by every spark sprite. */
export function sparkTexture(): THREE.CanvasTexture {
  if (texture) return texture
  const canvas = document.createElement('canvas')
  canvas.width = 64
  canvas.height = 64
  const ctx = canvas.getContext('2d')!
  const glow = ctx.createRadialGradient(32, 32, 0, 32, 32, 32)
  glow.addColorStop(0, 'rgba(255,255,255,1)')
  glow.addColorStop(0.25, 'rgba(255,241,189,0.85)')
  glow.addColorStop(1, 'rgba(255,221,130,0)')
  ctx.fillStyle = glow
  ctx.fillRect(0, 0, 64, 64)
  texture = new THREE.CanvasTexture(canvas)
  return texture
}

/** A spark rising from a local point of a model yawed by `yaw` radians. */
export function sparkSeed(
  rand: () => number,
  [lx, ly, lz]: [number, number, number],
  yaw: number
): SparkSeed {
  const cos = Math.cos(yaw)
  const sin = Math.sin(yaw)
  const drift = rand() * Math.PI * 2
  return {
    ox: lx * cos + lz * sin,
    oy: ly,
    oz: -lx * sin + lz * cos,
    dx: Math.cos(drift) * DRIFT,
    dz: Math.sin(drift) * DRIFT,
    phase: rand(),
    twinkle: rand() * Math.PI * 2,
  }
}

/** Each spark at `seconds`: rising, fading in then out, and flickering. */
export function sparksAt(
  seeds: SparkSeed[],
  seconds: number,
  look: SparkLook
): Spark[] {
  return seeds.map((s) => {
    const phase = (seconds / look.lifetime + s.phase) % 1
    const fade = Math.sin(phase * Math.PI)
    const twinkle = 0.5 + 0.5 * Math.sin(seconds * TWINKLE_FREQ + s.twinkle)
    return {
      x: s.ox + s.dx * phase,
      y: s.oy + phase * look.rise,
      z: s.oz + s.dz * phase,
      opacity: fade * (0.35 + 0.65 * twinkle),
      scale: look.size * (0.55 + 0.45 * twinkle) * (0.5 + 0.5 * fade),
    }
  })
}
