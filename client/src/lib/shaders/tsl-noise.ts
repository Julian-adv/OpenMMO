import { Fn, vec2, float, fract, sin, dot, floor, mix } from 'three/tsl'

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
