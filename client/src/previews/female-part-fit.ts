import * as THREE from 'three'

// Keep in sync with tools/outfits/female_priest_adjust.py, which bakes the same transform.
export const PART_FIT_KEYS = [
  'move-x',
  'move-y',
  'move-z',
  'scale',
  'scale-x',
  'scale-y',
  'scale-z',
  'rotate-x',
  'rotate-y',
  'rotate-z',
] as const
export type PartFitKey = (typeof PART_FIT_KEYS)[number]
export type PartFit = Record<PartFitKey, number>
// 1 is the character's left (+x), -1 the right.
export type PartSide = 1 | -1
export type PartFitter = (fitFor: (side: PartSide) => PartFit) => void

export const PART_FIT_RANGES: Record<PartFitKey, [number, number, number]> = {
  'move-x': [-10, 10, 0],
  'move-y': [-10, 10, 0],
  'move-z': [-10, 10, 0],
  scale: [70, 130, 100],
  'scale-x': [70, 130, 100],
  'scale-y': [70, 130, 100],
  'scale-z': [70, 130, 100],
  'rotate-x': [-45, 45, 0],
  'rotate-y': [-45, 45, 0],
  'rotate-z': [-45, 45, 0],
}

export function partFitMatrix(
  fit: PartFit,
  center: THREE.Vector3,
  side: PartSide
) {
  const scale = fit.scale / 100
  const rotation = new THREE.Euler(
    THREE.MathUtils.degToRad(fit['rotate-x']),
    THREE.MathUtils.degToRad(fit['rotate-y']) * side,
    THREE.MathUtils.degToRad(fit['rotate-z']) * side,
    'XYZ'
  )
  return new THREE.Matrix4()
    .makeTranslation(
      center.x + (fit['move-x'] / 100) * side,
      center.y + fit['move-y'] / 100,
      center.z + fit['move-z'] / 100
    )
    .multiply(new THREE.Matrix4().makeRotationFromEuler(rotation))
    .multiply(
      new THREE.Matrix4().makeScale(
        (scale * fit['scale-x']) / 100,
        (scale * fit['scale-y']) / 100,
        (scale * fit['scale-z']) / 100
      )
    )
    .multiply(
      new THREE.Matrix4().makeTranslation(-center.x, -center.y, -center.z)
    )
}

type Source = {
  mesh: THREE.SkinnedMesh
  positions: Float32Array
  normals?: Float32Array
}

// Paired parts split by the sign of x and mirror the transform for the right side (x < 0).
export function partFitter(
  meshes: THREE.SkinnedMesh[],
  paired: boolean
): PartFitter {
  const sources: Source[] = meshes.map((mesh) => {
    const normal = mesh.geometry.getAttribute('normal')
    return {
      mesh,
      positions: new Float32Array(mesh.geometry.getAttribute('position').array),
      normals: normal && new Float32Array(normal.array),
    }
  })
  const boxes = new Map<number, THREE.Box3>()
  const sideOf = (x: number): PartSide => (!paired || x >= 0 ? 1 : -1)
  const point = new THREE.Vector3()
  for (const { positions } of sources)
    for (let i = 0; i < positions.length; i += 3) {
      point.fromArray(positions, i)
      const side = sideOf(point.x)
      if (!boxes.has(side)) boxes.set(side, new THREE.Box3())
      boxes.get(side)!.expandByPoint(point)
    }
  const centers = new Map(
    [...boxes].map(([side, box]) => [side, box.getCenter(new THREE.Vector3())])
  )
  return (fitFor) => {
    const matrices = new Map(
      [...centers].map(([side, center]) => {
        const matrix = partFitMatrix(
          fitFor(side as PartSide),
          center,
          side as PartSide
        )
        return [
          side,
          { matrix, normal: new THREE.Matrix3().getNormalMatrix(matrix) },
        ]
      })
    )
    const normal = new THREE.Vector3()
    for (const { mesh, positions, normals } of sources) {
      const position = mesh.geometry.getAttribute(
        'position'
      ) as THREE.BufferAttribute
      const normalAttribute = mesh.geometry.getAttribute('normal') as
        | THREE.BufferAttribute
        | undefined
      for (let i = 0; i < positions.length; i += 3) {
        point.fromArray(positions, i)
        const transform = matrices.get(sideOf(point.x))!
        point.applyMatrix4(transform.matrix)
        position.setXYZ(i / 3, point.x, point.y, point.z)
        if (normals && normalAttribute) {
          normal
            .fromArray(normals, i)
            .applyMatrix3(transform.normal)
            .normalize()
          normalAttribute.setXYZ(i / 3, normal.x, normal.y, normal.z)
        }
      }
      position.needsUpdate = true
      if (normalAttribute) normalAttribute.needsUpdate = true
      mesh.geometry.computeBoundingBox()
      mesh.geometry.computeBoundingSphere()
    }
  }
}
