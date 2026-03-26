import * as THREE from 'three'
import { loadGLB } from '../utils/gltfCache'

// ── Grass billboard geometry from GLB ─────────────────────
// Loads grassLODs.glb and extracts the named LOD mesh geometry.
// UV y is flipped so that y=0 at base, y=1 at tip (matching our shader convention).

async function loadLODGeometry(
  lodName: string,
  url = '/models/grassLODs.glb',
  scale: number | [number, number, number] = 5
): Promise<THREE.BufferGeometry> {
  const gltf = await loadGLB(url)
  let found: THREE.BufferGeometry | null = null
  gltf.scene.traverse((child) => {
    if (child instanceof THREE.Mesh && child.name.includes(lodName)) {
      found = child.geometry
    }
  })
  if (!found) {
    throw new Error(`${lodName} mesh not found in ${url}`)
  }
  // Clone to avoid mutating the cached GLTF geometry
  const geometry = (found as THREE.BufferGeometry).clone()
  const [sx, sy, sz] = Array.isArray(scale) ? scale : [scale, scale, scale]
  geometry.scale(sx, sy, sz)

  // Flip UV y: GLB has y=1 at base, y=0 at tip → our convention y=0 base, y=1 tip
  const uvAttr = geometry.getAttribute('uv')
  if (uvAttr) {
    for (let i = 0; i < uvAttr.count; i++) {
      uvAttr.setY(i, 1 - uvAttr.getY(i))
    }
    uvAttr.needsUpdate = true
  }

  return geometry
}

export function loadFlowerBillboardGeometry(
  url = '/models/grassLODs.glb',
  scale: number | [number, number, number] = [3, 5.5, 3]
): Promise<THREE.BufferGeometry> {
  return loadLODGeometry('LOD02', url, scale)
}

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
  windStrength: 0.04,
  widthScaleMin: 0.8,
  widthScaleExtent: 0.5,
  heightScaleMin: 0.6,
  heightScaleExtent: 0.5,
  interactionRadius: 1.5,
  interactionStrength: 0.12,
  atlasGrid: 2,
}
