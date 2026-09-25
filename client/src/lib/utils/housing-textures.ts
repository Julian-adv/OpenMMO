// Shared housing/dungeon materials, loaded from GLB maps or standalone textures.
import * as THREE from 'three'
import { loadSplatLayer } from './splatLayerLoader'

export interface HousingTextureEntry {
  label: string
  glb: string
  mapUrl?: string
  bumpScale?: number
  roughness?: number
  vertexColors?: boolean
  ghostOpacity?: number
  fallbackColor: number
  /** UV scale multiplier — smaller = larger tiles. Default 1.0 */
  uvScale?: number
  /** UV offset in texture space (0..1 = one image), e.g. to centre a grid. */
  uvOffset?: [number, number]
  /** UI display order — lower values appear first. Defaults to array index. */
  sortOrder?: number
  /** Map UV 0→1 per wall segment (no tiling). Default false. */
  fitSegment?: boolean
  /** Internal texture — hidden from the user-facing texture picker. */
  internal?: boolean
  /** Enable standard alpha-blend transparency (transparent + depthWrite off). */
  transparent?: boolean
  decal?: boolean
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
    transparent: true,
  },
  {
    // Untextured near-black material for dungeon entrance pits. Empty glb
    // skips texture loading; the fallback color is the whole point.
    label: 'Void',
    glb: '',
    fallbackColor: 0x050505,
    internal: true,
  },
  // Appended at array end to keep existing persisted texture indices stable.
  // sortOrder places it in the stone group of the user-facing picker.
  {
    label: 'Stone Path',
    glb: 'housing/grey_stone_path_1k',
    fallbackColor: 0x888888,
    sortOrder: 8,
  },
  {
    // Dungeon entrance door — mapped 0→1 across the panel (fitSegment), so the
    // single garage-door image reads as one door rather than tiling.
    label: 'Garage Door',
    glb: 'dungeon/wooden_garage_door_1k',
    fallbackColor: 0x6b4a2e,
    fitSegment: true,
    internal: true,
  },
  {
    // Keyed dungeon door (DUNGEON_LOCKED_DOOR_TEXTURE_IDX). Door UVs run 0→1
    // across the opening; >1 tightens the grid so the bars read as bars.
    label: 'Rusty Metal Grid',
    glb: 'dungeon/rusty_metal_grid_1k',
    fallbackColor: 0x4a4038,
    uvScale: 1.5,
    uvOffset: [0, 0.16],
    internal: true,
  },
  {
    // Legacy slot retained so persisted texture indices stay stable.
    label: 'Rock Wall 10',
    glb: 'dungeon/rock_wall_10_1k',
    fallbackColor: 0x7d756a,
    uvScale: 0.8,
    internal: true,
  },
  ...[
    { id: 'limestone', color: 0x89867d, roughness: 0.94 },
    { id: 'moss', color: 0x6c7362, roughness: 0.88 },
    { id: 'masonry', color: 0x626763, roughness: 0.93 },
  ].flatMap(({ id, color, roughness }) =>
    ['wall', 'floor'].map((surface) => {
      const masonryWall = id === 'masonry' && surface === 'wall'
      return {
        label: `Cave ${id} ${surface}`,
        glb: '',
        mapUrl: `/textures/dungeon/cave-${id}-${surface}.webp`,
        uvScale: masonryWall ? 2 : 1,
        fallbackColor: color,
        roughness,
        vertexColors: masonryWall,
        ghostOpacity: masonryWall ? 0.22 : undefined,
        bumpScale: masonryWall
          ? 0.012
          : surface === 'wall'
            ? 0.09
            : id === 'masonry'
              ? 0
              : 0.035,
        internal: true,
      }
    })
  ),
  ...['wall-weathering-decals', 'wall-cracks-cobwebs'].map((id) => ({
    label: `Dungeon ${id}`,
    glb: '',
    mapUrl: `/textures/dungeon/${id}.webp`,
    fallbackColor: 0xffffff,
    roughness: 1,
    vertexColors: true,
    transparent: true,
    decal: true,
    internal: true,
  })),
]

/** Per-texture-index material cache (module-level singleton). */
const materialCache = new Map<number, THREE.MeshStandardMaterial>()

/** Semi-transparent ghost material cache — created on demand, synced with base materials. */
const ghostMaterialCache = new Map<number, THREE.MeshStandardMaterial>()

/** Cached materials use a fallback color until their textures load. */
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
      roughness: entry.roughness ?? 0.85,
      vertexColors: entry.vertexColors ?? false,
      metalness: 0.0,
      ...(entry.transparent && { transparent: true, depthWrite: false }),
      ...(entry.decal && {
        visible: false,
        alphaTest: 0.015,
        polygonOffset: true,
        polygonOffsetFactor: -2,
        polygonOffsetUnits: -2,
      }),
    })
    materialCache.set(idx, mat)
  }
  return mat
}

/** Swap between opaque and ghost materials using the mesh's texture index. */
export function setMeshGhost(mesh: THREE.Mesh, ghost: boolean) {
  const idx = mesh.userData.textureIndex
  if (typeof idx !== 'number') return
  mesh.material = ghost ? getGhostHousingMaterial(idx) : getHousingMaterial(idx)
}

/** Shared semi-transparent material for an occluding surface. */
export function getGhostHousingMaterial(
  textureIndex: number
): THREE.MeshStandardMaterial {
  const idx = textureIndex % HOUSING_TEXTURES.length
  let ghost = ghostMaterialCache.get(idx)
  if (!ghost) {
    const base = getHousingMaterial(idx)
    ghost = base.clone()
    ghost.transparent = true
    ghost.depthWrite = false
    ghost.opacity =
      HOUSING_TEXTURES[idx].ghostOpacity ?? (base.transparent ? 0.4 : 0.5)
    ghostMaterialCache.set(idx, ghost)
  }
  return ghost
}

let _initPromise: Promise<void> | null = null

/** Load textures once and update shared materials in place. */
export function initHousingTextures(): Promise<void> {
  if (_initPromise) return _initPromise

  _initPromise = (async () => {
    const promises = HOUSING_TEXTURES.map(async (entry, idx) => {
      if (!entry.glb && !entry.mapUrl) return
      try {
        const layer = entry.mapUrl
          ? { map: await new THREE.TextureLoader().loadAsync(entry.mapUrl) }
          : await loadSplatLayer(entry.glb, 1.0)
        const mat = getHousingMaterial(idx)

        const scale = entry.uvScale ?? 1.0
        const applyWrap = (tex: THREE.Texture) => {
          if (scale !== 1.0) tex.repeat.set(scale, scale)
          if (entry.uvOffset) tex.offset.set(...entry.uvOffset)
          // Set both ways: the loader caches textures, so an entry edited
          // under HMR must not inherit the wrap it had before.
          const wrap = entry.fitSegment
            ? THREE.ClampToEdgeWrapping
            : THREE.RepeatWrapping
          tex.wrapS = wrap
          tex.wrapT = wrap
          tex.needsUpdate = true
        }

        mat.map = layer.map
        layer.map.colorSpace = THREE.SRGBColorSpace
        layer.map.anisotropy = 8
        applyWrap(layer.map)
        if ('normalMap' in layer && layer.normalMap) {
          mat.normalMap = layer.normalMap
          applyWrap(layer.normalMap)
        }
        if ('orm' in layer && layer.orm) {
          // ORM packed: R=AO, G=roughness, B=metallic
          mat.roughnessMap = layer.orm
          mat.metalnessMap = layer.orm
          mat.aoMap = layer.orm
          applyWrap(layer.orm)
        }
        if (entry.bumpScale) {
          mat.bumpMap = layer.map
          mat.bumpScale = entry.bumpScale
        }

        // Switch from fallback color to texture-driven color
        mat.color.set(0xffffff)
        mat.visible = true
        mat.needsUpdate = true

        const oldGhost = ghostMaterialCache.get(idx)
        if (oldGhost) {
          const opacity = oldGhost.opacity
          oldGhost.copy(mat)
          oldGhost.transparent = true
          oldGhost.depthWrite = false
          oldGhost.opacity = opacity
          oldGhost.needsUpdate = true
        }
      } catch (e) {
        console.warn(
          `[housing] Failed to load texture "${entry.mapUrl ?? entry.glb}":`,
          e
        )
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
    const img = mat.map.image as HTMLImageElement
    const entry = HOUSING_TEXTURES[idx]
    const cropSize =
      (Math.min(img.width, img.height) / 2) * (entry.uvScale ?? 1.0)
    ctx.drawImage(img, 0, 0, cropSize, cropSize, 0, 0, 32, 32)
    return canvas.toDataURL()
  })
}

/** Dispose all cached housing materials. Call on layer teardown. */
export function disposeHousingMaterials() {
  for (const [idx, mat] of materialCache) {
    if (HOUSING_TEXTURES[idx].mapUrl) mat.map?.dispose()
    mat.dispose()
  }
  materialCache.clear()
  for (const mat of ghostMaterialCache.values()) {
    mat.dispose()
  }
  ghostMaterialCache.clear()
  _initPromise = null
}
