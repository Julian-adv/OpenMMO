import * as THREE from 'three'

/**
 * Load the water normal map texture (waternormals.jpg from Three.js examples).
 * Returns a RepeatWrapping texture suitable for multi-sample blending.
 */
export async function loadWaterNormalMap(): Promise<THREE.Texture> {
  const loader = new THREE.TextureLoader()
  const tex = await loader.loadAsync('/textures/waternormals.jpg')
  tex.wrapS = THREE.RepeatWrapping
  tex.wrapT = THREE.RepeatWrapping
  return tex
}
