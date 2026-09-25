import * as THREE from 'three'
import type { WebGPURenderer } from 'three/webgpu'
import { CSMShadowNode } from 'three/addons/csm/CSMShadowNode.js'
import type { GraphicsPreset } from '../../stores/graphicsSettings'

const CSM_MAX_FAR = 200
const CSM_CASCADES = 2

export function setupCsmShadow(light: THREE.DirectionalLight): void {
  const csm = new CSMShadowNode(light, {
    cascades: CSM_CASCADES,
    maxFar: CSM_MAX_FAR,
    mode: 'practical',
    lightMargin: 100,
  })
  csm.fade = true
  light.shadow.shadowNode = csm
}

export function setDirectionalShadowIntensity(
  light: THREE.DirectionalLight,
  intensity: number
): void {
  light.shadow.intensity = intensity
  const csm = light.shadow.shadowNode
  if (csm instanceof CSMShadowNode) {
    // CSM keeps a separate shadow for each cascade.
    for (const cascade of csm.lights) {
      if (cascade.shadow) cascade.shadow.intensity = intensity
    }
  }
}

const _tmpVec2 = new THREE.Vector2()

export function applyGraphicsPreset(
  renderer: WebGPURenderer,
  preset: GraphicsPreset,
  directionalLight: THREE.DirectionalLight | null | undefined
): void {
  const newRatio = Math.min(window.devicePixelRatio, preset.pixelRatioCap)
  if (renderer.getPixelRatio() !== newRatio) {
    renderer.setPixelRatio(newRatio)
    const sz = renderer.getSize(_tmpVec2)
    renderer.setSize(sz.width, sz.height)
  }

  if (directionalLight?.shadow) {
    const cur = directionalLight.shadow.mapSize
    if (cur.x !== preset.shadowMapSize) {
      cur.set(preset.shadowMapSize, preset.shadowMapSize)
      if (directionalLight.shadow.map) {
        directionalLight.shadow.map.dispose()
        directionalLight.shadow.map = null
      }
    }
  }
}
