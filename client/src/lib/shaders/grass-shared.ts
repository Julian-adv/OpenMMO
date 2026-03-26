/**
 * Shared TSL utilities for grass shaders (compute + material paths).
 */
import {
  vec2,
  vec3,
  float,
  sin,
  cos,
  mix,
  smoothstep,
  instanceIndex,
  hash,
} from 'three/tsl'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type N = any // TSL node -- broad type for shader node expressions

export const GUST_WAVE_COUNT = 3

// ── Per-instance hash helper ─────────────────────────────
// Replaces the repeated `hash(vec2(instanceIndex.toFloat().mul(s), float(d)))` pattern.
export function iHash(scale: number, seed: number): N {
  return hash(vec2(instanceIndex.toFloat().mul(scale), float(seed)))
}

// ── Gerstner wave gusts ──────────────────────────────────
// Builds TSL nodes for the gust scalar from overlapping Gerstner waves.
// JS-time loop (not TSL Fn) because it iterates uniform arrays.
export function computeGerstnerGust(
  posX: N,
  posZ: N,
  uTime: N,
  uWindDir: N,
  uWaveAngles: N[],
  uWaveAmps: N[],
  uWaveParams: N[],
  uGustStrength: N
): N {
  let gust: N = float(0)
  for (let wi = 0; wi < GUST_WAVE_COUNT; wi++) {
    const wp = uWaveParams[wi]
    const wFreq = wp.x
    const wSpeed = wp.y
    const wAmp = wp.z
    const wQ = wp.w

    const wAngle = uWaveAngles[wi]
    const cOff = cos(wAngle)
    const sOff = sin(wAngle)
    const wDirX = uWindDir.x.mul(cOff).sub(uWindDir.y.mul(sOff))
    const wDirZ = uWindDir.x.mul(sOff).add(uWindDir.y.mul(cOff))

    const spatial = posX.mul(wDirX).add(posZ.mul(wDirZ))
    const perp = posX.mul(wDirZ.negate()).add(posZ.mul(wDirX))
    const warp = sin(perp.mul(0.15)).mul(2.5)

    const phase = spatial.mul(wFreq).add(warp).sub(uTime.mul(wSpeed))
    const gerstnerPhase = phase.add(wQ.mul(sin(phase)))
    const waveVal = cos(gerstnerPhase).add(1).mul(0.5)
    gust = gust.add(waveVal.mul(wAmp).mul(uWaveAmps[wi]))
  }
  return gust.mul(float(0.15).add(uGustStrength.mul(0.85)))
}

// ── Color variation ──────────────────────────────────────
// Gradient, root AO, per-instance brightness and hue shift.
// Returns nodes ready to multiply together for the final color.
export function computeGrassColor(
  baseColor: N,
  tipColor: N,
  uvY: N
): { gradientColor: N; rootAO: N; brightness: N; hueShift: N } {
  const gradientColor = mix(
    baseColor,
    tipColor,
    smoothstep(float(0), float(0.8), uvY)
  )

  const rootAO = mix(
    float(0.45),
    float(1.0),
    smoothstep(float(0), float(0.35), uvY)
  )

  const brightnessHash = iHash(0.37, 1.7)
  const brightness = float(0.85).add(brightnessHash.mul(0.3))

  const hueHash = iHash(0.73, 3.1)
  const hueShift = vec3(
    float(1.0).add(hueHash.sub(0.5).mul(0.15)),
    float(1.0),
    float(1.0).add(hueHash.sub(0.5).mul(-0.1))
  )

  return { gradientColor, rootAO, brightness, hueShift }
}

// ── Width scale ──────────────────────────────────────────
export function computeWidthScale(wsMin: number, wsExt: number): N {
  return float(wsMin).add(iHash(0.53, 2.3).mul(wsExt))
}
