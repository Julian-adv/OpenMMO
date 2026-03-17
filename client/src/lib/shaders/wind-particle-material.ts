import * as THREE from 'three'
import { MeshBasicNodeMaterial } from 'three/webgpu'
import { attribute, texture } from 'three/tsl'
import { loadAlphaTexture } from './grass-material'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any

/** Per-instance opacity attribute name for age-based fade. */
export const PARTICLE_OPACITY_ATTR = 'aParticleOpacity'

/** Load pre-baked particle textures (same pattern as grass/flower). */
export const loadDandelionSeedTexture = () =>
  loadAlphaTexture('/textures/dandelion-seed.png')
export const loadGrassLeafTexture = () =>
  loadAlphaTexture('/textures/grass-leaf.png')
export const loadPetalTexture = () => loadAlphaTexture('/textures/petal.png')

/**
 * Create an unlit billboard material for wind-blown particles.
 * Uses alpha from the texture multiplied by per-instance opacity.
 */
export function createWindParticleMaterial(
  alphaMap: THREE.Texture
): MeshBasicNodeMaterial {
  const mat = new MeshBasicNodeMaterial()
  mat.side = THREE.DoubleSide
  mat.transparent = true
  mat.depthWrite = false
  mat.alphaTest = 0.01

  const texNode: N = texture(alphaMap)
  const opacity: N = attribute(PARTICLE_OPACITY_ATTR, 'float')
  mat.colorNode = texNode.rgb
  mat.opacityNode = texNode.a.mul(opacity)

  return mat
}
