import * as THREE from 'three'
import { NodeMaterial } from 'three/webgpu'
import {
  Fn,
  uniform,
  texture,
  uv,
  attribute,
  vec2,
  vec3,
  vec4,
  float,
  smoothstep,
  mix,
  clamp,
  pow,
  abs,
  max,
  reflect,
  varying,
  normalize,
  dot,
  positionLocal,
  modelWorldMatrix,
  cameraProjectionMatrix,
  cameraViewMatrix,
} from 'three/tsl'
import { waterFallbackTex } from './water-types'

// Sky-cloud reference photo (see doc/ASSETS.md). Non-tileable so we
// MirroredRepeat to hide seams across the projected cloud plane.
let _cloudTex: THREE.Texture | null = null
function getCloudTexture(): THREE.Texture {
  if (!_cloudTex) {
    const loader = new THREE.TextureLoader()
    _cloudTex = loader.load('/textures/white-cloud.jpg')
    _cloudTex.wrapS = _cloudTex.wrapT = THREE.MirroredRepeatWrapping
    _cloudTex.minFilter = THREE.LinearMipMapLinearFilter
    _cloudTex.magFilter = THREE.LinearFilter
    // Photo is sRGB-encoded; without this it's treated as linear and all
    // colors wash out to a milky pale since gamma decode is skipped.
    _cloudTex.colorSpace = THREE.SRGBColorSpace
  }
  return _cloudTex
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any // TSL node — broad type for internal helper params

export interface RiverMaterialOptions {
  normalMap: THREE.Texture
  reflectionMap?: THREE.Texture | null
}

export interface RiverMaterialUniforms {
  uTime: { value: number }
  uSunDirection: { value: THREE.Vector3 }
  uSunColor: { value: THREE.Color }
  uCameraDirection: { value: THREE.Vector3 }
  uMoonBrightness: { value: number }
  uReflectionMap: { value: THREE.Texture }
  uNormalMap: { value: THREE.Texture }
}

export interface RiverMaterialResult {
  material: NodeMaterial
  uniforms: RiverMaterialUniforms
}

/**
 * River surface shader. Shares the visual language of the ocean
 * (water-material.ts) — color palette, sun specular, sky reflection,
 * day/night — but drops Gerstner swells and heightmap-based depth since
 * the river ribbon is a flat strip carried by a carved channel.
 *
 * Variation across the ribbon comes from the geometry attributes produced
 * by `buildRiverGeometry`:
 *   uv.x       — 0 at left bank, 1 at right bank
 *   uv.y       — cumulative chain length (meters), used for scrolling
 *   flowDir    — per-vertex XZ tangent, drives normal/foam scroll
 *   flowNorm   — 0..1 flow accumulation, scales scroll speed
 */
export function createRiverMaterial(
  options: RiverMaterialOptions
): RiverMaterialResult {
  // ── Uniforms ──
  const uTime = uniform(0)
  const uSunDirection = uniform(new THREE.Vector3(0.5, 0.8, 0.3).normalize())
  const uSunColor = uniform(new THREE.Color(1.0, 0.95, 0.8))
  const uCameraDirection = uniform(new THREE.Vector3(0, -1, 0))
  const uMoonBrightness = uniform(0)

  // Palette — dark inland river: muddy bank → mid blue → deep navy. Mostly
  // a mirror for the sky, which is where the "color" really comes from.
  const uVeryShallowColor = uniform(new THREE.Color(0.22, 0.3, 0.3))
  const uShallowColor = uniform(new THREE.Color(0.06, 0.14, 0.22))
  const uMidColor = uniform(new THREE.Color(0.02, 0.05, 0.12))

  // ── Texture Nodes ──
  const normalMapTex = texture(options.normalMap)
  const reflectionTex = texture(options.reflectionMap ?? waterFallbackTex)
  const cloudTex = texture(getCloudTexture())

  // ── Varyings / Attributes ──
  const vWorldPos = varying(vec3(0), 'r_worldPos')
  const vClipPos = varying(vec4(0), 'r_clipPos')
  const vFlowDir = varying(vec2(0), 'r_flowDir')
  const vFlowNorm = varying(float(0), 'r_flowNorm')

  const aFlowDir = attribute('flowDir', 'vec2')
  const aFlowNorm = attribute('flowNorm', 'float')

  // ── Vertex Shader ─────────────────────────────────────

  const positionNode = Fn(() => {
    const localPos = vec4(positionLocal, 1.0)
    const worldPos = modelWorldMatrix.mul(localPos).toVar()
    vWorldPos.assign(worldPos.xyz)
    vFlowDir.assign(aFlowDir)
    vFlowNorm.assign(aFlowNorm)
    const clipPos = cameraProjectionMatrix.mul(cameraViewMatrix).mul(worldPos)
    vClipPos.assign(clipPos)
    return clipPos
  })()

  // ── Fragment Shader ───────────────────────────────────

  const fragmentNode = Fn(() => {
    const uvCoord = uv()
    // Distance from centerline across the ribbon: 0 center, 1 either bank.
    const bankFactor = abs(uvCoord.x.sub(0.5)).mul(2.0).toVar()
    // "Depth" proxy: deeper in the center, shallow at banks.
    const depthFactor = float(1.0).sub(bankFactor).toVar()
    const sunY = uSunDirection.y

    // ── Base color: 3-stop gradient bank → center ──
    // Tighter to the bank so most of the ribbon is the deep body color;
    // the very-shallow murk only shows in a thin strip at the edge.
    const shallowBlend = smoothstep(float(0.0), float(0.15), depthFactor)
    const midBlend = smoothstep(float(0.1), float(0.4), depthFactor)
    const c1 = mix(uVeryShallowColor, uShallowColor, shallowBlend)
    const waterColor = mix(c1, uMidColor, midBlend).toVar()

    // Night darkening (same shape as ocean)
    const waterNightFactor = smoothstep(float(-0.05), float(0.1), sunY)
      .mul(0.85)
      .add(0.15)
    waterColor.mulAssign(waterNightFactor)

    // ── Flow-aligned normal map ──
    // Scroll speed rises with flow accumulation; clamp at a baseline so
    // stagnant headwaters still ripple a little. Kept slow so the surface
    // reads as a calm lowland river rather than a chute.
    const scrollSpeed = float(0.06).add(vFlowNorm.mul(0.22))
    const flow = normalize(vFlowDir)
    // Cross-flow vector for the secondary layer — breaks up tiling seams.
    const cross = vec2(flow.y.negate(), flow.x)

    const baseUV1 = vWorldPos.xz.mul(0.45)
    const baseUV2 = vWorldPos.xz.mul(0.27)
    // Scrolling UV by `+flow*t` makes the pattern appear to move in `-flow`,
    // so we subtract to get visible downstream motion.
    const nUV1 = baseUV1.sub(flow.mul(uTime.mul(scrollSpeed)))
    const nUV2 = baseUV2.sub(
      flow.mul(uTime.mul(scrollSpeed).mul(0.6)).sub(cross.mul(0.3))
    )

    const nSample = normalMapTex
      .sample(nUV1)
      .add(normalMapTex.sample(nUV2))
      .mul(0.5)
      .sub(1.0)
    // Rotate XZ-plane perturbation using the flow frame so ripples elongate
    // along the direction of travel (the tangential component is scaled down).
    const rippleN = normalize(
      vec3(nSample.r.mul(1.2), float(1.0), nSample.g.mul(1.2))
    )

    // ── View / screen setup ──
    const viewDir = normalize(vec3(uCameraDirection).negate())
    const screenUV = vClipPos.xy.mul(0.5).add(0.5)

    // ── Sky + planar reflection (same shape as ocean, condensed) ──
    // Use an almost-flat normal for the sky reflection: the ripple amplitude
    // gets multiplied by `cloudHeight / reflectDir.y` when projected onto the
    // cloud plane, so even small wobbles turn into huge UV jitter that
    // triggers aggressive mipmapping and averages the photo to a flat tone.
    const reflNormal = normalize(mix(vec3(0, 1, 0), rippleN, 0.05))
    const reflectDir = reflect(viewDir.negate(), reflNormal)
    const skyY = clamp(reflectDir.y.mul(0.5).add(0.5), 0.0, 1.0)

    const nightFactor = float(1).sub(smoothstep(float(-0.15), float(0.05), sunY))
    const twilightFactor = smoothstep(float(-0.15), float(0.0), sunY).mul(
      float(1).sub(smoothstep(float(0.05), float(0.3), sunY))
    )
    const dayFactor = smoothstep(float(0.05), float(0.3), sunY)

    const hazeColorBase = vec3(0.04, 0.06, 0.12)
      .mul(nightFactor)
      .add(vec3(0.7, 0.35, 0.15).mul(twilightFactor))
      .add(vec3(0.45, 0.62, 0.82).mul(dayFactor))
    const zenithColor = vec3(0.02, 0.04, 0.1)
      .mul(nightFactor)
      .add(vec3(0.15, 0.1, 0.25).mul(twilightFactor))
      .add(vec3(0.12, 0.35, 0.8).mul(dayFactor))
    const groundColor = vec3(0.02, 0.03, 0.06)
      .mul(nightFactor)
      .add(vec3(0.12, 0.06, 0.04).mul(twilightFactor))
      .add(vec3(0.08, 0.12, 0.15).mul(dayFactor))

    const sunsetFactor = smoothstep(float(-0.05), float(0.0), sunY).mul(
      float(1).sub(smoothstep(float(0.0), float(0.3), sunY))
    )
    const hazeColor = mix(
      hazeColorBase,
      uSunColor.rgb.mul(0.6),
      sunsetFactor.mul(0.5)
    )
    const skyReflection = mix(
      mix(groundColor, hazeColor, smoothstep(float(0), float(0.35), skyY)),
      zenithColor,
      smoothstep(float(0.35), float(0.7), skyY)
    ).toVar()

    const sunDot = max(dot(reflectDir, vec3(uSunDirection)), 0.0)
    skyReflection.addAssign(uSunColor.rgb.mul(pow(sunDot, float(8)).mul(0.25)))

    // ── Sky photo reflection ──
    // There's no skybox; project the reflection ray onto an imagined cloud
    // plane and sample the sky-cloud photo directly as the reflected color.
    // cloudPlane UV = `worldXZ + reflectDir.xz * h / reflectDir.y` — without
    // the worldXZ term every pixel samples almost the same point (viewDir is
    // a single uniform here), so the sky would look like a uniform smear
    // instead of distinct patches. Height + scale are sized so the
    // reflection offset stays within one mirror tile of the image.
    const cloudHeight = float(150)
    const cloudFreeY = max(reflectDir.y, float(0.15))
    const cloudPlane = vWorldPos.xz.add(
      reflectDir.xz.mul(cloudHeight.div(cloudFreeY))
    )
    // Smaller divisor = more of the photo per camera view. Too small (~1/10)
    // and the steep UV gradient forces the lowest mip → milky average.
    const CLOUD_UV_SCALE = 1 / 30
    const cloudUV = cloudPlane
      .mul(CLOUD_UV_SCALE)
      .add(vec2(uTime.mul(0.0015), uTime.mul(0.0008)))
    const photoSky = cloudTex.sample(cloudUV).rgb
    // Contrast boost: pow curve pushes sky mid-tones toward dark while
    // leaving near-white clouds intact. Higher exponent = deeper dark sky
    // vs bright clouds separation.
    const contrastedSky = pow(photoSky, vec3(2.0, 2.0, 2.0))
    // Photo has no ground/twilight/night variants — only apply during day
    // and fade out toward the horizon where the procedural gradient wins.
    const photoGate = smoothstep(float(0.15), float(0.45), skyY).mul(dayFactor)
    skyReflection.assign(
      mix(skyReflection, contrastedSky, photoGate.mul(0.95))
    )

    // Planar entity reflection
    const reflUV = vec2(screenUV.x, float(1.0).sub(screenUV.y))
    const reflectionSample = reflectionTex.sample(
      clamp(reflUV.add(rippleN.xz.mul(0.01)), 0.0, 1.0)
    )
    skyReflection.assign(
      mix(skyReflection, reflectionSample.rgb, reflectionSample.a.mul(0.5))
    )

    // ── Specular + sun sparkle ──
    const specNormal = normalize(mix(vec3(0, 1, 0), rippleN, 0.3))
    const halfDir = normalize(vec3(uSunDirection).add(viewDir))
    const NdotH = max(dot(specNormal, halfDir), 0.0)
    const specular = uSunColor.rgb
      .mul(pow(NdotH, float(128)).mul(0.35))
      .toVar()

    const sparkleT = uTime.mul(0.05)
    const sp1 = normalMapTex.sample(
      vWorldPos.xz.mul(0.55).add(flow.mul(sparkleT))
    ).r
    const sp2 = normalMapTex.sample(
      vWorldPos.xz.mul(0.9).sub(flow.mul(sparkleT.mul(0.6)))
    ).g
    const sunSparkleStrength = smoothstep(
      float(0),
      float(0.15),
      sunY
    ).mul(float(0.3).add(float(0.7).mul(sunY)))
    const moonSparkleStrength = float(1)
      .sub(smoothstep(float(-0.05), float(0.05), sunY))
      .mul(0.15)
      .mul(smoothstep(float(0), float(0.1), uMoonBrightness))
    const sparkle = smoothstep(float(1.35), float(1.5), sp1.add(sp2))
      .mul(3.0)
      .mul(depthFactor)
      .mul(max(sunSparkleStrength, moonSparkleStrength))
    specular.addAssign(uSunColor.rgb.mul(sparkle))

    // ── Fresnel mix with sky ──
    const fresnelViewDir = normalize(vec3(viewDir.x, float(0.15), viewDir.z))
    const NdotV = max(dot(rippleN, fresnelViewDir), 0.0)
    const rippleBright = mix(
      float(0.85),
      float(1.2),
      pow(float(1).sub(NdotV), float(1.5))
    )
    waterColor.mulAssign(rippleBright)

    // Deep navy body with moderate reflection; fresnel still boosts
    // reflectivity at grazing angles so the far surface reads brighter.
    const fresnel = pow(float(1).sub(NdotV), float(2)).mul(0.5)
    const reflectionMix = clamp(float(0.35).add(fresnel), 0.0, 0.9)
    const color = mix(waterColor, skyReflection, reflectionMix)
      .add(specular.mul(depthFactor))
      .toVar()

    color.assign(mix(color, reflectionSample.rgb, reflectionSample.a.mul(0.3)))

    // Night darken pass
    const nightDarken = smoothstep(float(-0.05), float(0.1), sunY)
      .mul(0.75)
      .add(0.25)
    color.mulAssign(nightDarken)

    // ── Alpha ──
    // River water is clearer near the banks — fade alpha from the channel
    // toward the edge so gravel / sand show through instead of a hard cut.
    const edgeFade = smoothstep(float(0.55), float(1.0), bankFactor)
    const alpha = clamp(float(0.92).sub(edgeFade.mul(0.75)), 0.0, 1.0)

    return vec4(color, alpha)
  })()

  // ── Build Material ────────────────────────────────────

  const material = new NodeMaterial()
  material.transparent = true
  material.depthWrite = false
  material.side = THREE.DoubleSide
  material.vertexNode = positionNode
  material.fragmentNode = fragmentNode

  return {
    material,
    uniforms: {
      uTime,
      uSunDirection,
      uSunColor,
      uCameraDirection,
      uMoonBrightness,
      uReflectionMap: reflectionTex,
      uNormalMap: normalMapTex,
    },
  }
}
