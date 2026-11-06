import * as THREE from 'three'

// Keep in sync with tools/outfits/female_outfit_adjust.py, which bakes the same transform.
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
// `forearm` fits move, turn and scale in the forearm's frame (gloves since 2026-11-05); others use the
// character's axes about the part's centre.
export type PartFit = Record<PartFitKey, number> & { frame?: 'forearm' }
// Elbow and wrist bind positions.
export type ArmJoints = [THREE.Vector3, THREE.Vector3]
// 1 is the character's left (+x), -1 the right.
export type PartSide = 1 | -1
export type PartFitter = (fitFor: (side: PartSide) => PartFit) => void
// Sleeve-end offsets in cm across the upper arm: forward, and outward from the body.
export type SleeveEnd = { forward: number; out: number }
export const SLEEVE_END_KEYS = ['forward', 'out'] as const

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

/**
 * Columns: outward (width, x), toward the elbow (length, y), forward (thickness, z). The right side's frame is the
 * mirror image, so the same values mirror the transform.
 */
export function forearmFrame([elbow, wrist]: ArmJoints, side: PartSide) {
  const along = elbow.clone().sub(wrist).normalize()
  const forward = new THREE.Vector3(0, 0, 1)
    .addScaledVector(along, -along.z)
    .normalize()
  const outward = new THREE.Vector3().crossVectors(along, forward)
  if (outward.x * side < 0) outward.negate()
  return new THREE.Matrix4().makeBasis(outward, along, forward)
}

function forearmFitMatrix(
  fit: PartFit,
  center: THREE.Vector3,
  side: PartSide,
  arm: ArmJoints
) {
  const [elbow, wrist] = arm
  const axis = wrist.clone().sub(elbow).normalize()
  const origin = elbow
    .clone()
    .addScaledVector(axis, center.clone().sub(elbow).dot(axis))
  const frame = forearmFrame(arm, side)
  const scale = fit.scale / 100
  const rotation = new THREE.Euler(
    THREE.MathUtils.degToRad(fit['rotate-x']),
    THREE.MathUtils.degToRad(fit['rotate-y']),
    THREE.MathUtils.degToRad(fit['rotate-z']),
    'XYZ'
  )
  return new THREE.Matrix4()
    .makeTranslation(origin.x, origin.y, origin.z)
    .multiply(frame)
    .multiply(
      new THREE.Matrix4().makeTranslation(
        fit['move-x'] / 100,
        fit['move-y'] / 100,
        fit['move-z'] / 100
      )
    )
    .multiply(new THREE.Matrix4().makeRotationFromEuler(rotation))
    .multiply(
      new THREE.Matrix4().makeScale(
        (scale * fit['scale-x']) / 100,
        (scale * fit['scale-y']) / 100,
        (scale * fit['scale-z']) / 100
      )
    )
    .multiply(frame.clone().transpose())
    .multiply(
      new THREE.Matrix4().makeTranslation(-origin.x, -origin.y, -origin.z)
    )
}

export function partFitMatrix(
  fit: PartFit,
  center: THREE.Vector3,
  side: PartSide,
  arm?: ArmJoints
) {
  if (fit.frame === 'forearm' && arm)
    return forearmFitMatrix(fit, center, side, arm)
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

// Keep in sync with sleeve_end in tools/outfits/female_outfit_adjust.py.
const SLEEVE_START = 0.09
const SLEEVE_RADIUS = 0.07
const SLEEVE_END_INSET = 0.015

/**
 * Moves each sleeve's end across the upper arm, linearly from the shoulder; `joints[side]` is the
 * shoulder and elbow bind position. `positions` are the unmoved vertices, which fix each sleeve's end.
 */
export function sleeveEndDeformer(
  positions: Float32Array[],
  joints: Record<PartSide, [THREE.Vector3, THREE.Vector3]>
) {
  const frames = ([1, -1] as const).map((side) => {
    const [shoulder, elbow] = joints[side]
    const axis = elbow.clone().sub(shoulder).normalize()
    const forward = new THREE.Vector3(0, 0, 1)
      .addScaledVector(axis, -axis.z)
      .normalize()
    const out = new THREE.Vector3()
      .crossVectors(forward, axis)
      .multiplyScalar(side)
    return { side, shoulder, axis, forward, out, end: -Infinity }
  })
  const point = new THREE.Vector3()
  const d = new THREE.Vector3()
  const along = (frame: (typeof frames)[number], p: THREE.Vector3) => {
    if (p.x * frame.side <= frame.shoulder.x * frame.side) return undefined
    d.subVectors(p, frame.shoulder)
    const s = d.dot(frame.axis)
    const r = d.addScaledVector(frame.axis, -s).length()
    return r < SLEEVE_RADIUS && s > SLEEVE_START ? s : undefined
  }
  for (const array of positions)
    for (let i = 0; i < array.length; i += 3) {
      point.fromArray(array, i)
      for (const frame of frames) {
        const s = along(frame, point)
        if (s !== undefined) frame.end = Math.max(frame.end, s)
      }
    }
  for (const frame of frames) frame.end -= SLEEVE_END_INSET
  return (p: THREE.Vector3, endFor: (side: PartSide) => SleeveEnd) => {
    for (const frame of frames) {
      const s = along(frame, p)
      if (s === undefined) continue
      const t = (s - SLEEVE_START) / (frame.end - SLEEVE_START)
      const { forward, out } = endFor(frame.side)
      p.addScaledVector(frame.forward, (t * forward) / 100).addScaledVector(
        frame.out,
        (t * out) / 100
      )
    }
  }
}

// Paired parts split by the sign of x and mirror the transform for the right side (x < 0).
// `after` deforms each vertex once the fit is applied; `arms` enables forearm-frame fits.
export function partFitter(
  meshes: THREE.SkinnedMesh[],
  paired: boolean,
  after?: (point: THREE.Vector3) => void,
  arms?: Record<PartSide, ArmJoints>
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
          side as PartSide,
          arms?.[side as PartSide]
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
        after?.(point)
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
