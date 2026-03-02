import * as THREE from 'three'

/**
 * Load the water foam texture.
 * Returns a RepeatWrapping texture suitable for shore foam bands.
 */
export async function loadFoamTexture(): Promise<THREE.Texture> {
  const loader = new THREE.TextureLoader()
  const tex = await loader.loadAsync('/textures/13843.jpg')
  tex.wrapS = THREE.RepeatWrapping
  tex.wrapT = THREE.RepeatWrapping
  return tex
}

/**
 * Load the water surface texture.
 * Returns a RepeatWrapping texture for the overall water surface.
 */
export async function loadSurfaceTexture(): Promise<THREE.Texture> {
  const loader = new THREE.TextureLoader()
  const tex = await loader.loadAsync('/textures/4141.jpg')
  tex.wrapS = THREE.RepeatWrapping
  tex.wrapT = THREE.RepeatWrapping
  return tex
}
