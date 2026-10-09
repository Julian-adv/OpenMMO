import * as THREE from 'three'
import { WebGPURenderer } from 'three/webgpu'
import { configureGLTFRenderer } from './gltfCache'
import {
  applyInitialAntialias,
  getAppliedAntialias,
  getCurrentPreset,
} from '../stores/graphicsSettings'

// Disposal can race WebGPU initialization.
const _origMaterialDispose = THREE.Material.prototype.dispose
THREE.Material.prototype.dispose = function () {
  try {
    _origMaterialDispose.call(this)
  } catch {
    // WebGPU backend not ready — ignore
  }
}

export function createWebGPURenderer(canvas: HTMLCanvasElement) {
  const preset = getCurrentPreset()
  const antialias = applyInitialAntialias()
  const renderer = new WebGPURenderer({ canvas, antialias })
  configureGLTFRenderer(renderer)
  renderer.setPixelRatio(
    Math.min(window.devicePixelRatio, preset.pixelRatioCap)
  )
  return renderer
}

// Preview canvases reuse antialias settings and dispose after initialization.
export function createPreviewWebGPURenderer(canvas: HTMLCanvasElement) {
  const renderer = new WebGPURenderer({
    canvas,
    antialias: getAppliedAntialias(),
  })
  configureGLTFRenderer(renderer)

  const _origDispose = renderer.dispose.bind(renderer)
  renderer.dispose = () =>
    renderer
      .init()
      .then(() => _origDispose())
      .catch(() => {})

  return renderer
}
