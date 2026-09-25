import * as THREE from 'three'
import {
  MeshLambertNodeMaterial,
  PhongLightingModel,
  type LightingModelDirectInput,
  type Node,
} from 'three/webgpu'
import { attribute, diffuseColor, nodeObject, texture } from 'three/tsl'
import { PARTICLE_OPACITY_ATTR } from './wind-particle-material'

class RainLightingModel extends PhongLightingModel {
  override direct({ lightColor, reflectedLight }: LightingModelDirectInput) {
    // Tiny droplets scatter light regardless of the billboard's orientation.
    const color = nodeObject(lightColor as Node<'vec3'>)
    nodeObject(reflectedLight.directDiffuse as Node<'vec3'>).addAssign(
      diffuseColor.rgb.mul(color).mul(1 / Math.PI)
    )
  }
}

class RainParticleMaterial extends MeshLambertNodeMaterial {
  static override get type() {
    return 'RainParticleMaterial'
  }

  override setupLightingModel() {
    return new RainLightingModel(false)
  }
}

export function createRainParticleMaterial(
  map: THREE.Texture
): MeshLambertNodeMaterial {
  const tex = texture(map)
  return new RainParticleMaterial({
    side: THREE.DoubleSide,
    transparent: true,
    forceSinglePass: true,
    depthWrite: false,
    alphaTest: 0.01,
    colorNode: tex.rgb,
    opacityNode: tex.a.mul(attribute(PARTICLE_OPACITY_ATTR, 'float')),
  })
}
