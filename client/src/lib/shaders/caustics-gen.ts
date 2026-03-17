import * as THREE from 'three'

/**
 * Load the pre-baked caustics texture (256x256 Voronoi distance field).
 * The texture is deterministic so it was pre-generated as a static PNG
 * to avoid blocking the main thread during scene initialization.
 */
export function loadCausticsTexture(): Promise<THREE.Texture> {
  return new Promise((resolve) => {
    const loader = new THREE.TextureLoader()
    const tex = loader.load('/textures/caustics.png', () => resolve(tex))
    tex.wrapS = THREE.RepeatWrapping
    tex.wrapT = THREE.RepeatWrapping
    tex.minFilter = THREE.LinearMipMapLinearFilter
    tex.magFilter = THREE.LinearFilter
  })
}
