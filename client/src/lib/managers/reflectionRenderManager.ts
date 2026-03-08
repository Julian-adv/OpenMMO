import * as THREE from 'three'
import { RenderTarget } from 'three/webgpu'

/**
 * Renders the scene (entities only) with the camera mirrored across the water
 * plane so the water shader can sample it as a planar reflection texture.
 *
 * Below-water entity fragments are clipped via a ClippingGroup that wraps the
 * entity hierarchy.  The group's `enabled` flag is toggled on only during the
 * reflection render so normal rendering is unaffected.
 */

const WATER_Y = 0

// Reflection matrix that mirrors across Y = WATER_Y.
// For WATER_Y = 0 this is simply diag(1, -1, 1, 1).
const _reflectionMatrix = /* @__PURE__ */ new THREE.Matrix4().set(
  1,
  0,
  0,
  0,
  0,
  -1,
  0,
  2 * WATER_Y,
  0,
  0,
  1,
  0,
  0,
  0,
  0,
  1
)

export class ReflectionRenderManager {
  readonly target: RenderTarget
  private renderer: {
    getRenderTarget(): THREE.RenderTarget | null
    setRenderTarget(target: THREE.RenderTarget | null): void
    render(scene: THREE.Scene, camera: THREE.Camera): void
    getClearColor(target: THREE.Color): THREE.Color
    setClearColor(color: THREE.ColorRepresentation, alpha?: number): void
    getClearAlpha(): number
    setClearAlpha(alpha: number): void
  }
  private scene: THREE.Scene
  private camera: THREE.Camera | null = null
  private terrainGroup: THREE.Group | null = null
  private waterGroup: THREE.Group | null = null

  /** The ClippingGroup wrapping entities — clipping toggled per frame. */
  private entityClipGroup: { enabled: boolean } | null = null

  /** A dedicated camera that receives the mirrored transform each frame. */
  private reflCam: THREE.OrthographicCamera

  constructor(
    renderer: {
      getRenderTarget(): THREE.RenderTarget | null
      setRenderTarget(target: THREE.RenderTarget | null): void
      render(scene: THREE.Scene, camera: THREE.Camera): void
      getClearColor(target: THREE.Color): THREE.Color
      setClearColor(color: THREE.ColorRepresentation, alpha?: number): void
      getClearAlpha(): number
      setClearAlpha(alpha: number): void
    },
    scene: THREE.Scene,
    width: number,
    height: number
  ) {
    this.renderer = renderer
    this.scene = scene
    this.target = new RenderTarget(
      Math.max(1, Math.floor(width / 2)),
      Math.max(1, Math.floor(height / 2)),
      {
        minFilter: THREE.LinearFilter,
        magFilter: THREE.LinearFilter,
        format: THREE.RGBAFormat,
      }
    )
    // Create camera with auto-update permanently disabled
    this.reflCam = new THREE.OrthographicCamera()
    this.reflCam.matrixAutoUpdate = false
    this.reflCam.matrixWorldAutoUpdate = false
  }

  get texture(): THREE.Texture {
    return this.target.texture
  }

  setCamera(camera: THREE.Camera) {
    this.camera = camera
  }

  setTerrainGroup(group: THREE.Group | null) {
    this.terrainGroup = group
  }

  setWaterGroup(group: THREE.Group | null) {
    this.waterGroup = group
  }

  setEntityClipGroup(group: { enabled: boolean } | null) {
    this.entityClipGroup = group
  }

  /** Render reflected entities to the reflection target. */
  render() {
    if (!this.camera) return

    // --- build reflected camera (avoid copy() which resets auto-update flags) ---
    const cam = this.camera as THREE.OrthographicCamera
    const rc = this.reflCam

    // Sync orthographic frustum
    rc.left = cam.left
    rc.right = cam.right
    rc.top = cam.top
    rc.bottom = cam.bottom
    rc.near = cam.near
    rc.far = cam.far
    rc.layers.mask = cam.layers.mask

    // W' = R · W  (reflection applied to the camera's world matrix)
    rc.matrixWorld.copy(cam.matrixWorld).premultiply(_reflectionMatrix)
    rc.matrixWorldInverse.copy(rc.matrixWorld).invert()
    rc.matrixWorldNeedsUpdate = false

    // Copy projection (unchanged by reflection)
    rc.projectionMatrix.copy(cam.projectionMatrix)
    rc.projectionMatrixInverse.copy(cam.projectionMatrixInverse)

    // --- hide non-entity objects ---
    const savedTerrain = this.terrainGroup?.visible
    if (this.terrainGroup) this.terrainGroup.visible = false
    const savedWater = this.waterGroup?.visible
    if (this.waterGroup) this.waterGroup.visible = false

    // --- enable clipping to discard below-water fragments ---
    if (this.entityClipGroup) this.entityClipGroup.enabled = true

    // --- render with transparent background ---
    const savedClearColor = new THREE.Color()
    this.renderer.getClearColor(savedClearColor)
    const savedClearAlpha = this.renderer.getClearAlpha()

    this.renderer.setClearColor(0x000000, 0)

    const prev = this.renderer.getRenderTarget()
    this.renderer.setRenderTarget(this.target)
    this.renderer.render(this.scene, this.reflCam)
    this.renderer.setRenderTarget(prev)

    this.renderer.setClearColor(savedClearColor, savedClearAlpha)

    // --- restore ---
    if (this.entityClipGroup) this.entityClipGroup.enabled = false
    if (this.terrainGroup) this.terrainGroup.visible = savedTerrain ?? true
    if (this.waterGroup) this.waterGroup.visible = savedWater ?? true
  }

  /** Clear the reflection target to transparent black. */
  clear() {
    const savedClearColor = new THREE.Color()
    this.renderer.getClearColor(savedClearColor)
    const savedClearAlpha = this.renderer.getClearAlpha()
    this.renderer.setClearColor(0x000000, 0)
    const prev = this.renderer.getRenderTarget()
    this.renderer.setRenderTarget(this.target)
    // Render with scene hidden to produce only the clear color
    const savedVisible = this.scene.visible
    this.scene.visible = false
    this.renderer.render(this.scene, this.reflCam)
    this.scene.visible = savedVisible
    this.renderer.setRenderTarget(prev)
    this.renderer.setClearColor(savedClearColor, savedClearAlpha)
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
