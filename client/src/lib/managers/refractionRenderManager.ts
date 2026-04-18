import * as THREE from 'three'
import { RenderTarget, type WebGPURenderer } from 'three/webgpu'

/**
 * Renders the scene (without water) to a half-resolution render target
 * so the water shader can sample it as a refraction texture.
 */
export class RefractionRenderManager {
  readonly target: RenderTarget
  private renderer: WebGPURenderer
  private scene: THREE.Scene
  private camera: THREE.Camera | null = null
  private waterGroup: THREE.Group | null = null

  constructor(
    renderer: WebGPURenderer,
    scene: THREE.Scene,
    width: number,
    height: number
  ) {
    this.renderer = renderer
    this.scene = scene
    // Half-resolution for performance
    this.target = new RenderTarget(
      Math.max(1, Math.floor(width / 2)),
      Math.max(1, Math.floor(height / 2)),
      {
        minFilter: THREE.LinearFilter,
        magFilter: THREE.LinearFilter,
        format: THREE.RGBAFormat,
      }
    )
  }

  get texture(): THREE.Texture {
    return this.target.texture
  }

  setCamera(camera: THREE.Camera) {
    this.camera = camera
  }

  setWaterGroup(group: THREE.Group | null) {
    this.waterGroup = group
  }

  /** Render scene without water to the refraction target. */
  render() {
    if (!this.camera || !this.renderer.hasInitialized()) return

    // Hide water meshes
    if (this.waterGroup) this.waterGroup.visible = false

    const currentRenderTarget = this.renderer.getRenderTarget()
    this.renderer.setRenderTarget(this.target)
    this.renderer.render(this.scene, this.camera)
    this.renderer.setRenderTarget(currentRenderTarget)

    // Restore water visibility
    if (this.waterGroup) this.waterGroup.visible = true
  }

  /** Clear the refraction target to black. */
  clear() {
    if (!this.renderer.hasInitialized() || !this.camera) return
    const currentRenderTarget = this.renderer.getRenderTarget()
    this.renderer.setRenderTarget(this.target)
    const savedVisible = this.scene.visible
    this.scene.visible = false
    this.renderer.render(this.scene, this.camera!)
    this.scene.visible = savedVisible
    this.renderer.setRenderTarget(currentRenderTarget)
  }

  resize(width: number, height: number) {
    this.target.setSize(
      Math.max(1, Math.floor(width / 2)),
      Math.max(1, Math.floor(height / 2))
    )
  }

  dispose() {
    this.target.dispose()
  }
}
