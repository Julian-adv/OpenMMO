import * as THREE from 'three'

export function femaleBracerMask(
  body: THREE.BufferGeometry,
  bracers: THREE.SkinnedMesh[],
  armSegments: [THREE.Vector3, THREE.Vector3][],
  armBones: number[][]
) {
  const positions = body.getAttribute('position')
  const indices = body.index
  const joints = body.getAttribute('skinIndex')
  const weights = body.getAttribute('skinWeight')
  if (
    !indices ||
    !joints ||
    !weights ||
    bracers.length !== armSegments.length ||
    bracers.length !== armBones.length
  )
    throw new Error('보호대 가림 정보를 만들 수 없습니다.')
  const covered = new Uint8Array(positions.count)
  const material = new THREE.MeshBasicMaterial({ side: THREE.DoubleSide })
  const ray = new THREE.Raycaster()
  const point = new THREE.Vector3()
  const axisPoint = new THREE.Vector3()
  const direction = new THREE.Vector3()
  const offset = new THREE.Vector3()
  const tangent = new THREE.Vector3()
  const origin = new THREE.Vector3()
  const edgeMargin = 0.006
  try {
    for (let side = 0; side < bracers.length; side++) {
      const geometry = bracers[side].geometry
      geometry.computeBoundingBox()
      const bounds = geometry.boundingBox!.clone().expandByScalar(0.04)
      const target = new THREE.Mesh(geometry, material)
      target.updateMatrixWorld(true)
      const [elbow, wrist] = armSegments[side]
      const axis = wrist.clone().sub(elbow)
      const lengthSquared = axis.lengthSq()
      const along = axis.clone().normalize()
      if (lengthSquared === 0) throw new Error('팔 길이가 0입니다.')
      for (let i = 0; i < positions.count; i++) {
        point.fromBufferAttribute(positions, i)
        if (!bounds.containsPoint(point)) continue
        let armWeight = 0
        for (let j = 0; j < 4; j++)
          if (armBones[side].includes(joints.getComponent(i, j)))
            armWeight += weights.getComponent(i, j)
        if (armWeight < 0.5) continue
        offset.subVectors(point, elbow)
        const t = offset.dot(axis) / lengthSquared
        if (t < 0.05 || t > 0.92) continue
        axisPoint.copy(elbow).addScaledVector(axis, t)
        direction.subVectors(point, axisPoint).normalize()
        if (direction.lengthSq() === 0) continue
        tangent.crossVectors(along, direction).normalize()
        let inside = true
        for (const [axial, lateral] of [
          [0, 0],
          [-1, 0],
          [1, 0],
          [0, -1],
          [0, 1],
        ]) {
          origin
            .copy(point)
            .addScaledVector(along, axial * edgeMargin)
            .addScaledVector(tangent, lateral * edgeMargin)
            .addScaledVector(direction, -0.04)
          ray.set(origin, direction)
          ray.far = 0.08
          if (ray.intersectObject(target, false).length === 0) {
            inside = false
            break
          }
        }
        if (inside) covered[i] = 1
      }
    }
  } finally {
    material.dispose()
  }
  const hidden: number[] = []
  for (let i = 0; i < indices.count; i += 3) {
    if (
      covered[indices.getX(i)] &&
      covered[indices.getX(i + 1)] &&
      covered[indices.getX(i + 2)]
    )
      hidden.push(i / 3)
  }
  return {
    mesh_index: 0,
    primitive_index: 0,
    primitive_triangles: indices.count / 3,
    hide_triangle_indices: hidden,
  }
}
