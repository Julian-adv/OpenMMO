import * as THREE from 'three'

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

let fallback: THREE.Texture | undefined
let stage: THREE.Texture | undefined

/** Bright and warm all round, for scenes that capture nothing. */
function reflectedWorld() {
  return (fallback ??= panorama(
    [
      [0, '#fffaf0'],
      [0.5, '#e9dcc2'],
      [1, '#b59a72'],
    ],
    [
      [60, 30, 26, 0.9],
      [190, 22, 18, 0.9],
      [120, 92, 30, 0.9],
      [230, 84, 20, 0.9],
    ],
    '255,255,255'
  ))
}

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

const reflective = new Set<THREE.MeshStandardMaterial>()
let source: THREE.Texture | null = null

/** Give materials that ask for it in their glTF extras (`envReflect`, the
 *  reflection strength) something to reflect. */
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
      material.envMap = source ?? reflectedWorld()
      material.envMapIntensity = strength
      material.needsUpdate = true
      reflective.add(material)
    }
  })
}

export function hasMetalReflections() {
  return reflective.size > 0
}

/** Reflect `texture` (the captured surroundings), or the stock panorama. */
export function setReflectionSource(texture: THREE.Texture | null) {
  if (source === texture) return
  source = texture
  for (const material of reflective) {
    material.envMap = texture ?? reflectedWorld()
    material.needsUpdate = true
  }
}

/** Drop `texture` as the reflection source if it still is one. */
export function releaseReflectionSource(texture: THREE.Texture) {
  if (source === texture) setReflectionSource(null)
}

/** Turn what every reflective material sees, e.g. to tip the horizon
 *  toward a camera looking down. */
export function setReflectionRotation(rotation: THREE.Euler) {
  for (const material of reflective) material.envMapRotation.copy(rotation)
}
