import { GLTFLoader, type GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'
import { KTX2Loader } from 'three/examples/jsm/loaders/KTX2Loader.js'
import type { WebGPURenderer } from 'three/webgpu'
import { FileLoader, LoadingManager, LoaderUtils } from 'three'
import basisTranscoder from 'three/examples/jsm/libs/basis/basis_transcoder.js?url'
import basisWasm from 'three/examples/jsm/libs/basis/basis_transcoder.wasm?url'

const transcoderManager = new LoadingManager().setURLModifier((url) =>
  url.endsWith('basis_transcoder.js')
    ? basisTranscoder
    : url.endsWith('basis_transcoder.wasm')
      ? basisWasm
      : url
)
const ktx2Loader = new KTX2Loader(transcoderManager).setWorkerLimit(2)
let textureSupport: Promise<void> | undefined
// Share downloads across character selection and game canvases.
const cache = new Map<string, GLTF>()
const inflight = new Map<string, Promise<GLTF>>()
const loader = createGLTFLoader()
const fileLoader = new FileLoader().setResponseType('arraybuffer')

export function createGLTFLoader(): GLTFLoader {
  return new GLTFLoader()
    .setMeshoptDecoder(MeshoptDecoder)
    .setKTX2Loader(ktx2Loader)
}

export function configureGLTFRenderer(renderer: WebGPURenderer): void {
  if (textureSupport) return
  textureSupport = renderer.init().then(() => {
    ktx2Loader.detectSupport(renderer)
  })
  void textureSupport.catch(() => {
    textureSupport = undefined
  })
}

export function loadGLB(url: string): Promise<GLTF> {
  const cached = cache.get(url)
  if (cached) return Promise.resolve(cached)

  const existing = inflight.get(url)
  if (existing) return existing

  // Only the parse needs KTX2 support; download alongside GPU init.
  const promise = Promise.all([fileLoader.loadAsync(url), textureSupport])
    .then(([data]) => loader.parseAsync(data, LoaderUtils.extractUrlBase(url)))
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
