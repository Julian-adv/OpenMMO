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
  /** UI display order — lower values appear first. Defaults to array index. */
  sortOrder?: number
  /** Map UV 0→1 per wall segment (no tiling). Default false. */
  fitSegment?: boolean
  /** Internal texture — hidden from the user-facing texture picker. */
  internal?: boolean
}

/** Shared texture catalog for walls, floors, and roofs. */
export const HOUSING_TEXTURES: HousingTextureEntry[] = [
  // Stone
  {
    label: 'Stone',
    glb: 'rocky_terrain_02_1k',
    fallbackColor: 0x888888,
    sortOrder: 0,
  },
  {
    label: 'Brick',
    glb: 'red_laterite_soil_stones_1k',
    fallbackColor: 0xa85032,
    sortOrder: 1,
  },
  {
    label: 'Marble',
    glb: 'housing/marble_01_1k',
    fallbackColor: 0xe0d8cc,
    sortOrder: 2,
  },
  // Wood
  {
    label: 'Wood',
    glb: 'housing/planks_brown_10_1k',
    fallbackColor: 0x8b6914,
    sortOrder: 10,
  },
  {
    label: 'Plank',
    glb: 'housing/wood_planks_1k',
    fallbackColor: 0x9e7c4e,
    sortOrder: 11,
  },
  {
    label: 'Dark Wood',
    glb: 'housing/dark_wooden_planks_1k',
    fallbackColor: 0x4a3728,
    sortOrder: 12,
  },
  {
    label: 'Weathered',
    glb: 'housing/weathered_planks_1k',
    fallbackColor: 0x8a8070,
    sortOrder: 13,
  },
  {
    label: 'Log Wall',
    glb: 'housing/wood_trunk_wall_1k',
    fallbackColor: 0x7a5c3a,
    sortOrder: 14,
  },
  {
    label: 'Shutter',
    glb: 'housing/wood_shutter_1k',
    fallbackColor: 0x6b5a3e,
    sortOrder: 15,
  },
  {
    label: 'Plank Wall',
    glb: 'housing/wood_plank_wall_1k',
    fallbackColor: 0x8b7355,
    sortOrder: 16,
  },
  // Clay
  {
    label: 'Clay Roof',
    glb: 'housing/clay_roof_tiles_02_1k',
    fallbackColor: 0xb86b4a,
    uvScale: 0.3,
    sortOrder: 20,
  },
  {
    label: 'Clay Roof 2',
    glb: 'housing/clay_roof_tiles_03_1k',
    fallbackColor: 0xc47850,
    uvScale: 0.3,
    sortOrder: 21,
  },
  // Thatch
  {
    label: 'Reed Roof',
    glb: 'housing/reed_roof_03_1k',
    fallbackColor: 0x9a8a60,
    uvScale: 0.3,
    sortOrder: 30,
  },
  {
    label: 'Grey Roof',
    glb: 'housing/grey_roof_tiles_02_1k',
    fallbackColor: 0x707070,
    uvScale: 0.3,
    sortOrder: 22,
  },
  {
    label: 'Red Brick',
    glb: 'housing/red_brick_1k',
    fallbackColor: 0xb04030,
    uvScale: 0.5,
    sortOrder: 3,
  },
  {
    label: 'Medieval Stone',
    glb: 'housing/medieval_blocks_03_1k',
    fallbackColor: 0x8a8070,
    sortOrder: 4,
  },
  {
    label: 'Sandstone',
    glb: 'housing/sandstone_blocks_04_1k',
    fallbackColor: 0xc4a878,
    uvScale: 0.5,
    sortOrder: 5,
  },
  {
    label: 'Plaster Wall',
    glb: 'housing/worn_mossy_plasterwall_1k',
    fallbackColor: 0x7a7060,
    sortOrder: 6,
  },
  {
    label: 'Plaster & Wood',
    glb: 'housing/beige_wall_001_1k',
    fallbackColor: 0xe0d8cc,
    sortOrder: 7,
    fitSegment: true,
  },
  // Fabric
  {
    label: 'Linen',
    glb: 'housing/rough_linen_1k',
    fallbackColor: 0xc8b898,
    sortOrder: 40,
  },
  // Internal (not user-selectable)
  {
    label: 'Shutter Panel',
    glb: 'housing/shutter_panel_1k',
    fallbackColor: 0x8a7050,
    fitSegment: true,
    internal: true,
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
        const applyWrap = (tex: THREE.Texture) => {
          if (scale !== 1.0) tex.repeat.set(scale, scale)
          if (entry.fitSegment) {
            tex.wrapS = THREE.ClampToEdgeWrapping
            tex.wrapT = THREE.ClampToEdgeWrapping
            tex.needsUpdate = true
          }
        }

        mat.map = layer.map
        applyWrap(layer.map)
        if (layer.normalMap) {
          mat.normalMap = layer.normalMap
          applyWrap(layer.normalMap)
        }
        if (layer.orm) {
          // ORM packed: R=AO, G=roughness, B=metallic
          mat.roughnessMap = layer.orm
          mat.metalnessMap = layer.orm
          mat.aoMap = layer.orm
          applyWrap(layer.orm)
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
