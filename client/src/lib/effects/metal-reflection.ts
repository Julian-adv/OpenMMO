import * as THREE from 'three'
import { CubeRenderTarget, type WebGPURenderer } from 'three/webgpu'

/** An equirect panorama: a vertical gradient (`stops`, top to bottom) with
 *  soft glows `[x, y, radius, alpha = 1, squash = 1]` on a 256×128 canvas. */
export function panorama(
  stops: [number, string][],
  glows: number[][],
  glowRgb: string
) {
  const canvas = document.createElement('canvas')
  canvas.width = 256
  canvas.height = 128
  const g = canvas.getContext('2d')!
  const gradient = g.createLinearGradient(0, 0, 0, 128)
  for (const [at, color] of stops) gradient.addColorStop(at, color)
  g.fillStyle = gradient
  g.fillRect(0, 0, 256, 128)
  for (const [x, y, r, alpha = 1, squash = 1] of glows) {
    const glow = g.createRadialGradient(x, y, 0, x, y, r)
    glow.addColorStop(0, `rgba(${glowRgb},${alpha})`)
    glow.addColorStop(1, `rgba(${glowRgb},0)`)
    g.fillStyle = glow
    g.setTransform(1, 0, 0, squash, 0, y * (1 - squash))
    g.fillRect(x - r, y - r, r * 2, r * 2)
  }
  const texture = new THREE.CanvasTexture(canvas)
  texture.mapping = THREE.EquirectangularReflectionMapping
  texture.colorSpace = THREE.SRGBColorSpace
  return texture
}

let stage: THREE.Texture | undefined

/** The character select stage: dark blue backdrop and floor under a few
 *  overhead spotlights. */
export function stageReflection() {
  return (stage ??= panorama(
    [
      [0, '#1a2a40'],
      [0.47, '#4a6a96'],
      [0.53, '#2c4260'],
      [1, '#16243a'],
    ],
    [
      [64, 16, 22],
      [192, 20, 18, 0.9],
      [128, 28, 14, 0.6],
      [0, 26, 14, 0.6],
      [256, 26, 14, 0.6],
      [64, 104, 44, 0.45],
      [192, 100, 36, 0.35],
    ],
    '255,246,228'
  ))
}

/** The one cube map every polished material reflects; scenes draw into it. */
export const reflectionCube = new CubeRenderTarget(64)
/** Shared by every reflective material, so one write turns them all. */
export const reflectionRotation = new THREE.Euler()
let any = false
const frustum = new THREE.Frustum()
const frustumMatrix = new THREE.Matrix4()
const stack: THREE.Object3D[] = []
const fillCamera = new THREE.CubeCamera(1, 10, reflectionCube)
const fillScene = new THREE.Scene()

/** Give materials that ask for it in their glTF extras (`envReflect`, the
 *  reflection strength) the shared reflection. */
export function applyMetalReflections(root: THREE.Object3D) {
  root.traverse((o) => {
    if (!(o instanceof THREE.Mesh)) return
    const materials = Array.isArray(o.material) ? o.material : [o.material]
    for (const material of materials) {
      const strength = material.userData?.envReflect
      if (
        typeof strength !== 'number' ||
        !(material instanceof THREE.MeshStandardMaterial)
      )
        continue
      material.envMap = reflectionCube.texture
      material.envMapIntensity = strength
      material.envMapRotation = reflectionRotation
      material.needsUpdate = true
      // userData survives clones, unlike a set of meshes.
      o.userData.reflective = true
      any = true
    }
  })
}

export function hasMetalReflections() {
  return any
}

export function reflectiveOnScreen(
  scene: THREE.Object3D,
  camera: THREE.Camera
) {
  frustumMatrix.multiplyMatrices(
    camera.projectionMatrix,
    camera.matrixWorldInverse
  )
  frustum.setFromProjectionMatrix(frustumMatrix)
  stack.length = 0
  stack.push(scene)
  while (stack.length) {
    const o = stack.pop()!
    if (!o.visible) continue
    if (o.userData.reflective && frustum.intersectsObject(o)) return true
    for (const child of o.children) stack.push(child)
  }
  return false
}

/** Show a still panorama, e.g. a stage that has nothing to capture. */
export function fillReflection(
  renderer: WebGPURenderer,
  texture: THREE.Texture
) {
  fillScene.background = texture
  fillCamera.update(renderer as unknown as THREE.WebGLRenderer, fillScene)
}
