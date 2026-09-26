import type * as THREE from 'three'
import type { WebGPURenderer } from 'three/webgpu'
import { yieldTask } from './frameYield'

/** The parts of Threlte's context a warmup renders with. */
export interface WarmupContext {
  renderer: unknown
  camera: { current: THREE.Camera }
  scene: THREE.Scene
}

const warmedByRenderer = new WeakMap<object, Map<string, Promise<void>>>()
const compilationByRenderer = new WeakMap<object, Promise<void>>()

/** Prepare models one at a time and share the result across instances. */
export function warmupPipelines(
  ctx: WarmupContext,
  key: string,
  object: THREE.Object3D
): Promise<void> {
  const renderer = ctx.renderer as WebGPURenderer
  let warmed = warmedByRenderer.get(renderer)
  if (!warmed) {
    warmed = new Map()
    warmedByRenderer.set(renderer, warmed)
  }
  let promise = warmed.get(key)
  if (!promise) {
    const previous = compilationByRenderer.get(renderer) ?? Promise.resolve()
    promise = previous
      .then(async () => {
        await renderer.init()
        await yieldTask()
        return compileDetached(renderer, object, ctx.camera.current, ctx.scene)
      })
      .catch((error: unknown) =>
        console.warn(`Pipeline warmup failed for ${key}`, error)
      )
    compilationByRenderer.set(renderer, promise)
    warmed.set(key, promise)
  }
  return promise
}

function compileDetached(
  renderer: WebGPURenderer,
  object: THREE.Object3D,
  camera: THREE.Camera,
  scene: THREE.Scene
): Promise<void> {
  // Initialized renderers collect detached meshes before compileAsync yields.
  const culled: THREE.Object3D[] = []
  object.traverse((child) => {
    if (child.frustumCulled) {
      child.frustumCulled = false
      culled.push(child)
    }
  })
  try {
    return renderer.compileAsync(object, camera, scene)
  } finally {
    for (const child of culled) child.frustumCulled = true
  }
}
