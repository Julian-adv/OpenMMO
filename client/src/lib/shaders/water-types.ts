import * as THREE from 'three'
import type { NodeMaterial } from 'three/webgpu'

// ─── Interfaces ─────────────────────────────────────────

export interface WaterMaterialOptions {
  heightmapTexture: THREE.DataTexture
  normalMap: THREE.Texture
  foamMap: THREE.Texture
  causticsMap: THREE.Texture
  refractionMap?: THREE.Texture | null
  reflectionMap?: THREE.Texture | null
  wetnessMap?: THREE.Texture | null
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
