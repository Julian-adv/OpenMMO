import * as THREE from 'three'

export interface BracerFitGeometry {
  mesh: THREE.SkinnedMesh
  positions: Float32Array
  normals?: Float32Array
  tangents?: Float32Array
  center: THREE.Vector3
  side: number
}

export function applyFemaleBracerFit(
  parts: BracerFitGeometry[],
  axes: THREE.Vector3[],
  values: Record<string, number>
) {
  for (const { mesh, positions, normals, tangents, center, side } of parts) {
    const attribute = mesh.geometry.getAttribute(
      'position'
    ) as THREE.BufferAttribute
    const scale = values.scale / 100
    const rotation = new THREE.Quaternion().setFromEuler(
      new THREE.Euler(
        THREE.MathUtils.degToRad(values['rotate-x']),
        side * THREE.MathUtils.degToRad(values['rotate-y']),
        side * THREE.MathUtils.degToRad(values['rotate-z'])
      )
    )
    for (const [index, key] of (
      ['arm-outward', 'arm-across', 'arm-twist'] as const
    ).entries()) {
      const axis = axes[index].clone()
      axis.y *= side
      axis.z *= side
      rotation.premultiply(
        new THREE.Quaternion().setFromAxisAngle(
          axis,
          THREE.MathUtils.degToRad(values[key])
        )
      )
    }
    if (side > 0) {
      for (const [index, key] of (
        [
          'left-rotate-outward',
          'left-rotate-across',
          'left-rotate-length',
        ] as const
      ).entries())
        rotation.premultiply(
          new THREE.Quaternion().setFromAxisAngle(
            axes[index],
            THREE.MathUtils.degToRad(values[key])
          )
        )
    }
    const armOffset = new THREE.Vector3()
    for (const [index, key] of (
      ['move-arm-outward', 'move-arm-across', 'move-arm-length'] as const
    ).entries())
      armOffset.addScaledVector(axes[index], values[key] / 100)
    armOffset.x *= side
    if (side > 0) {
      for (const [index, key] of (
        ['left-move-outward', 'left-move-across', 'left-move-length'] as const
      ).entries())
        armOffset.addScaledVector(axes[index], values[key] / 100)
    }
    const normal = mesh.geometry.getAttribute('normal') as
      | THREE.BufferAttribute
      | undefined
    const tangent = mesh.geometry.getAttribute('tangent') as
      | THREE.BufferAttribute
      | undefined
    const vector = new THREE.Vector3()
    for (let i = 0; i < attribute.count; i++) {
      const j = i * 3
      vector
        .set(
          positions[j] - center.x,
          positions[j + 1] - center.y,
          positions[j + 2] - center.z
        )
        .multiplyScalar(scale)
        .applyQuaternion(rotation)
      attribute.setXYZ(
        i,
        center.x + vector.x + armOffset.x + (side * values.outward) / 100,
        center.y + vector.y + armOffset.y + values.height / 100,
        center.z + vector.z + armOffset.z + values.depth / 100
      )
      if (normal && normals) {
        vector
          .set(normals[j], normals[j + 1], normals[j + 2])
          .applyQuaternion(rotation)
        normal.setXYZ(i, vector.x, vector.y, vector.z)
      }
      if (tangent && tangents) {
        const k = i * 4
        vector
          .set(tangents[k], tangents[k + 1], tangents[k + 2])
          .applyQuaternion(rotation)
        tangent.setXYZW(i, vector.x, vector.y, vector.z, tangents[k + 3])
      }
    }
    attribute.needsUpdate = true
    if (normal) normal.needsUpdate = true
    if (tangent) tangent.needsUpdate = true
    mesh.geometry.computeBoundingSphere()
  }
}
