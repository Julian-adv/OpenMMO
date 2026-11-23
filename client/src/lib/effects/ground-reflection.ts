import * as THREE from 'three'
import { CubeRenderTarget, type WebGPURenderer } from 'three/webgpu'
import { renderKeepingLights } from '../utils/deferredShaderBuild'
import { AMBIENT_DAY_INTENSITY } from '../components/game-scene/scene-lighting'
import {
  hasMetalReflections,
  panorama,
  releaseReflectionSource,
  setReflectionRotation,
  setReflectionSource,
} from './metal-reflection'

/** Objects on this layer (terrain, dungeon floors and walls, lights) are what
 *  polished metal reflects. */
export const REFLECTION_LAYER = 7
const SIZE = 64
const EYE_HEIGHT = 1
const RECAPTURE_DISTANCE = 3
/** Recapture when daylight drifts this far (0 night … 1 midday). */
const DAYLIGHT_STEP = 0.05
/** The camera looks down at about 35°, so a mirror facing it would show only
 *  sky. Tip the reflection this far so it shows the horizon instead. */
const TILT = THREE.MathUtils.degToRad(40)
const right = new THREE.Vector3()
const tilt = new THREE.Quaternion()
const tiltEuler = new THREE.Euler()
const lastRight = new THREE.Vector3()
const ambientHsl = { h: 0, s: 0, l: 0 }

let skyTexture: THREE.Texture | undefined

/** A blue sky with a few soft clouds over plain ground. */
function sky() {
  return (skyTexture ??= panorama(
    [
      [0, '#5f8fc8'],
      [0.5, '#d6e4ef'],
      [0.5, '#8c8466'],
      [1, '#8c8466'],
    ],
    [
      [40, 34, 34, 0.85, 7 / 34],
      [110, 22, 26, 0.85, 6 / 26],
      [170, 40, 40, 0.85, 8 / 40],
      [225, 18, 22, 0.85, 5 / 22],
      [80, 50, 30, 0.85, 5 / 30],
    ],
    '255,255,255'
  ))
}

/**
 * Captures the surroundings into a small cube map for polished metal to
 * reflect: ground under a painted sky, or the dungeon around the player.
 * Runs only while some reflective material is loaded, and only after the
 * player moves a few metres, goes up or down, or daylight changes; one face
 * per frame.
 */
export class GroundReflection {
  private readonly target = new CubeRenderTarget(SIZE)
  private readonly camera = new THREE.CubeCamera(0.1, 80, this.target)
  private readonly last = new THREE.Vector3(Infinity, 0, 0)
  private underground = false
  private tilted: boolean | undefined
  private daylight = 0
  private ambient: THREE.AmbientLight | undefined
  private shadows: THREE.LightShadow[] = []
  /** The next cube face to draw, or -1 between captures. */
  private face = -1

  constructor() {
    this.camera.layers.set(REFLECTION_LAYER)
    this.camera.children.forEach((c) => c.layers.set(REFLECTION_LAYER))
  }

  update(
    renderer: WebGPURenderer,
    scene: THREE.Scene,
    view: THREE.Camera,
    position: THREE.Vector3Like,
    underground: boolean
  ) {
    if (!hasMetalReflections()) return
    this.tilt(view, underground)
    if (this.face < 0) {
      if (
        underground === this.underground &&
        this.last.distanceToSquared(position) < RECAPTURE_DISTANCE ** 2 &&
        Math.abs(this.currentDaylight() - this.daylight) < DAYLIGHT_STEP
      )
        return
      this.start(renderer, scene, position, underground)
    }
    this.drawFace(renderer, scene)
  }

  private tilt(view: THREE.Camera, underground: boolean) {
    right.setFromMatrixColumn(view.matrixWorld, 0)
    if (underground === this.tilted && right.equals(lastRight)) return
    this.tilted = underground
    lastRight.copy(right)
    tilt.setFromAxisAngle(right, underground ? 0 : TILT)
    setReflectionRotation(tiltEuler.setFromQuaternion(tilt))
  }

  /** 0 at night, about 1 at midday, from the ambient light. */
  private currentDaylight() {
    const ambient = this.ambient
    if (!ambient) return 0
    ambient.color.getHSL(ambientHsl, THREE.LinearSRGBColorSpace)
    return (ambientHsl.l * ambient.intensity) / AMBIENT_DAY_INTENSITY
  }

  private start(
    renderer: WebGPURenderer,
    scene: THREE.Scene,
    position: THREE.Vector3Like,
    underground: boolean
  ) {
    this.underground = underground
    this.last.copy(position)
    this.face = 0
    this.shadows = []
    scene.traverse((o) => {
      if (!(o instanceof THREE.Light)) return
      o.layers.enable(REFLECTION_LAYER)
      if (o instanceof THREE.AmbientLight) this.ambient = o
      if ('shadow' in o && o.shadow instanceof THREE.LightShadow)
        this.shadows.push(o.shadow)
    })
    this.daylight = this.currentDaylight()
    const camera = this.camera
    camera.position.set(position.x, position.y + EYE_HEIGHT, position.z)
    if (camera.coordinateSystem !== renderer.coordinateSystem) {
      camera.coordinateSystem = renderer.coordinateSystem
      camera.updateCoordinateSystem()
    }
    camera.updateMatrixWorld()
  }

  private drawFace(renderer: WebGPURenderer, scene: THREE.Scene) {
    const face = this.face
    // The shadow maps from the main pass serve; don't redraw them per face.
    const autoUpdates = this.shadows.map((shadow) => {
      const was = shadow.autoUpdate
      shadow.autoUpdate = false
      return was
    })
    const { background, backgroundIntensity } = scene
    // Underground the scene's own dark background stands in for the vault.
    if (!this.underground) {
      scene.background = sky()
      scene.backgroundIntensity = this.daylight
    }
    const texture = this.target.texture
    // Mipmaps once, on the last face.
    texture.generateMipmaps = face === 5
    const previous = renderer.getRenderTarget()
    renderer.setRenderTarget(this.target, face)
    renderKeepingLights(
      renderer,
      scene,
      this.camera.children[face] as THREE.Camera
    )
    renderer.setRenderTarget(previous)
    scene.background = background
    scene.backgroundIntensity = backgroundIntensity
    this.shadows.forEach((shadow, i) => (shadow.autoUpdate = autoUpdates[i]))
    if (++this.face < 6) return
    this.face = -1
    texture.needsPMREMUpdate = true
    setReflectionSource(texture)
  }

  dispose() {
    releaseReflectionSource(this.target.texture)
    this.target.dispose()
  }
}
