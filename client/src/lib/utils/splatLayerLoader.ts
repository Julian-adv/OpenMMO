import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/Addons.js'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import type { SplatLayer } from '../components/makeSplatStandardMaterial'

const GLB_PATHS = [
  { path: '/textures/rocky_terrain_02_1k.glb', tile: 8.0 }, // R = grass
  { path: '/textures/gravel_floor_1k.glb', tile: 6.0 }, // G = rock
  { path: '/textures/red_laterite_soil_stones_1k.glb', tile: 10.0 }, // B = dirt
  { path: '/textures/snow_02_1k.glb', tile: 4.0 }, // A = snow
] as const

let cached: [SplatLayer, SplatLayer, SplatLayer, SplatLayer] | null = null
let inflight: Promise<[SplatLayer, SplatLayer, SplatLayer, SplatLayer]> | null =
  null

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

function toLayer(gltf: GLTF, tile: number): SplatLayer {
  const mat = firstMaterial(gltf)
  if (!mat) throw new Error('No MeshStandardMaterial found in GLB')
  const albedo = prepColorTex(mat.map || null)!
  const normal = prepDataTex(mat.normalMap || null) || undefined
  const mr = prepDataTex(mat.roughnessMap || mat.metalnessMap || null)
  const ao = prepDataTex(mat.aoMap || null)
  const orm = packORM(ao, mr) || undefined
  return { map: albedo, normalMap: normal, orm, tile }
}

/** Load terrain splat layers once. Subsequent calls return the cached result. */
export function loadSplatLayers(): Promise<
  [SplatLayer, SplatLayer, SplatLayer, SplatLayer]
> {
  if (cached) return Promise.resolve(cached)
  if (inflight) return inflight

  inflight = (async () => {
    const glbLoader = new GLTFLoader()
    const gltfs = await Promise.all(
      GLB_PATHS.map((l) => glbLoader.loadAsync(l.path))
    )
    cached = gltfs.map((gltf, i) => toLayer(gltf, GLB_PATHS[i].tile)) as [
      SplatLayer,
      SplatLayer,
      SplatLayer,
      SplatLayer,
    ]
    return cached
  })()
  return inflight
}
