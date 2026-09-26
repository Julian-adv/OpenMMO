import type * as THREE from 'three'
import type { WebGPURenderer } from 'three/webgpu'

interface PipelinesInternals {
  getForRender(renderObject: unknown, promises?: unknown): unknown
  updateForRender(renderObject: unknown): void
}

export interface AsyncPipelines {
  /** Pipelines of `scene` still compiling; their objects are not drawn yet. */
  pendingCount(): number
  dispose(): void
}

/**
 * Create `scene`'s render pipelines with createRenderPipelineAsync. The sync
 * call three.js makes on first draw blocks the GPU process for up to hundreds
 * of ms per new material (monsters, NPCs, props streaming in); async, the
 * object skips a few frames instead. Other scenes (PMREM, one-shot bakes) keep
 * the sync path, where a skipped draw would bake a hole.
 */
export function installAsyncPipelines(
  renderer: WebGPURenderer,
  scene: THREE.Scene
): AsyncPipelines {
  let pending = 0
  let disposed = false
  let restore: (() => void) | null = null
  const sink = {
    push(promise: Promise<void>) {
      pending++
      void promise.finally(() => pending--)
    },
  }

  void renderer.init().then(() => {
    if (disposed) return
    const pipelines = (
      renderer as unknown as { _pipelines: PipelinesInternals }
    )._pipelines
    const original = pipelines.updateForRender
    pipelines.updateForRender = function (renderObject) {
      if ((renderObject as { scene: THREE.Scene }).scene === scene) {
        this.getForRender(renderObject, sink)
      } else {
        original.call(this, renderObject)
      }
    }
    restore = () => {
      pipelines.updateForRender = original
    }
  })

  return {
    pendingCount: () => pending,
    dispose() {
      disposed = true
      restore?.()
    },
  }
}
