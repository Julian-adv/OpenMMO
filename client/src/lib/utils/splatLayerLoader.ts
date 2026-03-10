import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/Addons.js'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import type { SplatLayer } from '../components/makeSplatStandardMaterial'

export interface LayerConfig {
  texture: string
  tileScale: number
}

export const DEFAULT_LAYER_CONFIGS: [
  LayerConfig,
  LayerConfig,
  LayerConfig,
  LayerConfig,
] = [
  { texture: 'rocky_terrain_02_1k', tileScale: 8.0 }, // R = grass
  { texture: 'gravel_floor_1k', tileScale: 6.0 }, // G = rock
  { texture: 'red_laterite_soil_stones_1k', tileScale: 10.0 }, // B = dirt
  { texture: 'snow_02_1k', tileScale: 4.0 }, // A = snow
]

/** All available splat textures that can be assigned to region layers. */
export const ALL_SPLAT_TEXTURES: { name: string; defaultTileScale: number }[] =
  [
    { name: 'rocky_terrain_02_1k', defaultTileScale: 8.0 },
    { name: 'gravel_floor_1k', defaultTileScale: 6.0 },
    { name: 'red_laterite_soil_stones_1k', defaultTileScale: 10.0 },
    { name: 'snow_02_1k', defaultTileScale: 4.0 },
    { name: 'gravel_road_1k', defaultTileScale: 8.0 },
    { name: 'sandy_gravel_02_1k', defaultTileScale: 8.0 },
    { name: 'fine_sand_material', defaultTileScale: 8.0 },
  ]

/** Atlas set: 2×2 packed textures for diffuse, normal, ORM */
export interface SplatAtlasSet {
  diffuseAtlas: THREE.Texture
  normalAtlas: THREE.Texture | null
  ormAtlas: THREE.Texture | null
}

/** Cache: texture name → extracted textures (without tile scale) */
interface CachedTextures {
  map: THREE.Texture
  normalMap?: THREE.Texture
  orm?: THREE.Texture
}

const textureCache = new Map<string, CachedTextures>()
const inflightTextures = new Map<string, Promise<CachedTextures>>()
const atlasCache = new Map<string, SplatAtlasSet>()

function prepColorTex(t: THREE.Texture | null) {
  if (!t) return null
  t.wrapS = t.wrapT = THREE.RepeatWrapping
  t.anisotropy = 8
  t.colorSpace = THREE.SRGBColorSpace
  t.needsUpdate = true
  return t
}

function prepDataTex(t: THREE.Texture | null) {
  if (!t) return null
  t.wrapS = t.wrapT = THREE.RepeatWrapping
  t.anisotropy = 8
  t.needsUpdate = true
  return t
}

function firstMaterial(gltf: GLTF): THREE.MeshStandardMaterial | null {
  let found: THREE.MeshStandardMaterial | null = null
  gltf.scene.traverse((o: THREE.Object3D) => {
    if (found) return
    if (
      o instanceof THREE.Mesh &&
      o.material instanceof THREE.MeshStandardMaterial
    ) {
      found = o.material
    }
  })
  return found
}

function packORM(
  ao: THREE.Texture | null,
  mr: THREE.Texture | null
): THREE.Texture | null {
  const aoImg = ao?.image as HTMLImageElement | undefined
  const mrImg = mr?.image as HTMLImageElement | undefined
  if (!aoImg && !mrImg) return null

  const w = mrImg?.width || aoImg?.width
  const h = mrImg?.height || aoImg?.height
  if (!w || !h) return null

  const canvas = document.createElement('canvas')
  canvas.width = w
  canvas.height = h
  const ctx = canvas.getContext('2d')!
  ctx.fillStyle = 'rgb(255,255,0)'
  ctx.fillRect(0, 0, w, h)

  if (mrImg) {
    const mrc = document.createElement('canvas')
    mrc.width = w
    mrc.height = h
    const mctx = mrc.getContext('2d')!
    mctx.drawImage(mrImg, 0, 0, w, h)
    const mrData = mctx.getImageData(0, 0, w, h).data

    const imgData = ctx.getImageData(0, 0, w, h)
    const data = imgData.data
    for (let i = 0; i < data.length; i += 4) {
      data[i + 1] = mrData[i + 1] // G = roughness
      data[i + 2] = mrData[i + 2] // B = metallic
    }
    ctx.putImageData(imgData, 0, 0)
  }

  if (aoImg) {
    const aoc = document.createElement('canvas')
    aoc.width = w
    aoc.height = h
    const actx = aoc.getContext('2d')!
    actx.drawImage(aoImg, 0, 0, w, h)
    const aoData = actx.getImageData(0, 0, w, h).data

    const imgData = ctx.getImageData(0, 0, w, h)
    const data = imgData.data
    for (let i = 0; i < data.length; i += 4) {
      data[i + 0] = aoData[i + 0] // R = AO
    }
    ctx.putImageData(imgData, 0, 0)
  }

  const tex = new THREE.CanvasTexture(canvas)
  tex.wrapS = tex.wrapT = THREE.RepeatWrapping
  tex.anisotropy = 8
  tex.flipY = false
  tex.needsUpdate = true
  return tex
}

function extractTextures(gltf: GLTF): CachedTextures {
  const mat = firstMaterial(gltf)
  if (!mat) throw new Error('No MeshStandardMaterial found in GLB')
  const albedo = prepColorTex(mat.map || null)!
  const normal = prepDataTex(mat.normalMap || null) || undefined
  const mr = prepDataTex(mat.roughnessMap || mat.metalnessMap || null)
  const ao = prepDataTex(mat.aoMap || null)
  const orm = packORM(ao, mr) || undefined
  return { map: albedo, normalMap: normal, orm }
}

/** Load a single texture by name, with caching. */
export function loadSplatLayer(
  textureName: string,
  tileScale: number
): Promise<SplatLayer> {
  const cached = textureCache.get(textureName)
  if (cached) return Promise.resolve({ ...cached, tile: tileScale })

  const existing = inflightTextures.get(textureName)
  if (existing) return existing.then((t) => ({ ...t, tile: tileScale }))

  const promise = (async () => {
    try {
      const glbLoader = new GLTFLoader()
      const gltf = await glbLoader.loadAsync(`/textures/${textureName}.glb`)
      const textures = extractTextures(gltf)
      textureCache.set(textureName, textures)
      return textures
    } finally {
      inflightTextures.delete(textureName)
    }
  })()
  inflightTextures.set(textureName, promise)
  return promise.then((t) => ({ ...t, tile: tileScale }))
}

/** Load 4 splat layers from config. Shared textures are loaded only once. */
export function loadSplatLayers(
  configs: [
    LayerConfig,
    LayerConfig,
    LayerConfig,
    LayerConfig,
  ] = DEFAULT_LAYER_CONFIGS
): Promise<[SplatLayer, SplatLayer, SplatLayer, SplatLayer]> {
  return Promise.all(
    configs.map((c) => loadSplatLayer(c.texture, c.tileScale))
  ) as Promise<[SplatLayer, SplatLayer, SplatLayer, SplatLayer]>
}

// ── Atlas building ──────────────────────────────────────────────

/**
 * Border pixels around each sub-texture in the atlas to prevent mipmap bleeding.
 * Filled with wrapping-edge pixels from the source texture.
 */
export const ATLAS_BORDER = 8

/**
 * Draw a single sub-texture into its atlas slot with wrapping borders.
 * The border pixels are taken from the opposite edges of the source (tiling wrap).
 */
function drawWithWrapBorder(
  ctx: CanvasRenderingContext2D,
  img: CanvasImageSource,
  slotX: number,
  slotY: number,
  srcSize: number,
  border: number
) {
  const B = border
  const S = srcSize

  // Main texture
  ctx.drawImage(img, 0, 0, S, S, slotX + B, slotY + B, S, S)

  // Left border: rightmost B columns of source
  ctx.drawImage(img, S - B, 0, B, S, slotX, slotY + B, B, S)
  // Right border: leftmost B columns of source
  ctx.drawImage(img, 0, 0, B, S, slotX + B + S, slotY + B, B, S)
  // Top border: bottom B rows of source
  ctx.drawImage(img, 0, S - B, S, B, slotX + B, slotY, S, B)
  // Bottom border: top B rows of source
  ctx.drawImage(img, 0, 0, S, B, slotX + B, slotY + B + S, S, B)

  // Corners (diagonal wrapping)
  ctx.drawImage(img, S - B, S - B, B, B, slotX, slotY, B, B) // TL ← BR of src
  ctx.drawImage(img, 0, S - B, B, B, slotX + B + S, slotY, B, B) // TR ← BL of src
  ctx.drawImage(img, S - B, 0, B, B, slotX, slotY + B + S, B, B) // BL ← TR of src
  ctx.drawImage(img, 0, 0, B, B, slotX + B + S, slotY + B + S, B, B) // BR ← TL of src
}

/**
 * Pack 4 textures into a 2×2 atlas with wrapping border padding.
 * Each slot is (srcSize + 2*ATLAS_BORDER) px. Atlas total = slot*2 per axis.
 * Layout: [0]=TL, [1]=TR, [2]=BL, [3]=BR
 */
function buildAtlasTexture(
  textures: (THREE.Texture | null | undefined)[],
  fallbackFill: string,
  isColor: boolean
): THREE.Texture | null {
  const hasAny = textures.some((t) => t?.image)
  if (!hasAny) return null

  const firstImg = textures.find((t) => t?.image)?.image as
    | HTMLImageElement
    | HTMLCanvasElement
    | ImageBitmap
    | undefined
  const srcSize =
    (firstImg &&
      ('naturalWidth' in firstImg
        ? firstImg.naturalWidth || firstImg.width
        : firstImg.width)) ||
    1024

  const slotSize = srcSize + ATLAS_BORDER * 2
  const atlasSize = slotSize * 2

  const canvas = document.createElement('canvas')
  canvas.width = atlasSize
  canvas.height = atlasSize
  const ctx = canvas.getContext('2d')!

  ctx.fillStyle = fallbackFill
  ctx.fillRect(0, 0, atlasSize, atlasSize)

  for (let i = 0; i < 4; i++) {
    const tex = textures[i]
    if (tex?.image) {
      const slotX = (i % 2) * slotSize
      const slotY = Math.floor(i / 2) * slotSize
      drawWithWrapBorder(
        ctx,
        tex.image as CanvasImageSource,
        slotX,
        slotY,
        srcSize,
        ATLAS_BORDER
      )
    }
  }

  const atlasTex = new THREE.CanvasTexture(canvas)
  atlasTex.wrapS = atlasTex.wrapT = THREE.ClampToEdgeWrapping
  atlasTex.anisotropy = 8
  atlasTex.flipY = false
  if (isColor) atlasTex.colorSpace = THREE.SRGBColorSpace
  atlasTex.needsUpdate = true
  return atlasTex
}

/** Build a 2×2 atlas set from 4 splat layers. Results are cached by texture UUIDs. */
export function buildSplatAtlas(
  layers: [SplatLayer, SplatLayer, SplatLayer, SplatLayer]
): SplatAtlasSet {
  const key = layers.map((l) => l.map.uuid).join(',')
  const cached = atlasCache.get(key)
  if (cached) return cached

  const diffuseAtlas = buildAtlasTexture(
    layers.map((l) => l.map),
    'rgb(128,128,128)',
    true
  )!

  const normalAtlas = buildAtlasTexture(
    layers.map((l) => l.normalMap),
    'rgb(128,128,255)',
    false
  )

  const ormAtlas = buildAtlasTexture(
    layers.map((l) => l.orm),
    'rgb(255,255,0)',
    false
  )

  const result: SplatAtlasSet = { diffuseAtlas, normalAtlas, ormAtlas }
  atlasCache.set(key, result)
  return result
}
