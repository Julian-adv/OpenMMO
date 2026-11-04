import * as THREE from 'three'

export type SkinBrushEdits = { hide: Set<number>; show: Set<number> }

// Picks body triangles in the rest pose, so a stroke means the same skin in any animation frame.
// A triangle counts when its centre or any corner is inside the radius; the clicked triangle always counts,
// since a long sliver can have its centre and corners all outside a small radius.
export function skinBrush(
  position: THREE.BufferAttribute,
  restIndex: ArrayLike<number>
) {
  const triangles = restIndex.length / 3
  const centres = new Float32Array(triangles * 3)
  for (let t = 0; t < triangles; t++)
    for (let corner = 0; corner < 3; corner++)
      for (let axis = 0; axis < 3; axis++)
        centres[t * 3 + axis] +=
          position.getComponent(restIndex[t * 3 + corner], axis) / 3
  const byCorners = new Map<string, number>()
  const key = (a: number, b: number, c: number) =>
    [a, b, c].sort((x, y) => x - y).join(':')
  for (let t = 0; t < triangles; t++)
    byCorners.set(
      key(restIndex[t * 3], restIndex[t * 3 + 1], restIndex[t * 3 + 2]),
      t
    )
  const point = new THREE.Vector3()
  const corner = new THREE.Vector3()
  return (
    face: { a: number; b: number; c: number },
    barycoord: THREE.Vector3,
    radius: number
  ) => {
    point.set(0, 0, 0)
    for (const [vertex, weight] of [
      [face.a, barycoord.x],
      [face.b, barycoord.y],
      [face.c, barycoord.z],
    ])
      point.addScaledVector(
        corner.fromBufferAttribute(position, vertex),
        weight
      )
    const picked = new Set<number>()
    const clicked = byCorners.get(key(face.a, face.b, face.c))
    if (clicked !== undefined) picked.add(clicked)
    const within = (x: number, y: number, z: number) =>
      (x - point.x) ** 2 + (y - point.y) ** 2 + (z - point.z) ** 2 <=
      radius * radius
    for (let t = 0; t < triangles; t++) {
      if (within(centres[t * 3], centres[t * 3 + 1], centres[t * 3 + 2])) {
        picked.add(t)
        continue
      }
      for (let c = 0; c < 3; c++) {
        corner.fromBufferAttribute(position, restIndex[t * 3 + c])
        if (within(corner.x, corner.y, corner.z)) {
          picked.add(t)
          break
        }
      }
    }
    return [...picked]
  }
}

export function applySkinBrush(
  hidden: readonly number[],
  edits: SkinBrushEdits
) {
  const result = new Set(hidden)
  for (const t of edits.hide) result.add(t)
  for (const t of edits.show) result.delete(t)
  return [...result].sort((a, b) => a - b)
}
