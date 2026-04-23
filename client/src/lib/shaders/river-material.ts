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
type _N = any // TSL node — broad type for internal helper params

export interface RiverMaterialOptions {
  normalMap: THREE.Texture
  reflectionMap?: THREE.Texture | null
  refractionMap?: THREE.Texture | null
}

export interface RiverMaterialUniforms {
  uTime: { value: number }
  uSunDirection: { value: THREE.Vector3 }
  uSunColor: { value: THREE.Color }
  uCameraDirection: { value: THREE.Vector3 }
  uMoonBrightness: { value: number }
  uReflectionMap: { value: THREE.Texture }
  uRefractionMap: { value: THREE.Texture }
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

  // UV distortion amplitude in screen space. Keep modest — values above
  // ~0.15 push samples far enough that the refraction reads as random
  // color flecks rather than a coherent bed with ripples on top.
  const uRefractionStrength = uniform(0.04)

  // ── Texture Nodes ──
  const normalMapTex = texture(options.normalMap)
  const reflectionTex = texture(options.reflectionMap ?? waterFallbackTex)
  const refractionTex = texture(options.refractionMap ?? waterFallbackTex)
  const cloudTex = texture(getCloudTexture())

  // ── Varyings / Attributes ──
  const vWorldPos = varying(vec3(0), 'r_worldPos')
  const vClipPos = varying(vec4(0), 'r_clipPos')
  const vFlowNorm = varying(float(0), 'r_flowNorm')
  const vMouthFactor = varying(float(0), 'r_mouthFactor')

  const aFlowNorm = attribute('flowNorm', 'float')
  const aMouthFactor = attribute('mouthFactor', 'float')

  // ── Vertex Shader ─────────────────────────────────────

  const positionNode = Fn(() => {
    const localPos = vec4(positionLocal, 1.0)
    const worldPos = modelWorldMatrix.mul(localPos).toVar()
    vWorldPos.assign(worldPos.xyz)
    vFlowNorm.assign(aFlowNorm)
    vMouthFactor.assign(aMouthFactor)
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

    // Estuary body-color fade. The river's per-ribbon "depth" is a lateral
    // proxy (1-bankFactor), so the center of the ribbon hard-locks to the
    // deep-river uMidColor regardless of *actual* water depth. In the
    // estuary that center band is much darker than the shoreline sea tone
    // and, while α tapers (1-vMouthFactor), a mid-α fragment stamps a
    // visible dark band onto the coast. Pull the body toward an estuary
    // tint with a quadratic ease-out — `1 - (1-x)^2` — so the color swap
    // front-loads ahead of the α drop while keeping finite derivatives at
    // both ends (a fractional-power curve like pow(x,0.25) has infinite
    // slope at 0 and interpolation produced a visible seam across
    // adjacent triangles, re: earlier `black line` regression).
    const estuaryColor = vec3(0.2, 0.48, 0.42)
    const oneMinus = float(1).sub(vMouthFactor)
    const colorFade = float(1).sub(oneMinus.mul(oneMinus))
    waterColor.assign(mix(waterColor, estuaryColor, colorFade))

    // Night darkening (same shape as ocean)
    const waterNightFactor = smoothstep(float(-0.05), float(0.1), sunY)
      .mul(0.85)
      .add(0.15)
    waterColor.mulAssign(waterNightFactor)

    // ── Channel-aligned normal map ──
    // Sample in mesh UV (uv.x ∈ [0,1] cross-bank, uv.y meters along
    // channel). World-XZ sampling with a per-fragment flow-vector scroll
    // left visible per-triangle seams at curves because flow interpolates
    // as a vector; mesh UV interpolates as two scalars so the texture
    // follows the channel bend smoothly. REF_WIDTH converts uv.x to
    // meters so cross-channel and along-channel tile rates roughly match
    // at typical river widths.
    //
    // Perturbation stays world-axis (R→worldX, G→worldZ) on purpose:
    // rotating into a channel frame ties distortion direction to the
    // channel, which amplifies into visible "clouds flowing" through the
    // cloud-plane projection and kills specular sparkle variety.
    const scrollSpeed = float(0.06).add(vFlowNorm.mul(0.22))
    const REF_WIDTH = float(20.0)
    const meshUV = vec2(uvCoord.x.sub(0.5).mul(REF_WIDTH), uvCoord.y)

    // Two-sample normal-map average → world-axis ripple normal. Used for
    // both the surface ripple (flow-driven scroll) and the sky reflection
    // (opposed scrolls, computed below).
    const buildRippleN = (uv1: _N, uv2: _N): _N => {
      const s = normalMapTex
        .sample(uv1)
        .add(normalMapTex.sample(uv2))
        .mul(0.5)
        .sub(1.0)
      return normalize(vec3(s.r.mul(1.2), float(1.0), s.g.mul(1.2)))
    }

    // Scroll sample point upstream (−V) so the texture appears to flow
    // downstream.
    const tScroll = uTime.mul(scrollSpeed)
    const nUV1 = meshUV.mul(0.45).sub(vec2(float(0), tScroll))
    const nUV2 = meshUV
      .mul(0.27)
      .add(vec2(float(0.3), float(0)))
      .sub(vec2(float(0), tScroll.mul(0.6)))
    const rippleN = buildRippleN(nUV1, nUV2)

    // ── View / screen setup ──
    const viewDir = normalize(vec3(uCameraDirection).negate())
    const screenUV = vClipPos.xy.mul(0.5).add(0.5)
    // Render targets store rows top-down; flip Y once and reuse for
    // both refraction and reflection screen-space sampling below.
    const screenUVFlipped = vec2(screenUV.x, float(1.0).sub(screenUV.y))

    // ── Refraction: show river bottom through shallow bank water ──
    // "Shallow" is proxied by `bankFactor` (no real depth). Two tricks to
    // keep the refraction readable as *water over bed* rather than as
    // bare terrain bleeding through a transparent ribbon:
    //
    //   1) Peak inside the opaque band. The refraction ramp peaks
    //      around bankFactor 0.6, matching the alpha edge-fade start
    //      below — so the strongest refraction sits in still-opaque
    //      water. Peaking later would let the alpha fade reveal
    //      identical raw terrain through the same pixels, masking
    //      the wobble entirely.
    //
    //   2) Tint the sampled bed with a teal absorption filter. Without
    //      this the refracted pixel is indistinguishable from the raw
    //      terrain (same colors, same scale) — the wobble reads as
    //      noise, not as water. The tint makes the refracted region
    //      visibly cooler/greener, so the eye registers it as shallow
    //      clear water over a colored bed.
    const refrDistort = rippleN.xz.mul(uRefractionStrength)
    const refrUV = clamp(screenUVFlipped.add(refrDistort), 0.0, 1.0)
    const rawRefr = refractionTex.sample(refrUV).rgb
    // Shallow-water absorption tint — a classic teal that reads as
    // clear freshwater. Mix weight 0.55 keeps the bed color recognizable
    // (pebbles still look pebbly) while shifting the hue clearly away
    // from raw terrain.
    const waterAbsorbTint = vec3(0.45, 0.75, 0.7)
    const tintedRefr = mix(rawRefr, rawRefr.mul(waterAbsorbTint), 0.55)
    const refrShallow = smoothstep(float(0.15), float(0.6), bankFactor).toVar()
    const refrMouthFade = float(1).sub(vMouthFactor)
    const refrMix = refrShallow.mul(refrMouthFade).mul(0.9).toVar()
    waterColor.assign(mix(waterColor, tintedRefr, refrMix))

    // ── Sky + planar reflection (same shape as ocean, condensed) ──
    // Use an almost-flat normal for the sky reflection: the ripple amplitude
    // gets multiplied by `cloudHeight / reflectDir.y` when projected onto the
    // cloud plane, so even small wobbles turn into huge UV jitter that
    // triggers aggressive mipmapping and averages the photo to a flat tone.
    //
    // A dedicated reflection normal uses *opposed* V scrolls — their
    // translational components largely cancel so the cloud image wobbles
    // in place instead of drifting at ripple speed. The 1.0/0.9 imbalance
    // prevents the wobble collapsing into a directional drift; tune
    // agitation via SHAKE_RATE, not the imbalance.
    //
    // Three speed axes, all decoupled from surface ripple (`scrollSpeed`
    // above):
    //   WOBBLE_SHAKE_RATE — opposed-scroll rate (per-fragment oscillation)
    //   WOBBLE_DRIFT_RATE — constant V offset on both samples (how fast
    //     the wobble pattern rides the current)
    //   CLOUD_DRIFT_RATE  — sky-cloud photo's own drift, applied to
    //     cloudUV below — independent of the water surface
    const WOBBLE_SHAKE_RATE = float(0.05)
    const WOBBLE_DRIFT_RATE = float(0.1)
    const CLOUD_DRIFT_RATE = vec2(float(0.0015), float(0.0008))
    const reflT = uTime.mul(WOBBLE_SHAKE_RATE)
    const reflDrift = vec2(float(0), uTime.mul(WOBBLE_DRIFT_RATE))
    const reflNUV1 = meshUV
      .mul(0.45)
      .sub(vec2(float(0), reflT))
      .sub(reflDrift)
    const reflNUV2 = meshUV
      .mul(0.33)
      .add(vec2(float(0.4), reflT.mul(0.9)))
      .sub(reflDrift)
    const reflRippleN = buildRippleN(reflNUV1, reflNUV2)
    const reflNormal = normalize(mix(vec3(0, 1, 0), reflRippleN, 0.05))
    const reflectDir = reflect(viewDir.negate(), reflNormal)
    const skyY = clamp(reflectDir.y.mul(0.5).add(0.5), 0.0, 1.0)

    const nightFactor = float(1).sub(
      smoothstep(float(-0.15), float(0.05), sunY)
    )
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
      .add(CLOUD_DRIFT_RATE.mul(uTime))
    const photoSky = cloudTex.sample(cloudUV).rgb
    // Contrast boost: pow curve pushes sky mid-tones toward dark while
    // leaving near-white clouds intact. Higher exponent = deeper dark sky
    // vs bright clouds separation.
    const contrastedSky = photoSky.pow(vec3(2.0, 2.0, 2.0))
    // Photo has no ground/twilight/night variants — only apply during day
    // and fade out toward the horizon where the procedural gradient wins.
    const photoGate = smoothstep(float(0.15), float(0.45), skyY).mul(dayFactor)
    skyReflection.assign(mix(skyReflection, contrastedSky, photoGate.mul(0.95)))

    // Planar entity reflection
    const reflectionSample = reflectionTex.sample(
      clamp(screenUVFlipped.add(rippleN.xz.mul(0.01)), 0.0, 1.0)
    )
    skyReflection.assign(
      mix(skyReflection, reflectionSample.rgb, reflectionSample.a.mul(0.5))
    )

    // ── Specular + sun sparkle ──
    const specNormal = normalize(mix(vec3(0, 1, 0), rippleN, 0.3))
    const halfDir = normalize(vec3(uSunDirection).add(viewDir))
    const NdotH = max(dot(specNormal, halfDir), 0.0)
    const specular = uSunColor.rgb.mul(pow(NdotH, float(128)).mul(0.35)).toVar()

    const sparkleT = uTime.mul(0.05)
    // Sparkle UV shares the mesh-UV frame used by the ripple normal so the
    // sparkle pattern scrolls along the channel instead of world axes.
    const sp1 = normalMapTex.sample(
      meshUV.mul(0.55).sub(vec2(float(0), sparkleT))
    ).r
    const sp2 = normalMapTex.sample(
      meshUV.mul(0.9).add(vec2(float(0), sparkleT.mul(0.6)))
    ).g
    const sunSparkleStrength = smoothstep(float(0), float(0.15), sunY).mul(
      float(0.3).add(float(0.7).mul(sunY))
    )
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
    // Estuary reflection fade. River uses ~0.35 base sky reflection while the
    // sea shallow uses ~0.03 — at the mouth that mismatch stamps a blue
    // sky-tinted band against the teal shoreline. Fade the base toward the
    // sea's value on the same `colorFade` curve as the body tint so both
    // transitions land together without introducing a new fade front.
    // Also fade toward the banks — where we just baked in the refraction
    // sample, heavy sky reflection would wash the bed color back out.
    const reflectionBaseEstuary = mix(float(0.35), float(0.03), colorFade)
    const reflectionBase = mix(
      reflectionBaseEstuary,
      float(0.05),
      refrShallow.mul(0.9)
    )
    const reflectionMix = clamp(reflectionBase.add(fresnel), 0.0, 0.9)
    // Specular + sparkle also need to fade at the mouth. They bypass
    // reflectionMix (added directly to color) so without damping the
    // flow-aligned sun sparkle stamps a "river texture" streak into the
    // calm sea surface even after body color and sky reflection match.
    // Use `1 - colorFade = (1 - vMouthFactor)^2` — quadratic ease-in so
    // the inland river keeps its sparkle and only the estuary calms down.
    const estuaryCalm = float(1).sub(colorFade)
    const color = mix(waterColor, skyReflection, reflectionMix)
      .add(specular.mul(depthFactor).mul(estuaryCalm))
      .toVar()

    color.assign(mix(color, reflectionSample.rgb, reflectionSample.a.mul(0.3)))

    // Night darken pass
    const nightDarken = smoothstep(float(-0.05), float(0.1), sunY)
      .mul(0.75)
      .add(0.25)
    color.mulAssign(nightDarken)

    // ── Alpha ──
    // River water is clearer near the banks — fade alpha from the channel
    // toward the edge. The ribbon geometry is built 1.5×+0.5m wider than
    // the baked channel so the outer strip sits over the carved pebble
    // bank that should read as exposed shore, not as submerged bed.
    //
    // Two competing constraints:
    //   • Fade must start *after* the refraction peak (≈0.6) — if alpha
    //     drops while refraction is still strong, direct-terrain view
    //     leaks through the same pixels and the wobble disappears.
    //   • Fade must finish before the outer geometry margin so the
    //     pebble bank behind it is visible — a late/long fade leaves
    //     the water stamped over the shore.
    // A narrow fast ramp threads both: opaque through the refraction
    // band, then a hard drop that uncovers the shore.
    const edgeFade = smoothstep(float(0.55), float(0.85), bankFactor)
    const bankAlpha = clamp(float(0.95).sub(edgeFade.mul(0.95)), 0.0, 1.0)
    // Estuary fade: vMouthFactor=1 where the ribbon sits in open sea.
    // Fully-opaque upstream, fully transparent at the mouth so the sea
    // quad underneath takes over. Coverage of the sea shader's shoreline
    // foam band is handled independently by the sea shader sampling the
    // splatmap's river-proximity byte and attenuating its own foam term.
    const alpha = bankAlpha.mul(float(1).sub(vMouthFactor))

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
      uRefractionMap: refractionTex,
      uNormalMap: normalMapTex,
    },
  }
}
