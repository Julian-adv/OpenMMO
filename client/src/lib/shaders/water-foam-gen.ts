import * as THREE from 'three'

/**
 * Load the water foam texture.
 * Returns a RepeatWrapping texture suitable for shore foam bands.
 */
export async function loadFoamTexture(): Promise<THREE.Texture> {
  const loader = new THREE.TextureLoader()
  const tex = await loader.loadAsync('/textures/13843.png')
  tex.wrapS = THREE.RepeatWrapping
  tex.wrapT = THREE.RepeatWrapping
  return tex
}
