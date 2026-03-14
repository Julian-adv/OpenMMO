import {
  Fn,
  vec2,
  vec4,
  float,
  fract,
  sin,
  sqrt,
  dot,
  normalize,
  floor,
  mix,
  texture,
} from 'three/tsl'

import { PI } from './gerstner'

// ─── Hash-based value noise (shared by water material + wetness compute) ─────

export const hash = /* #__PURE__ */ Fn(
  ([p_immutable]: [ReturnType<typeof vec2>]) => {
    const p = vec2(p_immutable)
    return fract(sin(dot(p, vec2(127.1, 311.7))).mul(43758.5453))
  }
)

export const valueNoise = /* #__PURE__ */ Fn(
  ([p_immutable]: [ReturnType<typeof vec2>]) => {
    const p = vec2(p_immutable)
    const i = floor(p)
    const fv = fract(p)
    const f = fv.mul(fv).mul(float(3).sub(fv.mul(2))) // smoothstep interpolation

    const a = hash(i)
    const b = hash(i.add(vec2(1.0, 0.0)))
    const c = hash(i.add(vec2(0.0, 1.0)))
    const d = hash(i.add(vec2(1.0, 1.0)))

    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y)
  }
)

// ─── Normal Map Noise (3-sample, wave-aligned) ──────────
// Samples a normal map texture at 3 UVs derived from wave directions/speeds.
export const sampleNormalNoise = /* #__PURE__ */ Fn(
  ([
    worldXZ_immutable,
    normalMapTex,
    time_immutable,
    waveA_immutable,
    waveB_immutable,
    waveC_immutable,
  ]: [
    ReturnType<typeof vec2>,
    ReturnType<typeof texture>,
    ReturnType<typeof float>,
    ReturnType<typeof vec4>,
    ReturnType<typeof vec4>,
    ReturnType<typeof vec4>,
  ]) => {
    const worldXZ = vec2(worldXZ_immutable)
    const time = float(time_immutable)
    const t = time.mul(0.06)

    const wA = vec4(waveA_immutable)
    const wB = vec4(waveB_immutable)
    const wC = vec4(waveC_immutable)

    const dirA = normalize(wA.xy)
    const dirB = normalize(wB.xy)
    const dirC = normalize(wC.xy)

    const cA = sqrt(float(9.8).div(PI.mul(2).div(wA.w))).mul(0.1)
    const cB = sqrt(float(9.8).div(PI.mul(2).div(wB.w))).mul(0.1)
    const cC = sqrt(float(9.8).div(PI.mul(2).div(wC.w))).mul(0.1)

    const uv0 = worldXZ.div(wA.w.mul(0.5)).add(dirA.mul(cA.mul(t).mul(0.3)))
    const uv1 = worldXZ.div(wB.w.mul(0.5)).add(dirB.mul(cB.mul(t).mul(0.2)))
    const uv2 = worldXZ.div(wC.w.mul(0.5)).add(dirC.mul(cC.mul(t).mul(0.1)))

    return normalMapTex
      .sample(uv0)
      .add(normalMapTex.sample(uv1))
      .add(normalMapTex.sample(uv2))
      .mul(0.5)
      .sub(1.0)
  }
)
