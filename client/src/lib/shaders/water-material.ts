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
  uHeightmapTexture: { value: THREE.Texture }
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
  const uFoamBandColor = uniform(new THREE.Color(0.7, 0.9, 0.92))
  const uShallowColor = uniform(new THREE.Color(0.1, 0.7, 0.7))
  const uMidColor = uniform(new THREE.Color(0.0, 0.4, 0.6))
  const uDeepColor = uniform(new THREE.Color(0.0, 0.05, 0.14))
  const uMaxDepth = uniform(1.8)
  const uSunDirection = uniform(new THREE.Vector3(0.5, 0.8, 0.3).normalize())
  const uSunColor = uniform(new THREE.Color(1.0, 0.95, 0.8))
  const uCameraDirection = uniform(new THREE.Vector3(0, -1, 0))
  const uRefractionStrength = uniform(0.02)

  // Texture nodes (use texture() directly — update via .value)
  const heightmapTex = texture(options.heightmapTexture)
  const normalMapTex = texture(options.normalMap)
  const foamMapTex = texture(options.foamMap)
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

    vWorldPos.assign(worldPos.xyz)

    const clipPos = cameraProjectionMatrix.mul(cameraViewMatrix).mul(worldPos)
    vClipPos.assign(clipPos)

    return clipPos
  })()

  // ─── Fragment: full water shading ──────────────────
  const fragmentNode = Fn(() => {
    // 1. Depth calculation
    // Remap UV to align 65 vertices with 65×65 texel centers:
    // vertex k has UV = k/64, texel center k is at (k+0.5)/65
    const heightmapUV = vUv.mul(64.0 / 65.0).add(0.5 / 65.0)
    const terrainHeight = heightmapTex.sample(heightmapUV).r
    const depth = max(float(0), vOrigWorldPos.y.sub(terrainHeight))
    const depthFactor = clamp(depth.div(uMaxDepth), 0.0, 1.0)

    // 2. Depth-based color (4-stop gradient)
    const smoothDepth = smoothstep(float(0), float(1), depthFactor)
    const c1 = mix(
      uFoamBandColor,
      uShallowColor,
      smoothstep(float(0), float(0.15), depthFactor)
    )
    const c2 = mix(
      c1,
      uMidColor,
      smoothstep(float(0.15), float(0.4), depthFactor)
    )
    const waterColor = mix(
      c2,
      uDeepColor,
      smoothstep(float(0.4), float(1.0), depthFactor)
    ).toVar()

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

    // 5. Shore foam — wide breaking waves
    const foamNoise = noise.x.mul(0.5).add(0.5)

    // Noise-perturbed depth for irregular edges
    const noisyD = depth
      .add(noise.x.mul(0.15))
      .add(noise.z.mul(0.1))
      .add(valueNoise(vOrigWorldPos.xz.mul(0.2)).mul(0.3))

    // Two wave cycles offset by half phase
    const waveSpeed = float(0.02)
    const cycle1 = fract(uTime.mul(waveSpeed))
    const cycle2 = fract(uTime.mul(waveSpeed).add(0.5))

    // Waves move from deeper water toward shore
    const spawnDepth = float(1.0)
    const shoreDepth = float(0.05)
    const move1 = smoothstep(float(0), float(0.75), cycle1)
    const move2 = smoothstep(float(0), float(0.75), cycle2)
    const center1 = mix(spawnDepth, shoreDepth, move1)
    const center2 = mix(spawnDepth, shoreDepth, move2)

    // Fade in/out
    const fade1 = smoothstep(float(0), float(0.1), cycle1).mul(
      float(1).sub(smoothstep(float(0.8), float(1), cycle1))
    )
    const fade2 = smoothstep(float(0), float(0.1), cycle2).mul(
      float(1).sub(smoothstep(float(0.8), float(1), cycle2))
    )

    // Wide bands — thick near spawn, thinner near shore
    const bandWidth1 = float(0.12).add(float(0.18).mul(float(1).sub(move1)))
    const bandWidth2 = float(0.12).add(float(0.18).mul(float(1).sub(move2)))

    // Soft band shape
    const band1 = smoothstep(center1.sub(bandWidth1), center1, noisyD)
      .mul(float(1).sub(smoothstep(center1, center1.add(bandWidth1), noisyD)))
      .mul(fade1)
      .toVar()
    const band2 = smoothstep(center2.sub(bandWidth2), center2, noisyD)
      .mul(float(1).sub(smoothstep(center2, center2.add(bandWidth2), noisyD)))
      .mul(fade2)
      .toVar()

    // Break up with large-scale noise for organic edges
    const bn1 = valueNoise(vOrigWorldPos.xz.mul(0.15).add(center1.mul(1.5)))
    const bn2 = valueNoise(vOrigWorldPos.xz.mul(0.15).add(center2.mul(1.5)))
    band1.mulAssign(smoothstep(float(0.2), float(0.5), bn1))
    band2.mulAssign(smoothstep(float(0.2), float(0.5), bn2))

    // Persistent shore foam — always present near the edge
    const shoreBase = float(1)
      .sub(smoothstep(float(0), float(0.5), noisyD))
      .mul(0.6)

    // Subtle brightening near shore
    const foamGlow = float(1)
      .sub(smoothstep(float(0), float(0.5), depth))
      .mul(0.1)

    // Blend water with sky reflection via Fresnel, then add specular
    const surfaceColor = mix(waterColor, skyReflection, fresnel.mul(0.6))
      .add(specular)
      .toVar()

    // Foam texture with band movement
    const foamUV1 = vOrigWorldPos.xz.mul(0.4).add(cycle1.mul(0.3))
    const foamUV2 = vOrigWorldPos.xz.mul(0.4).add(cycle2.mul(0.3))
    const foamTex1 = foamMapTex.sample(foamUV1).r
    const foamTex2 = foamMapTex.sample(foamUV2).r

    // Blend foam — combine wave bands, persistent shore foam, and glow
    const waveFoam = max(band1.mul(foamTex1), band2.mul(foamTex2))
    const foamWithTex = clamp(
      max(max(waveFoam, shoreBase.mul(foamNoise)), foamGlow),
      0.0,
      1.0
    )
    const foamColor = mix(vec3(0.85, 0.92, 0.95), vec3(1, 1, 1), foamWithTex)
    const finalColor = mix(surfaceColor, foamColor, foamWithTex.mul(0.9))

    // 6. Alpha
    const alpha = mix(float(0.15), float(0.85), smoothDepth)
      .add(foamWithTex.mul(0.5))
      .add(sparkle)
      .min(1.0)
      .toVar()

    // 7. Shore edge — noisy holes near coastline to reveal terrain underneath
    const shoreZone = float(1).sub(smoothstep(float(0), float(0.6), depth))
    const sn1 = valueNoise(vOrigWorldPos.xz.mul(0.8).add(uTime.mul(0.07)))
    const sn2 = valueNoise(vOrigWorldPos.xz.mul(1.5).add(uTime.mul(0.04)))
    const sn3 = valueNoise(vOrigWorldPos.xz.mul(0.3).add(uTime.mul(0.1)))
    const holeMask = sn1.mul(0.5).add(sn2.mul(0.3)).add(sn3.mul(0.2))
    // Near shore: if noise < threshold, punch alpha to 0
    const holeThreshold = shoreZone.mul(0.6)
    const holeAlpha = smoothstep(
      holeThreshold.sub(0.05),
      holeThreshold.add(0.05),
      holeMask
    )
    alpha.mulAssign(holeAlpha)

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
      uHeightmapTexture: heightmapTex,
    },
  }
}
