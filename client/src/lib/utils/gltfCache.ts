import { GLTFLoader } from 'three/examples/jsm/Addons.js'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'

/**
 * Module-level GLB cache shared across all Threlte contexts.
 * Threlte's useLoader cache is scoped to each <Canvas> — when switching
 * from character select to game scene (separate Canvas), GLBs are re-downloaded.
 * This cache persists across Canvas lifecycles.
 */
const cache = new Map<string, GLTF>()
const inflight = new Map<string, Promise<GLTF>>()
const loader = new GLTFLoader()

export function loadGLB(url: string): Promise<GLTF> {
  const cached = cache.get(url)
  if (cached) return Promise.resolve(cached)

  const existing = inflight.get(url)
  if (existing) return existing

  const promise = loader.loadAsync(url).then((gltf) => {
    cache.set(url, gltf)
    inflight.delete(url)
    return gltf
  })
  inflight.set(url, promise)
  return promise
}
