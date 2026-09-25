import * as THREE from 'three'
import type { MeshStandardNodeMaterial, Node } from 'three/webgpu'
import {
  Fn,
  If,
  uniform,
  convert,
  texture,
  vec2,
  vec3,
  float,
  mix,
  smoothstep,
  positionWorld,
  normalWorldGeometry,
  normalViewGeometry,
  output,
  dot,
  sin,
  fract,
  floor,
  length,
  abs,
  max,
  fwidth,
} from 'three/tsl'
import { hash } from './tsl-noise'
import { getNoiseTexture, NOISE_PERIODS } from './water-shore-waves'
import { TERRAIN_TILE_SIZE } from '../components/game-scene/terrain-utils'
import { PUDDLE_VISIBLE_WETNESS } from '../utils/rainPuddles'

export function createRainPuddleUniforms() {
  return {
    time: uniform(0),
    enabled: uniform(1),
    rippleFacing: uniform(new THREE.Vector2(0, -1)),
    rippleStrength: uniform(0),
    noise: texture(getNoiseTexture()),
  }
}

export type RainPuddleUniforms = ReturnType<typeof createRainPuddleUniforms>

export function updateRainPuddleLighting(
  shared: RainPuddleUniforms,
  time: number,
  sunDirection: THREE.Vector3,
  cameraDirection: THREE.Vector3
) {
  const day = THREE.MathUtils.smoothstep(sunDirection.y, -0.12, 0.18)
  const horizontalLength = Math.hypot(cameraDirection.x, cameraDirection.z)
  const forwardX =
    horizontalLength > 0.001 ? cameraDirection.x / horizontalLength : 0
  const forwardZ =
    horizontalLength > 0.001 ? cameraDirection.z / horizontalLength : 1
  shared.time.value = time
  shared.rippleFacing.value.set(-forwardX, -forwardZ)
  shared.rippleStrength.value = 0.01 + day * 0.03
}

export function applyRainPuddles(
  material: MeshStandardNodeMaterial,
  shared: RainPuddleUniforms,
  editorActive?: Node<'float'>
) {
  const wetness = uniform(new THREE.Vector4())
  const rain = uniform(new THREE.Vector4())
  const origin = uniform(new THREE.Vector2())
  material.userData.rainPuddles = { wetness, rain, origin }

  const p = positionWorld.xz
  const cell = p.sub(origin).div(TERRAIN_TILE_SIZE).clamp(0, 1)
  const interpolate = (v: Node<'vec4'>) =>
    mix(mix(v.x, v.y, cell.x), mix(v.z, v.w, cell.x), cell.y)
  const editorGate = editorActive
    ? float(1).sub(editorActive.clamp(0, 1))
    : float(1)
  const moisture = interpolate(wetness).mul(shared.enabled).mul(editorGate)
  const puddleSurface = Fn(() => {
    const surface = vec2(0).toVar()

    If(moisture.greaterThan(PUDDLE_VISIBLE_WETNESS), () => {
      const flatness = smoothstep(0.985, 0.999, normalWorldGeometry.y)
      const broadNoise = shared.noise
        .sample(p.mul(0.38 / NOISE_PERIODS))
        .r.toVar()
      const fineNoise = shared.noise
        .sample(p.mul(1.15).add(31.7).div(NOISE_PERIODS))
        .r.toVar()
      const noise = broadNoise.mul(0.72).add(fineNoise.mul(0.28))
      const fill = smoothstep(PUDDLE_VISIBLE_WETNESS, 1, moisture)
      const threshold = mix(float(0.88), float(0.48), fill)
      const edge = noise.sub(threshold.add(0.0175))
      const edgeWidth = max(fwidth(edge).mul(0.75), 0.0015)
      const mask = smoothstep(edgeWidth.negate(), edgeWidth, edge)
        .mul(flatness)
        .mul(smoothstep(0.02, 0.15, positionWorld.y))
        .mul(smoothstep(PUDDLE_VISIBLE_WETNESS, 0.3, moisture))
      const rim = float(1).sub(
        smoothstep(edgeWidth, edgeWidth.add(0.012), edge)
      )
      surface.assign(vec2(mask, rim))
    })
    return surface
  })().toVar()
  const mask = puddleSurface.x
  const lumaWeights = vec3(0.299, 0.587, 0.114)
  const bedColor = material.colorNode
  if (bedColor) {
    material.colorNode = Fn(() => {
      const color = (convert(bedColor, 'vec4') as Node<'vec4'>).toVar()
      const luma = dot(color.rgb, lumaWeights)
      const softened = color.rgb.mul(float(0.55).div(luma.add(0.3)))
      color.rgb.assign(mix(color.rgb, softened, mask.mul(0.45)))
      return color
    })()
  }
  const bedNormal =
    (material.normalNode as Node<'vec3'> | null) ?? normalViewGeometry
  material.normalNode = mix(
    bedNormal,
    normalViewGeometry,
    mask.mul(0.94)
  ).normalize()
  const bedOcclusion = (material.aoNode as Node<'float'> | null) ?? float(1)
  material.aoNode = mix(bedOcclusion, float(1), mask.mul(0.6))

  material.outputNode = Fn(() => {
    const result = output.toVar()
    If(moisture.greaterThan(0.001), () => {
      const dampness = smoothstep(0, 0.45, moisture)
      result.rgb.mulAssign(float(1).sub(dampness.mul(0.38)))

      If(mask.greaterThan(0.001), () => {
        const intensity = interpolate(rain)
        const rippleHighlight = float(0).toVar()
        If(
          intensity.greaterThan(0.02).and(shared.rippleStrength.greaterThan(0)),
          () => {
            const seed = hash(floor(p.mul(1.3)))
            const age = fract(shared.time.mul(0.72).add(seed))
            const center = vec2(seed, fract(seed.mul(17.17)))
              .mul(0.6)
              .add(0.2)
            const delta = fract(p.mul(1.3)).sub(center)
            const radius = length(delta)
            const ripple = sin(radius.mul(65).sub(age.mul(30)))
              .mul(
                float(1).sub(
                  smoothstep(0.04, 0.12, abs(radius.sub(age.mul(0.65))))
                )
              )
              .mul(float(1).sub(age))
              .mul(smoothstep(0, 0.08, age))
              .mul(intensity)
            const rippleSlope = delta
              .div(max(radius, 0.01))
              .mul(ripple.mul(0.08))
            rippleHighlight.assign(
              smoothstep(
                0.006,
                0.026,
                dot(rippleSlope, shared.rippleFacing)
              ).mul(shared.rippleStrength)
            )
          }
        )
        const water = result.rgb.mul(float(0.72).sub(puddleSurface.y.mul(0.08)))
        const surface = water.add(vec3(0.65, 0.7, 0.75).mul(rippleHighlight))
        result.rgb.assign(mix(result.rgb, surface, mask))
      })
    })
    return result
  })()
}
