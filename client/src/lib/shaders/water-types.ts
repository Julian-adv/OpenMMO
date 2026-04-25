import * as THREE from 'three'
import type { NodeMaterial } from 'three/webgpu'
import { vec2, float, smoothstep, max, clamp } from 'three/tsl'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any // TSL node — broad type for shared helper params

// ─── Interfaces ─────────────────────────────────────────

export interface WaterMaterialOptions {
  heightmapTexture: THREE.DataTexture
  normalMap: THREE.Texture
  foamMap: THREE.Texture
  causticsMap: THREE.Texture
  refractionMap?: THREE.Texture | null
  reflectionMap?: THREE.Texture | null
  wetnessMap?: THREE.Texture | null
  /** Per-tile splatmap. Byte 1 (G channel) stores river proximity —
   *  0 on a river center, 255 past the foam-suppress radius. Sampled by
   *  the water shader to attenuate shoreline foam at estuaries. */
  splatMap?: THREE.Texture | null
}

export interface WaterMaterialUniforms {
  uTime: { value: number }
  uSunDirection: { value: THREE.Vector3 }
  uSunColor: { value: THREE.Color }
  uCameraDirection: { value: THREE.Vector3 }
  uMoonBrightness: { value: number }
  uRefractionMap: { value: THREE.Texture }
  uReflectionMap: { value: THREE.Texture }
  uHeightmapTexture: { value: THREE.Texture }
  uNormalMap: { value: THREE.Texture }
  uFoamMap: { value: THREE.Texture }
  uCausticsMap: { value: THREE.Texture }
  uWetnessMap: { value: THREE.Texture }
  uSplatMap: { value: THREE.Texture }
  uCaptureMode: { value: number }
  uWaveA: { value: THREE.Vector4 }
  uWaveB: { value: THREE.Vector4 }
  uWaveC: { value: THREE.Vector4 }
}

export interface WaterMaterialResult {
  material: NodeMaterial
  updateWaveDirections: (elapsed: number) => void
  uniforms: WaterMaterialUniforms
}

// ─── Fallback Textures ─────────────────────────────────

/** Module-level fallback texture — shared across all water materials for pooling safety. */
export const waterFallbackTex = new THREE.DataTexture(
  new Uint8Array([128, 128, 128, 255]),
  1,
  1,
  THREE.RGBAFormat
)
waterFallbackTex.needsUpdate = true

/** Wetness fallback (RGBA8, r=0) — matches StorageTexture default format. */
export const waterWetnessFallbackTex = new THREE.DataTexture(
  new Uint8Array([0, 0, 0, 255]),
  1,
  1,
  THREE.RGBAFormat
)
waterWetnessFallbackTex.needsUpdate = true

/** Splatmap fallback (RGBA8) with G=255 — "no river nearby", so the
 *  water shader leaves foam at full strength when no splatmap is bound. */
export const waterSplatFallbackTex = new THREE.DataTexture(
  new Uint8Array([0, 255, 0, 0]),
  1,
  1,
  THREE.RGBAFormat
)
waterSplatFallbackTex.needsUpdate = true

/** Heightmap-compatible fallback (RedFormat + FloatType) — must match the format
 *  the heightmap TextureNode was compiled with, otherwise WebGPU bind groups fail. */
export const waterHeightFallbackTex = new THREE.DataTexture(
  new Float32Array([0]),
  1,
  1,
  THREE.RedFormat,
  THREE.FloatType
)
waterHeightFallbackTex.needsUpdate = true

// ─── Heightmap Sampling ───────────────────────────────

/** 65×65 heightmap covers a 64m tile with vertices on texel CENTERS, so
 *  vertex UVs in [0,1] need a half-texel inset on each side to land on
 *  centers rather than edges. Sea and river materials must agree on this
 *  alignment so a fragment in the river ribbon at the same world XZ as
 *  a sea fragment reads the identical bed height — that's what keeps
 *  their alpha edges on the same shoreline contour. */
export function toHeightmapUV(uvCoord: N): N {
  return uvCoord.mul(64.0 / 65.0).add(0.5 / 65.0)
}

// ─── Cloud Texture ─────────────────────────────────────

// Sky-cloud reference photo (see doc/ASSETS.md). Non-tileable so we
// MirroredRepeat to hide seams across the projected cloud plane.
// Shared between river and water (sea) materials so they read the same
// sky when sampled with the cloud-plane projection trick.
let _cloudTex: THREE.Texture | null = null
export function getCloudTexture(): THREE.Texture {
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

// ─── Cloud Plane Sampling ──────────────────────────────

// Project the reflection ray onto a virtual cloud plane and sample the
// sky photo. Caller picks the reflectDir — river uses its sky reflectDir
// directly; sea uses a dedicated almost-flat normal because the rippled
// `reflNormal` (mix factor 0.3) would blow the projected UV gradient out
// and force the lowest mip into a milky smear.
//
// Returns `cloudColor` (squared for cloud/sky contrast) and `cloudWeight`
// (horizon fade × dayFactor — photo has no night/twilight variants).
//
// `cloudFreeY` floor of 0.25 (not 0.15) prevents mip saturation at near-
// grazing angles where `cloudHeight / reflectDir.y` blows the UV up.
const CLOUD_HEIGHT = 150
const CLOUD_UV_SCALE = 1 / 30
const CLOUD_DRIFT_RATE: readonly [number, number] = [0.0015, 0.0008]

export function sampleCloudPhoto(
  cloudReflectDir: N,
  worldXZ: N,
  uTime: N,
  dayFactor: N,
  cloudTex: N
): { cloudColor: N; cloudWeight: N } {
  const cloudSkyY = clamp(cloudReflectDir.y.mul(0.5).add(0.5), 0.0, 1.0)
  const cloudFreeY = max(cloudReflectDir.y, float(0.25))
  const cloudPlane = worldXZ.add(
    cloudReflectDir.xz.mul(float(CLOUD_HEIGHT).div(cloudFreeY))
  )
  const cloudUV = cloudPlane
    .mul(CLOUD_UV_SCALE)
    .add(
      vec2(float(CLOUD_DRIFT_RATE[0]), float(CLOUD_DRIFT_RATE[1])).mul(uTime)
    )
  const photoSky = cloudTex.sample(cloudUV).rgb
  // x*x is materially cheaper than pow(x, 2) on WebGPU (1 MAD vs ~3 ops).
  const cloudColor = photoSky.mul(photoSky)
  const cloudWeight = smoothstep(float(0.15), float(0.45), cloudSkyY).mul(
    dayFactor
  )
  return { cloudColor, cloudWeight }
}

// ─── Wave Configuration ────────────────────────────────

export const waveConfigs = [
  {
    angle: Math.random() * Math.PI * 2,
    speed: 0.0013,
    steepness: 0.06,
    wavelength: 20,
  },
  {
    angle: Math.random() * Math.PI * 2,
    speed: 0.0021,
    steepness: 0.04,
    wavelength: 14,
  },
  {
    angle: Math.random() * Math.PI * 2,
    speed: 0.0009,
    steepness: 0.03,
    wavelength: 9,
  },
]
