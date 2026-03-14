import {
  Fn,
  vec3,
  vec4,
  float,
  sin,
  cos,
  sqrt,
  dot,
  normalize,
} from 'three/tsl'

export const PI = float(Math.PI)

// ─── Gerstner Wave Displacement ─────────────────────────
// wave = vec4(dirX, dirY, steepness, wavelength)
// Returns vertical-only displacement to avoid tile boundary tearing.
export const gerstnerWave = /* #__PURE__ */ Fn(
  ([wave_immutable, p_immutable, time_immutable]: [
    ReturnType<typeof vec4>,
    ReturnType<typeof vec3>,
    ReturnType<typeof float>,
  ]) => {
    const wave = vec4(wave_immutable)
    const p = vec3(p_immutable)
    const time = float(time_immutable)
    const k = PI.mul(2).div(wave.w)
    const c = sqrt(float(9.8).div(k))
    const d = normalize(wave.xy)
    const f = k.mul(dot(d, p.xz).sub(c.mul(time).mul(0.1)))
    const a = wave.z.div(k)
    return vec3(0, a.mul(sin(f)), 0)
  }
)

// ─── Gerstner Wave Normal Contribution ──────────────────
// Returns vec4(tx, ty, bz, by) partial derivatives for accumulation.
// Reconstruct normal: normalize(vec3(-ty, tx*bz, -by))
export const gerstnerNormal = /* #__PURE__ */ Fn(
  ([wave_immutable, p_immutable, time_immutable]: [
    ReturnType<typeof vec4>,
    ReturnType<typeof vec3>,
    ReturnType<typeof float>,
  ]) => {
    const wave = vec4(wave_immutable)
    const p = vec3(p_immutable)
    const time = float(time_immutable)
    const k = PI.mul(2).div(wave.w)
    const c = sqrt(float(9.8).div(k))
    const d = normalize(wave.xy)
    const f = k.mul(dot(d, p.xz).sub(c.mul(time).mul(0.1)))
    const sf = sin(f)
    const cf = cos(f)
    return vec4(
      d.x.mul(d.x).mul(wave.z).mul(sf).negate(),
      d.x.mul(wave.z).mul(cf),
      d.y.mul(d.y).mul(wave.z).mul(sf).negate(),
      d.y.mul(wave.z).mul(cf)
    )
  }
)
