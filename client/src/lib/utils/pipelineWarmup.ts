import type * as THREE from 'three'
import type { WebGPURenderer } from 'three/webgpu'

/** The parts of Threlte's context a warmup renders with. */
export interface WarmupContext {
  renderer: unknown
  camera: { current: THREE.Camera }
  scene: THREE.Scene
}

const warmedByRenderer = new WeakMap<object, Map<string, Promise<void>>>()

/**
 * Build `object`'s shaders and pipelines in yielding chunks before it joins
 * the scene, instead of in one blocking go on its first draw. Shared per
 * `key`, so later instances of the same model resolve at once.
 */
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
    promise = compileDetached(
      renderer,
      object,
      ctx.camera.current,
      ctx.scene
    ).catch((error: unknown) =>
      console.warn(`Pipeline warmup failed for ${key}`, error)
    )
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
  // A detached model sits at the origin, usually outside the view frustum.
  // compileAsync gathers its draw list synchronously, so the flags can be
  // restored as soon as it returns.
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
