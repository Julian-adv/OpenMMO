import * as THREE from 'three'
import { WALL_THICKNESS } from './dungeon-geo-constants'

const BASE_CURVE = [1, 0.5, 0.134]

export function buildCaveWall(
  alongX: boolean,
  lo: number,
  hi: number,
  boundary: number,
  inward: number,
  height: number,
  seed: number
): THREE.BufferGeometry {
  const length = hi - lo
  const radius = Math.min(0.16, height * 0.12)
  const levels = [
    0,
    radius * 0.134,
    radius * 0.5,
    radius,
    height * 0.16,
    height * 0.3,
    height * 0.44,
    height * 0.58,
    height * 0.72,
    height * 0.86,
    height * 0.94,
    height,
  ]
  const segments = Math.max(2, Math.ceil(length / 0.5))
  const geo = new THREE.BoxGeometry(
    length,
    height,
    WALL_THICKNESS,
    segments,
    levels.length - 1,
    1
  )
  const positions = geo.getAttribute('position')
  const normals = geo.getAttribute('normal')
  const uvs = geo.getAttribute('uv')
  const phase = (seed % 65536) * 0.013 + boundary * 1.37
  for (let i = 0; i < positions.count; i++) {
    const along = positions.getX(i) + length / 2
    const t = THREE.MathUtils.clamp(positions.getY(i) / height + 0.5, 0, 1)
    const row = Math.round(t * (levels.length - 1))
    const taper = THREE.MathUtils.smoothstep(
      Math.min(along, length - along),
      0,
      0.7
    )
    const wave = Math.sin((lo + along) * 2.1 + phase)
    const detail = Math.sin((lo + along) * 4.3 - phase)
    const a = lo + along
    const level = levels[row]
    const body =
      THREE.MathUtils.smoothstep(level, radius, height * 0.2) *
      (1 - THREE.MathUtils.smoothstep(level, height * 0.78, height * 0.96))
    const bulge =
      Math.sin(a * 1.7 + level * 1.3 + phase) *
        Math.cos(level * 2.8 - a * 0.45 + phase * 0.3) *
        0.09 +
      Math.sin(a * 3.2 - level * 3.4 - phase * 0.7) * 0.045 +
      Math.cos(a * 1.2 + level * 4.8 + phase * 0.4) * 0.025
    const relief = taper * ((wave + detail) * 0.006 + bulge * body)
    const inset = taper * (BASE_CURVE[row] ?? 0) * radius * (1 + wave * 0.12)
    const front = positions.getZ(i) > 0
    const across =
      boundary + inward * (relief + (front ? inset : -WALL_THICKNESS))
    const y = level * (1 + taper * (wave * 0.026 + detail * 0.012))
    positions.setXYZ(i, alongX ? a : across, y, alongX ? across : a)
    if (Math.abs(normals.getY(i)) > 0.5) {
      uvs.setXY(i, positions.getX(i) * 0.45, positions.getZ(i) * 0.45)
    } else if (Math.abs(normals.getX(i)) > 0.5) {
      uvs.setXY(i, across * 0.45, y * 0.45)
    } else {
      uvs.setXY(i, a * 0.45, y * 0.45)
    }
  }
  // The local box's front must face into the corridor on all four sides.
  if ((alongX && inward < 0) || (!alongX && inward > 0)) {
    const indices = geo.getIndex()!
    for (let i = 0; i < indices.count; i += 3) {
      const first = indices.getX(i)
      indices.setX(i, indices.getX(i + 2))
      indices.setX(i + 2, first)
    }
  }
  geo.computeVertexNormals()
  geo.computeBoundingBox()
  return geo
}
