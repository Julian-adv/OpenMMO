import { GLTFLoader, type GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'

// Share downloads across character selection and game canvases.
const cache = new Map<string, GLTF>()
const inflight = new Map<string, Promise<GLTF>>()
const loader = createGLTFLoader()

export function createGLTFLoader(): GLTFLoader {
  return new GLTFLoader().setMeshoptDecoder(MeshoptDecoder)
}

export function loadGLB(url: string): Promise<GLTF> {
  const cached = cache.get(url)
  if (cached) return Promise.resolve(cached)

  const existing = inflight.get(url)
  if (existing) return existing

  const promise = loader
    .loadAsync(url)
    .then((gltf) => {
      cache.set(url, gltf)
      inflight.delete(url)
      return gltf
    })
    .catch((err) => {
      inflight.delete(url)
      throw err
    })
  inflight.set(url, promise)
  return promise
}
