import * as THREE from 'three'
import { NodeMaterial } from 'three/webgpu'
import {
  Fn,
  uniform,
  texture,
  uv,
  vec2,
  vec3,
  vec4,
  float,
  sin,
  cos,
  sqrt,
  dot,
  normalize,
  smoothstep,
  mix,
  clamp,
  pow,
  fract,
  floor,
  max,
  min,
  reflect,
  varying,
  positionLocal,
  modelWorldMatrix,
  cameraProjectionMatrix,
  cameraViewMatrix,
} from 'three/tsl'

// ─── Uniforms ────────────────────────────────────────────
const PI = float(Math.PI)

// ─── Gerstner Wave (TSL Fn) ─────────────────────────────
const gerstnerWave = /* #__PURE__ */ Fn(
  ([wave_immutable, p_immutable, time_immutable]: [
    ReturnType<typeof vec4>,
    ReturnType<typeof vec3>,
    ReturnType<typeof float>,
  ]) => {
    const wave = vec4(wave_immutable)
    const p = vec3(p_immutable)
    const time = float(time_immutable)

    const steepness = wave.z
    const wavelength = wave.w
    const k = PI.mul(2).div(wavelength)
    const c = sqrt(float(9.8).div(k))
    const d = normalize(wave.xy)
    const f = k.mul(dot(d, p.xz).sub(c.mul(time).mul(0.1)))
    const a = steepness.div(k)
    return vec3(d.x.mul(a).mul(cos(f)), a.mul(sin(f)), d.y.mul(a).mul(cos(f)))
  }
)

// ─── Hash-based value noise ─────────────────────────────
const hash = /* #__PURE__ */ Fn(([p_immutable]: [ReturnType<typeof vec2>]) => {
  const p = vec2(p_immutable)
  return fract(sin(dot(p, vec2(127.1, 311.7))).mul(43758.5453))
})

const valueNoise = /* #__PURE__ */ Fn(
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

// ─── 4-sample normal noise ──────────────────────────────
const getNoise = /* #__PURE__ */ Fn(
  ([worldXZ_immutable, normalMapTex, time_immutable]: [
    ReturnType<typeof vec2>,
    ReturnType<typeof texture>,
    ReturnType<typeof float>,
  ]) => {
    const worldXZ = vec2(worldXZ_immutable)
    const time = float(time_immutable)
    const t = time.mul(0.06)

    const uv0 = worldXZ.div(79.0).add(vec2(t.div(17.0), t.div(29.0)))
    const uv1 = worldXZ.div(263.0).sub(vec2(t.div(-19.0), t.div(31.0)))
    const uv2 = worldXZ
      .div(vec2(8907.0, 9803.0))
      .add(vec2(t.div(101.0), t.div(97.0)))
    const uv3 = worldXZ
      .div(vec2(1091.0, 1027.0))
      .sub(vec2(t.div(109.0), t.div(-113.0)))

    const noise = normalMapTex
      .sample(uv0)
      .add(normalMapTex.sample(uv1))
      .add(normalMapTex.sample(uv2))
      .add(normalMapTex.sample(uv3))

    return noise.mul(0.5).sub(1.0)
  }
)

// ─── Export interface ────────────────────────────────────
export interface WaterMaterialOptions {
  heightmapTexture: THREE.DataTexture
  normalMap: THREE.Texture
  foamMap: THREE.Texture
  surfaceMap: THREE.Texture
  refractionMap?: THREE.Texture | null
}

export interface WaterMaterialUniforms {
  uTime: { value: number }
  uSunDirection: { value: THREE.Vector3 }
  uSunColor: { value: THREE.Color }
  uCameraDirection: { value: THREE.Vector3 }
  uRefractionMap: { value: THREE.Texture }
}

export interface WaterMaterialResult {
  material: NodeMaterial
  uniforms: WaterMaterialUniforms
}

export function createWaterMaterial(
  options: WaterMaterialOptions
): WaterMaterialResult {
  const fallbackTex = new THREE.DataTexture(
    new Uint8Array([128, 128, 128, 255]),
    1,
    1,
    THREE.RGBAFormat
  )
  fallbackTex.needsUpdate = true

  // Gerstner wave direction helpers
  const degToDir = (deg: number) => {
    const rad = (deg * Math.PI) / 180
    return [Math.sin(rad), Math.cos(rad)]
  }
  const [ax, az] = degToDir(0)
  const [bx, bz] = degToDir(30)
  const [cx, cz] = degToDir(60)

  // Scalar/vector uniforms
  const uTime = uniform(0)
  const uWaveA = uniform(new THREE.Vector4(ax, az, 0.03, 20))
  const uWaveB = uniform(new THREE.Vector4(bx, bz, 0.02, 15))
  const uWaveC = uniform(new THREE.Vector4(cx, cz, 0.015, 10))
  const uShallowColor = uniform(new THREE.Color(0.08, 0.55, 0.58))
  const uDeepColor = uniform(new THREE.Color(0.02, 0.05, 0.18))
  const uMaxDepth = uniform(1.8)
  const uSunDirection = uniform(new THREE.Vector3(0.5, 0.8, 0.3).normalize())
  const uSunColor = uniform(new THREE.Color(1.0, 0.95, 0.8))
  const uCameraDirection = uniform(new THREE.Vector3(0, -1, 0))
  const uRefractionStrength = uniform(0.02)

  // Texture nodes (use texture() directly — update via .value)
  const heightmapTex = texture(options.heightmapTexture)
  const normalMapTex = texture(options.normalMap)
  const foamMapTex = texture(options.foamMap)
  const surfaceMapTex = texture(options.surfaceMap)
  const refractionTex = texture(options.refractionMap ?? fallbackTex)

  // ─── Vertex: Gerstner wave displacement ────────────
  const vOrigWorldPos = varying(vec3(0), 'v_origWorldPos')
  const vWorldPos = varying(vec3(0), 'v_worldPos')
  const vWaveHeight = varying(float(0), 'v_waveHeight')
  const vClipPos = varying(vec4(0), 'v_clipPos')
  const vUv = varying(vec2(0), 'v_uv')

  const positionNode = Fn(() => {
    const localPos = positionLocal.toVar()
    vUv.assign(uv())

    const worldPos = modelWorldMatrix.mul(vec4(localPos, 1.0)).toVar()
    vOrigWorldPos.assign(worldPos.xyz)

    const p = worldPos.xyz
    const offset = gerstnerWave(uWaveA, p, uTime)
      .add(gerstnerWave(uWaveB, p, uTime))
      .add(gerstnerWave(uWaveC, p, uTime))
      .toVar()

    worldPos.xyz.addAssign(offset)
    vWaveHeight.assign(offset.y)

    // Clamp wave so it never dips below the terrain
    const terrainHeight = heightmapTex.sample(uv()).r
    worldPos.y.assign(max(terrainHeight.add(0.01), worldPos.y))

    vWorldPos.assign(worldPos.xyz)

    const clipPos = cameraProjectionMatrix.mul(cameraViewMatrix).mul(worldPos)
    vClipPos.assign(clipPos)

    return clipPos
  })()

  // ─── Fragment: full water shading ──────────────────
  const fragmentNode = Fn(() => {
    // 1. Depth calculation
    const terrainHeight = heightmapTex.sample(vUv).r
    const depth = max(float(0), vOrigWorldPos.y.sub(terrainHeight))
    const depthFactor = clamp(depth.div(uMaxDepth), 0.0, 1.0)

    // 2. Depth-based color
    const smoothDepth = smoothstep(float(0), float(1), depthFactor)
    const waterColor = mix(uShallowColor, uDeepColor, smoothDepth).toVar()

    // 3. Surface normal from 4-sample noise
    const noise = getNoise(vOrigWorldPos.xz, normalMapTex, uTime).toVar()
    const surfaceNormal = normalize(noise.xzy.mul(vec3(1.5, 1.0, 1.5)))

    // View direction
    const viewDir = normalize(vec3(uCameraDirection).negate())

    // 4. Refraction
    const screenUV = vClipPos.xy.mul(0.5).add(0.5)
    const refractionUV = clamp(
      screenUV.add(surfaceNormal.xz.mul(uRefractionStrength)),
      0.0,
      1.0
    )
    const refractionColor = refractionTex.sample(refractionUV).rgb

    // Blend refraction with depth tint
    const refractionMix = float(1).sub(
      smoothstep(float(0), float(0.7), smoothDepth)
    )
    waterColor.assign(mix(waterColor, refractionColor, refractionMix.mul(0.7)))

    // Specular: broad sun reflection
    const halfDir = normalize(vec3(uSunDirection).add(viewDir))
    const NdotH = max(dot(surfaceNormal, halfDir), 0.0)
    const specBroad = pow(NdotH, float(64)).mul(0.35)
    const specular = vec3(uSunColor).mul(specBroad).toVar()

    // Sun sparkles
    const spT = uTime.mul(0.04)
    const spUV1 = vOrigWorldPos.xz.mul(0.5).add(vec2(spT, spT.mul(0.7)))
    const spUV2 = vOrigWorldPos.xz.mul(0.8).sub(vec2(spT.mul(0.6), spT))
    const sp1 = normalMapTex.sample(spUV1).r
    const sp2 = normalMapTex.sample(spUV2).g
    const sparkle = smoothstep(float(1.2), float(1.38), sp1.add(sp2))
      .mul(0.8)
      .mul(
        smoothstep(float(0), float(0.15), uSunDirection.y).mul(
          float(0.3).add(float(0.7).mul(uSunDirection.y))
        )
      )
    specular.addAssign(vec3(uSunColor).mul(sparkle))

    // Diffuse lighting
    const diffuse = max(dot(surfaceNormal, vec3(uSunDirection)), 0.0).mul(0.1)

    // Smoothed normal for reflection
    const reflNormal = normalize(mix(vec3(0, 1, 0), surfaceNormal, 0.3))

    // Fresnel reflection
    const cosTheta = max(dot(viewDir, reflNormal), 0.0)
    const fresnel = float(0.1).add(
      float(0.9).mul(pow(float(1).sub(cosTheta), float(2)))
    )

    // Procedural sky reflection
    const reflectDir = reflect(viewDir.negate(), reflNormal)
    const skyY = clamp(reflectDir.y.mul(0.5).add(0.5), 0.0, 1.0)
    const skyBrightness = smoothstep(float(-0.1), float(0.3), uSunDirection.y)

    const groundColor = vec3(0.08, 0.12, 0.15).mul(skyBrightness)
    const hazeColorBase = vec3(0.55, 0.65, 0.75).mul(skyBrightness)
    const zenithColor = vec3(0.12, 0.25, 0.5).mul(skyBrightness)

    const sunsetFactor = float(1).sub(
      smoothstep(float(0), float(0.5), uSunDirection.y)
    )
    const hazeColor = mix(
      hazeColorBase,
      vec3(uSunColor).mul(0.5),
      sunsetFactor.mul(0.3)
    )

    const skyReflection = mix(
      mix(groundColor, hazeColor, smoothstep(float(0), float(0.35), skyY)),
      zenithColor,
      smoothstep(float(0.35), float(0.7), skyY)
    ).toVar()

    const sunDot = max(dot(reflectDir, vec3(uSunDirection)), 0.0)
    skyReflection.addAssign(
      vec3(uSunColor).mul(pow(sunDot, float(8)).mul(0.25))
    )

    // 5. Shore foam — animated waves
    const noisePerturb = noise.x.mul(0.07).add(noise.z.mul(0.04))
    const foamNoise = noise.x.mul(0.5).add(0.5)
    const noisyD = depth.add(noisePerturb)

    const waveSpeed = float(0.0175)
    const spawnDepth = float(1.5)
    const bandHalfMax = float(0.03)

    const cycle1 = fract(uTime.mul(waveSpeed))
    const cycle2 = fract(uTime.mul(waveSpeed).add(0.5))

    const movePhase1 = min(cycle1.div(0.7), float(1))
    const movePhase2 = min(cycle2.div(0.7), float(1))
    const minDepth = float(0.25)
    const center1 = mix(spawnDepth, minDepth, movePhase1)
    const center2 = mix(spawnDepth, minDepth, movePhase2)

    const fadeIn1 = smoothstep(float(0), float(0.15), cycle1)
    const fadeIn2 = smoothstep(float(0), float(0.15), cycle2)
    const fadeOut1 = float(1).sub(smoothstep(float(0.85), float(1), cycle1))
    const fadeOut2 = float(1).sub(smoothstep(float(0.85), float(1), cycle2))

    const proximity1 = clamp(center1.div(spawnDepth), 0.0, 1.0)
    const proximity2 = clamp(center2.div(spawnDepth), 0.0, 1.0)

    const thickVar1 = float(0.7).add(
      float(0.6).mul(
        sin(
          vOrigWorldPos.x
            .mul(2.1)
            .add(vOrigWorldPos.z.mul(1.7))
            .add(center1.mul(4))
        )
      )
    )
    const thickVar2 = float(0.7).add(
      float(0.6).mul(
        sin(
          vOrigWorldPos.x
            .mul(1.8)
            .add(vOrigWorldPos.z.mul(2.3))
            .add(center2.mul(4))
        )
      )
    )

    const bh1 = bandHalfMax
      .mul(float(0.15).add(float(0.85).mul(float(1).sub(proximity1))))
      .mul(thickVar1)
    const bh2 = bandHalfMax
      .mul(float(0.15).add(float(0.85).mul(float(1).sub(proximity2))))
      .mul(thickVar2)
    const bright1 = float(1)
      .add(float(0.6).mul(float(1).sub(proximity1)))
      .mul(fadeIn1)
      .mul(fadeOut1)
    const bright2 = float(1)
      .add(float(0.6).mul(float(1).sub(proximity2)))
      .mul(fadeIn2)
      .mul(fadeOut2)

    // Soft bands around each center
    const band1 = smoothstep(
      center1.sub(bh1).sub(0.06),
      center1.sub(bh1),
      noisyD
    )
      .mul(
        float(1).sub(
          smoothstep(center1.add(bh1), center1.add(bh1).add(0.06), noisyD)
        )
      )
      .toVar()
    const band2 = smoothstep(
      center2.sub(bh2).sub(0.06),
      center2.sub(bh2),
      noisyD
    )
      .mul(
        float(1).sub(
          smoothstep(center2.add(bh2), center2.add(bh2).add(0.06), noisyD)
        )
      )
      .toVar()

    // Break bands with value noise
    const bn1 = valueNoise(vOrigWorldPos.xz.mul(0.3).add(center1.mul(2)))
    const bn2 = valueNoise(vOrigWorldPos.xz.mul(0.3).add(center2.mul(2)))
    band1.mulAssign(smoothstep(float(0.35), float(0.55), bn1))
    band2.mulAssign(smoothstep(float(0.35), float(0.55), bn2))

    // Density variation from noise
    band1.mulAssign(
      smoothstep(float(0.2), float(0.55), foamNoise).mul(0.25).mul(bright1)
    )
    band2.mulAssign(
      smoothstep(float(0.25), float(0.6), foamNoise).mul(0.2).mul(bright2)
    )

    // Subtle brightening near shore
    const foamGlow = float(1)
      .sub(smoothstep(float(0), float(0.35), depth))
      .mul(0.06)

    // Surface texture
    const st = uTime.mul(0.008)
    const surfUV0 = vWorldPos.xz.mul(0.12).add(vec2(st, st.mul(0.7)))
    const surfUV1 = vWorldPos.xz.mul(0.08).sub(vec2(st.mul(0.6), st.mul(0.9)))
    const surfTex = surfaceMapTex
      .sample(surfUV0)
      .rgb.add(surfaceMapTex.sample(surfUV1).rgb)
      .mul(0.5)

    // Blend water with sky reflection via Fresnel, then add specular
    const litWater = mix(waterColor, surfTex, 0.3).add(diffuse)
    const surfaceColor = mix(litWater, skyReflection, fresnel.mul(0.6))
      .add(specular)
      .toVar()

    // Foam texture with band movement
    const foamUV1 = vOrigWorldPos.xz.mul(0.4).add(cycle1.mul(0.3))
    const foamUV2 = vOrigWorldPos.xz.mul(0.4).add(cycle2.mul(0.3))
    const foamTex1 = foamMapTex.sample(foamUV1).r
    const foamTex2 = foamMapTex.sample(foamUV2).r

    // Wave crest foam
    const crestFoam = smoothstep(float(0.08), float(0.18), vWaveHeight)
      .mul(0.3)
      .mul(smoothstep(float(0.2), float(0.55), foamNoise))
      .toVar()

    // Blend foam
    const shoreFoam = clamp(
      max(max(band1.mul(foamTex1), band2.mul(foamTex2)), foamGlow),
      0.0,
      1.0
    )
    const foamWithTex = clamp(max(shoreFoam, crestFoam), 0.0, 1.0)
    const foamColor = mix(vec3(0.85, 0.92, 0.95), vec3(1, 1, 1), foamWithTex)
    const finalColor = mix(surfaceColor, foamColor, foamWithTex.mul(0.9))

    // 6. Alpha
    const alpha = mix(float(0.45), float(0.95), smoothDepth)
      .add(foamWithTex.mul(0.5))
      .min(1.0)
      .toVar()

    // 7. Shore edge softening
    const shoreFade = smoothstep(
      float(0),
      float(0.25),
      depth.add(noise.y.mul(0.12)).add(noise.x.mul(0.06))
    )
    alpha.mulAssign(max(shoreFade, foamWithTex.mul(0.85)))

    return vec4(finalColor, alpha)
  })()

  // ─── Build material ────────────────────────────────
  const material = new NodeMaterial()
  material.transparent = true
  material.depthWrite = false
  material.side = THREE.FrontSide
  material.vertexNode = positionNode
  material.fragmentNode = fragmentNode

  return {
    material,
    uniforms: {
      uTime,
      uSunDirection,
      uSunColor,
      uCameraDirection,
      uRefractionMap: refractionTex,
    },
  }
}
