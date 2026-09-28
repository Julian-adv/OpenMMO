import { readFileSync } from 'node:fs'
import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { createGLTFLoader } from './gltfCache'

/** Parses a GLB under client/public with placeholder materials, since node has no image decoder. */
export function loadHeadlessGlb(publicPath: string): Promise<GLTF> {
  const file = readFileSync(
    new URL(`../../../public/${publicPath}`, import.meta.url)
  )
  return createGLTFLoader()
    .register(() => ({
      name: 'headless-materials',
      loadMaterial: async () => new THREE.MeshBasicMaterial(),
    }))
    .parseAsync(
      file.buffer.slice(file.byteOffset, file.byteOffset + file.byteLength),
      ''
    )
}
