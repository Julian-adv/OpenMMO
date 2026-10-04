import type * as THREE from 'three'
import type { WebGPURenderer } from 'three/webgpu'

export function installSceneMatrixReuse(
  renderer: WebGPURenderer,
  scene: THREE.Scene
): () => void {
  const render = renderer.render
  renderer.render = function (renderScene, camera) {
    if (renderScene !== scene || !scene.matrixWorldAutoUpdate)
      return render.call(this, renderScene, camera)

    scene.updateMatrixWorld()
    scene.matrixWorldAutoUpdate = false
    try {
      return render.call(this, renderScene, camera)
    } finally {
      scene.matrixWorldAutoUpdate = true
    }
  }
  return () => {
    renderer.render = render
  }
}
