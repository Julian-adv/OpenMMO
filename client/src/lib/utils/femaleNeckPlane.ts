import * as THREE from 'three'

/** Plane under a stand collar (bind pose, metres): neck skin below it is hidden. */
export interface NeckPlane {
  point: [number, number, number]
  normal: [number, number, number]
  radius: number
}

// Skin farther below the plane than this (m) is not neck inside the collar.
const REACH = 0.1

/** Mask cutting a face part's mesh along a collar's plane, or null when the plane misses it. */
export function neckPlaneMask(
  geometry: THREE.BufferGeometry,
  plane: NeckPlane
) {
  const { index } = geometry
  const position = geometry.attributes.position
  if (!index || !position || !geometry.attributes.skinIndex) return null
  const [px, py, pz] = plane.point
  const [nx, ny, nz] = plane.normal
  const depth = new Float32Array(position.count)
  const inside = new Uint8Array(position.count)
  for (let i = 0; i < position.count; i++) {
    const x = position.getX(i) - px
    const y = position.getY(i) - py
    const z = position.getZ(i) - pz
    depth[i] = x * nx + y * ny + z * nz
    inside[i] = Math.hypot(x, z) < plane.radius && depth[i] > -REACH ? 1 : 0
  }
  const hidden: number[] = []
  const cut: number[] = []
  for (let t = 0; t < index.count / 3; t++) {
    const corners = [0, 1, 2].map((k) => index.getX(t * 3 + k))
    if (!corners.every((i) => inside[i])) continue
    const below = corners.filter((i) => depth[i] < 0).length
    if (below === 3) hidden.push(t)
    else if (below) cut.push(t)
  }
  if (!hidden.length && !cut.length) return null
  return {
    mesh_index: 0,
    primitive_index: 0,
    primitive_triangles: index.count / 3,
    hide_triangle_indices: hidden,
    clip_planes: [
      { point: plane.point, normal: plane.normal, triangle_indices: cut },
    ],
  }
}

export const neckPlaneSlot = (name: string, mesh: number) =>
  `${name}:neck-plane:${mesh}`
