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

/** Cache: texture name → extracted textures (without tile scale) */
interface CachedTextures {
  map: THREE.Texture
  normalMap?: THREE.Texture
  orm?: THREE.Texture
}

const textureCache = new Map<string, CachedTextures>()
const inflightTextures = new Map<string, Promise<CachedTextures>>()

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
    const glbLoader = new GLTFLoader()
    const gltf = await glbLoader.loadAsync(`/textures/${textureName}.glb`)
    const textures = extractTextures(gltf)
    textureCache.set(textureName, textures)
    inflightTextures.delete(textureName)
    return textures
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
