import type * as THREE from 'three'
import type { WebGPURenderer } from 'three/webgpu'

interface PipelinesInternals {
  getForRender(renderObject: unknown, promises?: unknown): unknown
  updateForRender(renderObject: unknown): void
}

type DrawObject = (
  object: THREE.Object3D,
  material: THREE.Material,
  scene: THREE.Scene,
  camera: THREE.Camera,
  lights: unknown,
  group: unknown,
  clipping: unknown,
  passId?: string
) => void

interface RendererInternals {
  _pipelines: PipelinesInternals
  _renderObjectDirect: DrawObject
  _currentRenderContext: unknown
  _objects: {
    getChainMap(passId?: string): { get(keys: unknown[]): unknown }
  }
}

export interface AsyncPipelines {
  /** Pipelines of `scene` still compiling; their objects are not drawn yet. */
  pendingCount(): number
  beginFrame(): void
  dispose(): void
}

/** Budget first draws and compile GPU pipelines asynchronously; preserve one-shot bakes. */
export function installAsyncPipelines(
  renderer: WebGPURenderer,
  scene: THREE.Scene
): AsyncPipelines {
  let pending = 0
  let deferred = 0
  let preparationMs = 0
  const preparationBudgetMs = 4
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
    const internals = renderer as unknown as RendererInternals
    const pipelines = internals._pipelines
    const original = pipelines.updateForRender
    const originalDraw = internals._renderObjectDirect
    internals._renderObjectDirect = function (...args) {
      const [object, material, objectScene, , lights, , , passId] = args
      // Inspect without registering an undrawn object with GPU resource ownership.
      const cold =
        objectScene === scene &&
        this._objects
          .getChainMap(passId)
          .get([object, material, this._currentRenderContext, lights]) ===
          undefined
      if (!cold) {
        originalDraw.apply(this, args)
        return
      }
      if (preparationMs >= preparationBudgetMs) {
        deferred++
        return
      }
      const start = performance.now()
      // Nested shadow passes share this frame's preparation budget.
      preparationMs += preparationBudgetMs
      try {
        originalDraw.apply(this, args)
      } finally {
        preparationMs += performance.now() - start - preparationBudgetMs
      }
    }
    pipelines.updateForRender = function (renderObject) {
      if ((renderObject as { scene: THREE.Scene }).scene === scene) {
        this.getForRender(renderObject, sink)
      } else {
        original.call(this, renderObject)
      }
    }
    restore = () => {
      pipelines.updateForRender = original
      internals._renderObjectDirect = originalDraw
    }
  })

  return {
    pendingCount: () => pending + deferred,
    beginFrame() {
      preparationMs = 0
      deferred = 0
    },
    dispose() {
      disposed = true
      restore?.()
    },
  }
}
