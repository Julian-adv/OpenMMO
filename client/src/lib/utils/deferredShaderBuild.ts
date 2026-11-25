import type { Camera, Scene } from 'three'
import { RenderObjectRefreshType, type WebGPURenderer } from 'three/webgpu'

/* eslint-disable @typescript-eslint/no-explicit-any */

/**
 * Build shaders for newly seen objects off the frame. three@0.186.1 has the
 * queue (`getForRenderDeferred`, yielding between build stages) but still
 * builds synchronously inside `render`, which costs 10-25 ms per new material
 * and pass. This is `Renderer._renderObjectDirect` with that one check added:
 * an object is skipped until its shader is ready, so it appears a few frames
 * late instead of stalling the frame. Re-check on three upgrades.
 */
export function deferShaderBuilds(
  renderer: WebGPURenderer,
  enabled: () => boolean
): void {
  const r = renderer as any
  if (
    r.__deferredShaderBuilds ||
    typeof r._renderObjectDirect !== 'function' ||
    typeof r._nodes?.getForRenderDeferred !== 'function'
  )
    return
  r.__deferredShaderBuilds = true
  spreadShaderBuildStages()
  retainUnusedShaders(r)
  // An object disposed before its deferred first draw never took its bind groups,
  // so releasing them would undercount the shared ones and free buffers still in use.
  const bindings = r._bindings
  const deleteForRender = bindings.deleteForRender
  bindings.deleteForRender = function (this: any, renderObject: any) {
    if (this.get(renderObject).initialized === true)
      return deleteForRender.call(this, renderObject)
    this.delete(renderObject)
  }
  const direct = r._renderObjectDirect
  r._renderObjectDirect = function (
    this: any,
    object: any,
    material: any,
    scene: any,
    camera: any,
    lightsNode: any,
    group: any,
    clippingContext: any,
    passId: any
  ) {
    if (!enabled() || this._currentRenderBundle !== null)
      return direct.call(
        this,
        object,
        material,
        scene,
        camera,
        lightsNode,
        group,
        clippingContext,
        passId
      )
    const renderObject = this._objects.get(
      object,
      material,
      scene,
      camera,
      lightsNode,
      this._currentRenderContext,
      clippingContext,
      passId
    )
    if (this._nodes.getForRenderDeferred(renderObject) === null) return
    renderObject.drawRange = object.geometry.drawRange
    renderObject.group = group
    const refresh = this._nodes.needsRefresh(renderObject)
    if (refresh === RenderObjectRefreshType.FULL) {
      this._nodes.updateBefore(renderObject)
      this._geometries.updateForRender(renderObject)
      this._nodes.updateForRender(renderObject)
      this._bindings.updateForRender(renderObject)
    } else if (refresh === RenderObjectRefreshType.SHARED) {
      this._nodes.updateBefore(renderObject)
      this._nodes.updateForRender(renderObject)
      this._bindings.updateSharedForRender(renderObject)
    }
    this._pipelines.updateForRender(renderObject)
    if (this._pipelines.isReady(renderObject)) {
      this.backend.draw(renderObject, this.info)
      if (refresh !== RenderObjectRefreshType.NONE)
        this._nodes.updateAfter(renderObject)
    }
  }
}

/** Shaders and pipelines kept after their last object goes. */
const RETAINED_SHADERS = 400

/** three drops a shader with its last object; keep recent ones so a
 *  character coming back into view skips the rebuild. */
function retainUnusedShaders(r: any) {
  const lru = <T>(limit: number, drop: (key: any, value: T) => void) => {
    const idle = new Map<any, T>()
    return (key: any, value: T) => {
      idle.delete(key)
      idle.set(key, value)
      if (idle.size <= limit) return
      const [oldKey, oldValue] = idle.entries().next().value!
      idle.delete(oldKey)
      drop(oldKey, oldValue)
    }
  }
  const nodes = r._nodes
  const keepState = lru<any>(RETAINED_SHADERS, (key, state) => {
    if (state.usedTimes === 0 && nodes.nodeBuilderCache.get(key) === state)
      nodes.nodeBuilderCache.delete(key)
  })
  const deleteNodes = nodes.delete
  nodes.delete = function (this: any, object: any) {
    const data = object?.isRenderObject ? this.data.get(object) : undefined
    const state = data?.nodeBuilderState
    if (state !== undefined) {
      data.nodeBuilderState = undefined
      if (--state.usedTimes === 0)
        keepState(this.getForRenderCacheKey(object), state)
    }
    return deleteNodes.call(this, object)
  }
  const pipelines = r._pipelines
  const releasePipeline = pipelines._releasePipeline
  const releaseProgram = pipelines._releaseProgram
  const keepPipeline = lru<any>(RETAINED_SHADERS, (_, pipeline) => {
    if (pipeline.usedTimes === 0) releasePipeline.call(pipelines, pipeline)
  })
  const keepProgram = lru<any>(RETAINED_SHADERS * 2, (_, program) => {
    if (program.usedTimes === 0) releaseProgram.call(pipelines, program)
  })
  pipelines._releasePipeline = (pipeline: any) =>
    keepPipeline(pipeline.cacheKey, pipeline)
  pipelines._releaseProgram = (program: any) => keepProgram(program, program)
}

/** Build time a frame may spend on shader stages before the rest waits. */
const BUILD_BUDGET_MS = 3

/** `scheduler.yield` resumes before the next paint, so 6 ms build stages
 *  piled into one frame; cap them per frame and push the rest to later ones. */
function spreadShaderBuildStages() {
  const scheduler = (globalThis as any).scheduler
  if (!scheduler?.yield || scheduler.__spread) return
  scheduler.__spread = true
  const yieldNow = scheduler.yield.bind(scheduler)
  let used = 0
  let resumedAt: number | null = null
  const tick = () => {
    used = 0
    requestAnimationFrame(tick)
  }
  requestAnimationFrame(tick)
  const resumed = () => {
    resumedAt = performance.now()
  }
  const nextFrame = () =>
    new Promise<void>((resolve) =>
      requestAnimationFrame(() => setTimeout(resolve, 0))
    )
  scheduler.yield = () => {
    if (resumedAt !== null) used += performance.now() - resumedAt
    resumedAt = null
    return (used < BUILD_BUDGET_MS ? yieldNow() : nextFrame()).then(resumed)
  }
}

/**
 * Render a secondary pass, then restore the scene's light list. Deferred
 * builds read it when they resume and cached shaders without hidden lights.
 */
export function renderKeepingLights(
  renderer: WebGPURenderer,
  scene: Scene,
  camera: Camera
): void {
  const lights = (renderer as any).lighting.getNode(scene)
  // A pass through the main camera refills the very array the node holds.
  const kept = lights.getLights().slice()
  renderer.render(scene, camera)
  lights.setLights(kept)
}
