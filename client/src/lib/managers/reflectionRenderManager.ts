import * as THREE from 'three'
import { RenderTarget } from 'three/webgpu'

/**
 * Renders the scene (entities only) with the camera mirrored across the water
 * plane so the water shader can sample it as a planar reflection texture.
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
  private terrainMeshes: (THREE.Mesh | undefined)[] = []
  private waterGroup: THREE.Group | null = null

  /** A dedicated camera that receives the mirrored transform each frame. */
  private reflCam: THREE.OrthographicCamera

  /**
   * Invisible plane at Y = WATER_Y rendered AFTER entities (renderOrder 999)
   * with depthFunc = GREATER.  From the mirrored camera below the water,
   * entity fragments below Y=0 are *closer* than the plane, so the plane's
   * depth is GREATER → it passes the depth test there.  Custom zero-blending
   * erases those below-water pixels to transparent.
   */
  private eraserMesh: THREE.Mesh

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

    // Build the eraser plane
    const eraserMat = new THREE.MeshBasicMaterial({
      side: THREE.DoubleSide,
      transparent: true,
      depthTest: true,
      depthWrite: false,
      depthFunc: THREE.GreaterDepth,
      blending: THREE.CustomBlending,
      blendSrc: THREE.ZeroFactor,
      blendDst: THREE.ZeroFactor,
      blendSrcAlpha: THREE.ZeroFactor,
      blendDstAlpha: THREE.ZeroFactor,
    })
    this.eraserMesh = new THREE.Mesh(
      new THREE.PlaneGeometry(2000, 2000),
      eraserMat
    )
    this.eraserMesh.rotation.x = -Math.PI / 2 // horizontal
    this.eraserMesh.position.y = WATER_Y
    this.eraserMesh.renderOrder = 999
    this.eraserMesh.frustumCulled = false
  }

  get texture(): THREE.Texture {
    return this.target.texture
  }

  setCamera(camera: THREE.Camera) {
    this.camera = camera
  }

  setTerrainMeshes(meshes: (THREE.Mesh | undefined)[]) {
    this.terrainMeshes = meshes
  }

  setWaterGroup(group: THREE.Group | null) {
    this.waterGroup = group
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
    const savedTerrain: boolean[] = []
    for (let i = 0; i < this.terrainMeshes.length; i++) {
      const m = this.terrainMeshes[i]
      if (m) {
        savedTerrain[i] = m.visible
        m.visible = false
      }
    }
    const savedWater = this.waterGroup?.visible
    if (this.waterGroup) this.waterGroup.visible = false

    // --- render with transparent background ---
    const savedClearColor = new THREE.Color()
    this.renderer.getClearColor(savedClearColor)
    const savedClearAlpha = this.renderer.getClearAlpha()

    this.renderer.setClearColor(0x000000, 0)

    // Add eraser plane to the scene for this render only
    this.scene.add(this.eraserMesh)

    const prev = this.renderer.getRenderTarget()
    this.renderer.setRenderTarget(this.target)
    this.renderer.render(this.scene, this.reflCam)
    this.renderer.setRenderTarget(prev)

    this.scene.remove(this.eraserMesh)

    this.renderer.setClearColor(savedClearColor, savedClearAlpha)

    // --- restore visibility ---
    for (let i = 0; i < this.terrainMeshes.length; i++) {
      const m = this.terrainMeshes[i]
      if (m) m.visible = savedTerrain[i]
    }
    if (this.waterGroup) this.waterGroup.visible = savedWater ?? true
  }

  resize(width: number, height: number) {
    this.target.setSize(
      Math.max(1, Math.floor(width / 2)),
      Math.max(1, Math.floor(height / 2))
    )
  }

  dispose() {
    ;(this.eraserMesh.material as THREE.Material).dispose()
    this.eraserMesh.geometry.dispose()
    this.target.dispose()
  }
}
