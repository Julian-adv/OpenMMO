import * as THREE from 'three'

export function createPeltBend(
  geometry: THREE.BufferGeometry,
  pivot: THREE.Vector3,
  outward: THREE.Vector3,
  length: number,
  droop: number,
  joint?: { at: number; width: number },
  crease?: { width: number }
) {
  const axis = new THREE.Vector3().crossVectors(
    outward,
    THREE.Object3D.DEFAULT_UP
  )
  const rotation = new THREE.Quaternion()
  const foldRotation = new THREE.Quaternion()
  const creaseRotation = new THREE.Quaternion()
  const creasePivot = new THREE.Vector3()
  const creaseAxis = new THREE.Vector3()
  const positions = geometry.getAttribute('position') as THREE.BufferAttribute
  const groups = new Map<string, number>()
  const ids: number[] = []
  const rest: THREE.Vector3[] = []
  const points: THREE.Vector3[] = []
  for (let i = 0; i < positions.count; i++) {
    const p = new THREE.Vector3().fromBufferAttribute(positions, i)
    const key = p
      .toArray()
      .map((v) => Math.round(v * 1e6))
      .join(',')
    let id = groups.get(key)
    if (id === undefined) {
      id = rest.length
      groups.set(key, id)
      rest.push(p)
      points.push(p.clone())
    }
    ids.push(id)
  }
  const weights = rest.map((p) => (p.y > pivot.y - 0.025 ? 0 : 1))
  const edges = new Map<string, { a: number; b: number; length: number }>()
  const index = geometry.index!
  for (let i = 0; i < index.count; i += 3) {
    for (let j = 0; j < 3; j++) {
      const a = ids[index.getX(i + j)]
      const b = ids[index.getX(i + ((j + 1) % 3))]
      const key = a < b ? `${a},${b}` : `${b},${a}`
      if (a !== b) edges.set(key, { a, b, length: rest[a].distanceTo(rest[b]) })
    }
  }
  const links = [...edges.values()]
  const median = (values: number[]) => {
    values.sort((a, b) => a - b)
    return values[Math.floor(values.length / 2)]
  }
  const centerline = crease
    ? Array.from({ length: 9 }, (_, bin) => {
        const depth = (length * bin) / 8
        const ranked = rest
          .map((p) => {
            const offset = p.clone().sub(pivot)
            return {
              radial: offset.dot(outward),
              score: (offset.y + depth) ** 2 + offset.dot(axis) ** 2,
            }
          })
          .sort((a, b) => a.score - b.score)
        return median(
          ranked
            .slice(0, Math.max(3, Math.floor(rest.length * 0.02)))
            .map((p) => p.radial)
        )
      })
    : []
  const jointDepth = length * (joint?.at ?? 0.5)
  const jointWidth = length * (joint?.width ?? 0.12)
  const nearJoint = rest.filter(
    (p) => Math.abs(pivot.y - p.y - jointDepth) < length * 0.08
  )
  const radialValues = (nearJoint.length ? nearJoint : rest)
    .map((p) => p.clone().sub(pivot).dot(outward))
    .sort((a, b) => a - b)
  const jointPivot = outward
    .clone()
    .multiplyScalar(radialValues[Math.floor(radialValues.length / 2)])
  jointPivot.y = -jointDepth
  const delta = new THREE.Vector3()
  const smoothNormals = rest.map(() => new THREE.Vector3())
  const normal = new THREE.Vector3()
  const rigid = rest.map(() => new THREE.Vector3())
  const baseEdge = new THREE.Vector3()
  geometry.deleteAttribute('tangent')

  function orientation(depth: number, angle: number) {
    const curve = droop * Math.sin(angle)
    return angle - curve * THREE.MathUtils.clamp(depth / length, 0, 1)
  }

  function foldAmount(depth: number) {
    return THREE.MathUtils.smoothstep(
      depth,
      jointDepth - jointWidth / 2,
      jointDepth + jointWidth / 2
    )
  }

  function deform(
    point: THREE.Vector3,
    angle: number,
    jointAngle = 0,
    creaseAngle = 0
  ) {
    point.sub(pivot)
    const originalDepth = -point.y
    if (crease) {
      const bin = THREE.MathUtils.clamp((originalDepth / length) * 8, 0, 8)
      const low = Math.min(7, Math.floor(bin))
      const radius = THREE.MathUtils.lerp(
        centerline[low],
        centerline[low + 1],
        bin - low
      )
      const slope = ((centerline[low + 1] - centerline[low]) * 8) / length
      const across = point.dot(axis)
      const weight =
        THREE.MathUtils.smoothstep(originalDepth, 0.025, 0.08) *
        THREE.MathUtils.smoothstep(Math.abs(across), 0, crease.width)
      creasePivot.copy(outward).multiplyScalar(radius)
      creasePivot.y = -originalDepth
      creaseAxis
        .copy(THREE.Object3D.DEFAULT_UP)
        .addScaledVector(outward, -slope)
        .normalize()
      creaseRotation.setFromAxisAngle(
        creaseAxis,
        -Math.sign(across) * creaseAngle * weight
      )
      point.sub(creasePivot).applyQuaternion(creaseRotation).add(creasePivot)
    }
    if (joint) {
      const weight = foldAmount(originalDepth)
      foldRotation.setFromAxisAngle(axis, jointAngle * weight)
      rotation.setFromAxisAngle(axis, angle)
      return point
        .sub(jointPivot)
        .applyQuaternion(foldRotation)
        .add(jointPivot)
        .applyQuaternion(rotation)
        .add(pivot)
    }
    const depth = THREE.MathUtils.clamp(-point.y, 0, length)
    const curve = droop * Math.sin(angle)
    const theta = orientation(depth, angle)
    const radial = point.dot(outward)
    const across = point.dot(axis)
    let offset: number, down: number
    if (Math.abs(curve) < 1e-6) {
      offset = depth * Math.sin(angle)
      down = -depth * Math.cos(angle)
    } else {
      offset = (length / curve) * (Math.cos(theta) - Math.cos(angle))
      down = -(length / curve) * (Math.sin(angle) - Math.sin(theta))
    }
    const excess = -point.y - depth
    point
      .copy(axis)
      .multiplyScalar(across)
      .addScaledVector(
        outward,
        offset + radial * Math.cos(theta) + excess * Math.sin(theta)
      )
    point.y = down + radial * Math.sin(theta) - excess * Math.cos(theta)
    return point.add(pivot)
  }

  function constrain(angle: number, contact?: (point: THREE.Vector3) => void) {
    for (let i = 0; i < positions.count; i++)
      points[ids[i]].fromBufferAttribute(positions, i)
    for (let iteration = 0; iteration < (contact ? 512 : 128); iteration++) {
      if (contact && iteration % 8 === 0)
        for (let i = 0; i < points.length; i++)
          if (weights[i]) contact(points[i])
      for (const { a, b, length } of links) {
        const weight = weights[a] + weights[b]
        if (!weight) continue
        delta.copy(points[b]).sub(points[a])
        const distance = delta.length()
        if (distance < 1e-10) continue
        const target = THREE.MathUtils.clamp(
          distance,
          0,
          length * (contact ? 1.02 : 1.01)
        )
        delta.multiplyScalar((distance - target) / (distance * weight))
        if (weights[a]) points[a].add(delta)
        if (weights[b]) points[b].sub(delta)
      }
    }
    rotation.setFromAxisAngle(axis, angle)
    for (let i = 0; i < rest.length; i++)
      rigid[i].copy(rest[i]).sub(pivot).applyQuaternion(rotation).add(pivot)
    let amount = 1
    for (const { a, b, length } of links) {
      baseEdge.copy(rigid[b]).sub(rigid[a])
      delta.copy(points[b]).sub(points[a]).sub(baseEdge)
      const qa = delta.lengthSq()
      if (qa < 1e-16) continue
      const qb = 2 * baseEdge.dot(delta)
      const qc = baseEdge.lengthSq() - (length * 1.02 + 1e-6) ** 2
      amount = Math.min(
        amount,
        (-qb + Math.sqrt(Math.max(0, qb * qb - 4 * qa * qc))) / (2 * qa)
      )
    }
    amount = THREE.MathUtils.clamp(amount, 0, 1)
    for (let i = 0; i < points.length; i++)
      points[i].lerpVectors(rigid[i], points[i], amount)
    for (let i = 0; i < positions.count; i++) {
      const p = points[ids[i]]
      positions.setXYZ(i, p.x, p.y, p.z)
    }
    geometry.computeVertexNormals()
    const normals = geometry.getAttribute('normal') as THREE.BufferAttribute
    for (const n of smoothNormals) n.set(0, 0, 0)
    for (let i = 0; i < positions.count; i++)
      smoothNormals[ids[i]].add(normal.fromBufferAttribute(normals, i))
    for (const n of smoothNormals) n.normalize()
    for (let i = 0; i < positions.count; i++) {
      const n = smoothNormals[ids[i]]
      normals.setXYZ(i, n.x, n.y, n.z)
    }
  }

  return { deform, constrain }
}
