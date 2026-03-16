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
  const rtOpts: THREE.RenderTargetOptions = {
    format: THREE.RGBAFormat,
    type: THREE.HalfFloatType,
    minFilter: THREE.LinearFilter,
    magFilter: THREE.LinearFilter,
    depthBuffer: false,
  }
  const rtA = new THREE.RenderTarget(WETNESS_SIZE, WETNESS_SIZE, rtOpts)
  const rtB = new THREE.RenderTarget(WETNESS_SIZE, WETNESS_SIZE, rtOpts)

  const captureTexNode = texture(captureRT.texture)
  const prevWetnessNode = texture(rtA.texture)
  const uDeltaTime = uniform(0.016)

  const decayMat = new NodeMaterial()
  decayMat.depthTest = false
  decayMat.depthWrite = false
  decayMat.blending = THREE.NoBlending
  decayMat.lights = false

  decayMat.fragmentNode = Fn(() => {
    const vUv = uv()
    // The capture pass renders with the water material (transparent: true),
    // so alpha blending squares the alpha (alpha_out = src_alpha²). This
    // naturally suppresses bilinear interpolation bleed at the water
    // boundary — do NOT compensate with sqrt, as it amplifies the bleed.
    const waterAlpha = captureTexNode.sample(vUv).a
    // Y-flip for WebGPU RT coordinate convention: the capture camera's
    // render introduces a V-flip (clip Y=+1 → texture V=0), and the
    // fullscreen quad introduces another — so captureRT's double-flip
    // cancels out for waterAlpha. But prevWetness was already correctly
    // mapped (texture V = mesh UV v), so reading it at the fullscreen
    // quad's flipped UV gives the wrong row. Flip V to compensate.
    const prevUV = vec2(vUv.x, float(1.0).sub(vUv.y))
    const prev = prevWetnessNode.sample(prevUV).r
    const decay = pow(float(DECAY_RATE), uDeltaTime)
    const decayed = prev.mul(decay)
    // Cut off near-zero values so they don't linger indefinitely
    const cleaned = decayed.mul(step(float(0.01), decayed))
    const newVal = max(waterAlpha, cleaned)
    return vec4(newVal, 0, 0, 1)
  })()

  const decayScene = new THREE.Scene()
  const decayCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1)
  const decayMesh = new THREE.Mesh(new THREE.PlaneGeometry(2, 2), decayMat)
  decayScene.add(decayMesh)

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
      if (!(renderer as unknown as { _initialized: boolean })._initialized)
        return
      const dt = prevTime >= 0 ? Math.min(time - prevTime, 0.1) : 0
      prevTime = time
      uDeltaTime.value = dt

      const prevRT = renderer.getRenderTarget()

      // 1. Capture: render water mesh from above → alpha = holeAlpha
      captureMesh.material = material
      renderer.setRenderTarget(captureRT)
      renderer.render(captureScene, captureCamera)

      // 2. Decay: combine captured alpha with previous wetness
      const [readRT, writeRT] = phase === 0 ? [rtA, rtB] : [rtB, rtA]
      prevWetnessNode.value = readRT.texture
      renderer.setRenderTarget(writeRT)
      renderer.render(decayScene, decayCamera)

      renderer.setRenderTarget(prevRT)

      phase = phase === 0 ? 1 : 0
    },

    get readTexture() {
      // After update, phase was flipped: phase=1 means rtB was just written
      return (phase === 1 ? rtB : rtA).texture
    },
  }
}
