import * as THREE from 'three'

const textureLoader = new THREE.TextureLoader()

export function loadAlphaTexture(url: string): Promise<THREE.Texture> {
  return textureLoader.loadAsync(url)
}

export const loadFlowerColorTexture = () =>
  loadAlphaTexture('/textures/flowerx4.png')

// ── Splatmap R-channel vegetation subtype ranges ─────────
export const SHORT_GRASS_R_MIN = 230
export const SHORT_GRASS_R_MAX = 239
export const TALL_GRASS_R_MIN = 240
export const TALL_GRASS_R_MAX = 249

// ── Wind state snapshot (shared with particle systems) ───
export interface WindState {
  windDirX: number
  windDirZ: number
  /** Wind strength multiplier (0.3 .. 1.0) */
  windStrength: number
  time: number
}

// ── Grass material configuration ─────────────────────────
export interface GrassMaterialConfig {
  baseColor?: [number, number, number]
  tipColor?: [number, number, number]
  windStrength?: number
  windFrequency?: number
  widthScaleMin?: number
  widthScaleExtent?: number
  heightScaleMin?: number
  heightScaleExtent?: number
  interactionRadius?: number
  interactionStrength?: number
  alphaMap?: THREE.Texture
  /** Color texture for the billboard. When set, the texture color is used
   *  directly and alpha is derived from the texture. */
  colorMap?: THREE.Texture
  /** Atlas grid size (e.g. 2 for a 2×2 atlas). Each instance randomly picks
   *  one sub-tile by offsetting UVs. Only used with colorMap. */
  atlasGrid?: number
  /** Roughness at blade tip (default 0.18). Lower = sharper specular glint. */
  tipRoughness?: number
}

export const TALL_GRASS_CONFIG: GrassMaterialConfig = {
  baseColor: [0.012, 0.035, 0.01],
  tipColor: [0.04, 0.09, 0.02],
  windStrength: 0.07,
  widthScaleMin: 0.6,
  widthScaleExtent: 0.6,
  interactionRadius: 2.0,
  interactionStrength: 0.35,
  tipRoughness: 0.32,
}

export const FLOWER_CONFIG: GrassMaterialConfig = {
  baseColor: [0.02, 0.06, 0.015],
  tipColor: [0.06, 0.12, 0.03],
  windStrength: 0.015,
  widthScaleMin: 0.8,
  widthScaleExtent: 0.25,
  heightScaleMin: 0.6,
  heightScaleExtent: 0.25,
  interactionRadius: 1.5,
  interactionStrength: 0.12,
  atlasGrid: 2,
}
