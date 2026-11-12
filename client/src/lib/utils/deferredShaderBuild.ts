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
