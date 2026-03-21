/**
 * housing-textures.ts — Texture catalog and material management for the housing system.
 *
 * Loads PBR textures from GLB files (reusing the splatLayerLoader pipeline)
 * and provides per-texture MeshStandardMaterial instances shared across all houses.
 */
import * as THREE from 'three'
import { loadSplatLayer } from './splatLayerLoader'

export interface HousingTextureEntry {
  label: string
  glb: string
  fallbackColor: number
  /** UV scale multiplier — smaller = larger tiles. Default 1.0 */
  uvScale?: number
}

/** Shared texture catalog for walls, floors, and roofs. */
export const HOUSING_TEXTURES: HousingTextureEntry[] = [
  { label: 'Stone', glb: 'rocky_terrain_02_1k', fallbackColor: 0x888888 },
  {
    label: 'Brick',
    glb: 'red_laterite_soil_stones_1k',
    fallbackColor: 0xa85032,
  },
  { label: 'Wood', glb: 'housing/planks_brown_10_1k', fallbackColor: 0x8b6914 },
  { label: 'Marble', glb: 'housing/marble_01_1k', fallbackColor: 0xe0d8cc },
  { label: 'Plank', glb: 'housing/wood_planks_1k', fallbackColor: 0x9e7c4e },
  {
    label: 'Dark Wood',
    glb: 'housing/dark_wooden_planks_1k',
    fallbackColor: 0x4a3728,
  },
  {
    label: 'Weathered',
    glb: 'housing/weathered_planks_1k',
    fallbackColor: 0x8a8070,
  },
  {
    label: 'Log Wall',
    glb: 'housing/wood_trunk_wall_1k',
    fallbackColor: 0x7a5c3a,
  },
  { label: 'Shutter', glb: 'housing/wood_shutter_1k', fallbackColor: 0x6b5a3e },
  {
    label: 'Plank Wall',
    glb: 'housing/wood_plank_wall_1k',
    fallbackColor: 0x8b7355,
  },
  {
    label: 'Clay Roof',
    glb: 'housing/clay_roof_tiles_02_1k',
    fallbackColor: 0xb86b4a,
    uvScale: 0.3,
  },
]

/** Per-texture-index material cache (module-level singleton). */
const materialCache = new Map<number, THREE.MeshStandardMaterial>()

/**
 * Get or create a MeshStandardMaterial for the given texture index.
 * Before textures are loaded, uses fallback color. After loading,
 * the material is updated in-place with PBR maps.
 */
export function getHousingMaterial(
  textureIndex: number
): THREE.MeshStandardMaterial {
  const idx = textureIndex % HOUSING_TEXTURES.length
  let mat = materialCache.get(idx)
  if (!mat) {
    const entry = HOUSING_TEXTURES[idx]
    mat = new THREE.MeshStandardMaterial({
      color: entry.fallbackColor,
      side: THREE.FrontSide,
      roughness: 0.85,
      metalness: 0.0,
    })
    materialCache.set(idx, mat)
  }
  return mat
}

let _initPromise: Promise<void> | null = null

/**
 * Load all housing textures from GLB files and apply them to cached materials.
 * Safe to call multiple times — subsequent calls return the same promise.
 */
export function initHousingTextures(): Promise<void> {
  if (_initPromise) return _initPromise

  _initPromise = (async () => {
    const promises = HOUSING_TEXTURES.map(async (entry, idx) => {
      try {
        const layer = await loadSplatLayer(entry.glb, 1.0)
        const mat = getHousingMaterial(idx)

        const scale = entry.uvScale ?? 1.0
        const applyRepeat = (tex: THREE.Texture) => {
          if (scale !== 1.0) tex.repeat.set(scale, scale)
        }

        mat.map = layer.map
        applyRepeat(layer.map)
        if (layer.normalMap) {
          mat.normalMap = layer.normalMap
          applyRepeat(layer.normalMap)
        }
        if (layer.orm) {
          // ORM packed: R=AO, G=roughness, B=metallic
          mat.roughnessMap = layer.orm
          mat.metalnessMap = layer.orm
          mat.aoMap = layer.orm
          applyRepeat(layer.orm)
        }

        // Switch from fallback color to texture-driven color
        mat.color.set(0xffffff)
        mat.needsUpdate = true
      } catch (e) {
        console.warn(`[housing] Failed to load texture "${entry.glb}":`, e)
        // Material keeps its fallback color
      }
    })

    await Promise.all(promises)
  })()

  return _initPromise
}

/** Generate preview data URLs from loaded textures. Returns null for unloaded entries. */
export function getTexturePreviewUrls(): (string | null)[] {
  const canvas = document.createElement('canvas')
  canvas.width = 32
  canvas.height = 32
  const ctx = canvas.getContext('2d')!
  return HOUSING_TEXTURES.map((_, idx) => {
    const mat = materialCache.get(idx)
    if (!mat?.map?.image) return null
    ctx.clearRect(0, 0, 32, 32)
    const img = mat.map.image
    const entry = HOUSING_TEXTURES[idx]
    const cropSize =
      (Math.min(img.width, img.height) / 2) * (entry.uvScale ?? 1.0)
    ctx.drawImage(img, 0, 0, cropSize, cropSize, 0, 0, 32, 32)
    return canvas.toDataURL()
  })
}

/** Dispose all cached housing materials. Call on layer teardown. */
export function disposeHousingMaterials() {
  for (const mat of materialCache.values()) {
    mat.dispose()
  }
  materialCache.clear()
  _initPromise = null
}
