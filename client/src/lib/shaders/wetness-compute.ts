import * as THREE from 'three'
import { NodeMaterial, WebGPURenderer } from 'three/webgpu'
import {
  Fn,
  texture,
  vec2,
  vec4,
  float,
  max,
  pow,
  step,
  uniform,
  uv,
} from 'three/tsl'

const WETNESS_SIZE = 256
/** Wetness decays to this fraction per second */
const DECAY_RATE = 0.92

// ── Shared decay pass resources ──
// The decay NodeMaterial + its fullscreen quad are shared across every
// wetness system so its WebGPU pipeline compiles exactly once instead of
// once per tile. Creating a NodeMaterial per tile made each new water tile
// pay a ~100ms pipeline compile on first use, stalling the main thread
// whenever the player walked into a new region with fresh water tiles.
let sharedDecayScene: THREE.Scene | undefined
let sharedDecayCamera: THREE.OrthographicCamera | undefined
let sharedCaptureTexNode: ReturnType<typeof texture> | undefined
let sharedPrevWetnessNode: ReturnType<typeof texture> | undefined
const sharedUDeltaTime = uniform(0.016)

function ensureSharedDecayPass() {
  if (sharedDecayScene) return

  // Placeholder 1x1 textures — actual tile RTs are swapped in per-update via
  // `.value = ...`. The node only needs a non-null reference at build time.
  const placeholder = new THREE.DataTexture(
    new Uint8Array([0, 0, 0, 255]),
    1,
    1,
    THREE.RGBAFormat,
    THREE.UnsignedByteType
  )
  placeholder.needsUpdate = true

  sharedCaptureTexNode = texture(placeholder)
  sharedPrevWetnessNode = texture(placeholder)

  const sharedDecayMat = new NodeMaterial()
  sharedDecayMat.depthTest = false
  sharedDecayMat.depthWrite = false
  sharedDecayMat.blending = THREE.NoBlending
  sharedDecayMat.lights = false

  sharedDecayMat.fragmentNode = Fn(() => {
    const vUv = uv()
    const waterAlpha = sharedCaptureTexNode!.sample(vUv).a
    // Y-flip: capture camera flips V once; fullscreen quad flips again —
    // captureRT double-flip cancels, but prevWetness needs manual flip.
    const prevUV = vec2(vUv.x, float(1.0).sub(vUv.y))
    const prev = sharedPrevWetnessNode!.sample(prevUV).r
    const decay = pow(float(DECAY_RATE), sharedUDeltaTime)
    const decayed = prev.mul(decay)
    const cleaned = decayed.mul(step(float(0.01), decayed))
    const newVal = max(waterAlpha, cleaned)
    return vec4(newVal, 0, 0, 1)
  })()

  sharedDecayScene = new THREE.Scene()
  sharedDecayCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1)
  const decayMesh = new THREE.Mesh(
    new THREE.PlaneGeometry(2, 2),
    sharedDecayMat
  )
  sharedDecayScene.add(decayMesh)
}

export interface WetnessResult {
  /**
   * Render the wetness pre-pass:
   * 1. Capture the water material's alpha (holeAlpha) to a 128x128 RT
   * 2. Combine with previous wetness via exponential decay
   *
   * The caller must set the water material's uWetnessMap to fallback BEFORE
   * calling this to avoid feedback, and restore it to readTexture AFTER.
   */
  update: (
    renderer: WebGPURenderer,
    material: THREE.Material,
    time: number
  ) => void
  /** Current wetness texture for fragment shader sampling */
  readonly readTexture: THREE.Texture
  /** Reposition this wetness system to a new tile */
  reposition: (tileX: number, tileZ: number) => void
}

/**
 * Creates a per-tile wetness accumulation system.
 *
 * Two-pass approach per frame:
 * 1. **Capture pass** — renders the actual water tile mesh (with the real water
 *    material) from an orthographic camera looking straight down into a 128x128
 *    RenderTarget. The alpha channel of this RT contains the water material's
 *    holeAlpha, guaranteeing identical noise because it's the same shader.
 * 2. **Decay pass** — a fullscreen quad reads the captured alpha and the
 *    previous frame's wetness, outputting `max(capturedAlpha, prev * decay)`.
 *    Two RTs ping-pong for the decay state.
 *
 * The main water material samples the decay RT for wet-sand darkening.
 */
export function createWetnessSystem(
  geometry: THREE.BufferGeometry,
  tileX: number,
  tileZ: number,
  tileSize: number
): WetnessResult {
  ensureSharedDecayPass()

  const px = tileX * tileSize
  const pz = tileZ * tileSize

  // ── Capture pass: render water mesh from above ──
  const captureRT = new THREE.RenderTarget(WETNESS_SIZE, WETNESS_SIZE, {
    format: THREE.RGBAFormat,
    type: THREE.UnsignedByteType,
    depthBuffer: true,
  })
  const captureScene = new THREE.Scene()
  const captureMesh = new THREE.Mesh(geometry)
  captureMesh.position.set(px, 0.01, pz)
  captureMesh.receiveShadow = false
  captureMesh.castShadow = false
  captureScene.add(captureMesh)

  const captureCamera = new THREE.OrthographicCamera(
    -tileSize / 2,
    tileSize / 2,
    tileSize / 2,
    -tileSize / 2,
    0.01,
    20
  )
  captureCamera.position.set(px, 10, pz)
  captureCamera.up.set(0, 0, -1)
  captureCamera.lookAt(px, 0, pz)

  // ── Decay pass: fullscreen quad combining captured alpha + previous wetness ──
  // NodeMaterial + scene are shared across all tiles (see ensureSharedDecayPass).
  const rtOpts: THREE.RenderTargetOptions = {
    format: THREE.RGBAFormat,
    type: THREE.HalfFloatType,
    minFilter: THREE.LinearFilter,
    magFilter: THREE.LinearFilter,
    depthBuffer: false,
  }
  const rtA = new THREE.RenderTarget(WETNESS_SIZE, WETNESS_SIZE, rtOpts)
  const rtB = new THREE.RenderTarget(WETNESS_SIZE, WETNESS_SIZE, rtOpts)

  let phase = 0
  let prevTime = -1

  return {
    reposition(newTileX: number, newTileZ: number) {
      const newPx = newTileX * tileSize
      const newPz = newTileZ * tileSize
      captureMesh.position.set(newPx, 0.01, newPz)
      captureCamera.position.set(newPx, 10, newPz)
      captureCamera.lookAt(newPx, 0, newPz)
      // Reset decay state so old wetness doesn't bleed into new position
      phase = 0
      prevTime = -1
    },

    update(renderer: WebGPURenderer, material: THREE.Material, time: number) {
      if (!renderer.hasInitialized()) return
      const dt = prevTime >= 0 ? Math.min(time - prevTime, 0.1) : 0
      prevTime = time
      sharedUDeltaTime.value = dt

      const prevRT = renderer.getRenderTarget()

      // 1. Capture: render water mesh from above → alpha = holeAlpha
      captureMesh.material = material
      renderer.setRenderTarget(captureRT)
      renderer.render(captureScene, captureCamera)

      // 2. Decay: combine captured alpha with previous wetness using the
      // shared decay material (swap texture inputs per tile).
      const [readRT, writeRT] = phase === 0 ? [rtA, rtB] : [rtB, rtA]
      sharedCaptureTexNode!.value = captureRT.texture
      sharedPrevWetnessNode!.value = readRT.texture
      renderer.setRenderTarget(writeRT)
      renderer.render(sharedDecayScene!, sharedDecayCamera!)

      renderer.setRenderTarget(prevRT)

      phase = phase === 0 ? 1 : 0
    },

    get readTexture() {
      // After update, phase was flipped: phase=1 means rtB was just written
      return (phase === 1 ? rtB : rtA).texture
    },
  }
}
