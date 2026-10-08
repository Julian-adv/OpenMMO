import * as THREE from 'three'

export interface RobePhysics {
  waist_height_m: number
  hem_height_m: number
  center_z_m: number
  waist_radii_m: [number, number]
  vertices: number[]
}

const COLUMNS = 4
const STEP = 1 / 30
const MAX_ANGLE = 1.65
const baseSkeletons = new WeakMap<THREE.SkinnedMesh, THREE.Skeleton>()
const geometries = new WeakMap<
  THREE.BufferGeometry,
  { geometry: THREE.BufferGeometry; users: number }
>()

export function robeBaseSkeleton(mesh: THREE.SkinnedMesh) {
  return baseSkeletons.get(mesh) ?? mesh.skeleton
}

function sharedGeometry(
  original: THREE.BufferGeometry,
  config: RobePhysics,
  hips: number,
  boneCount: number
) {
  let shared = geometries.get(original)
  if (shared) {
    shared.users++
    return shared
  }
  const geometry = original.clone()
  const positions = geometry.getAttribute('position')
  const joints = geometry.getAttribute('skinIndex') as THREE.BufferAttribute
  const weights = geometry.getAttribute('skinWeight') as THREE.BufferAttribute
  for (const i of config.vertices) {
    const x = positions.getX(i)
    const theta = Math.atan2(Math.abs(x), positions.getZ(i) - config.center_z_m)
    const across = (theta / Math.PI) * (COLUMNS - 1)
    const column = Math.min(Math.floor(across), COLUMNS - 2)
    const blend = across - column
    const amount = THREE.MathUtils.smoothstep(
      config.waist_height_m - positions.getY(i),
      0,
      0.06
    )
    const first = boneCount + (x >= 0 ? 0 : COLUMNS) + column
    joints.setXYZW(i, hips, first, first + 1, 0)
    weights.setXYZW(i, 1 - amount, amount * (1 - blend), amount * blend, 0)
  }
  geometry.computeBoundingSphere()
  shared = { geometry, users: 1 }
  geometries.set(original, shared)
  return shared
}

export function createRobeRig(mesh: THREE.SkinnedMesh, config: RobePhysics) {
  const original = mesh.geometry
  const originalSkeleton = mesh.skeleton
  const hips = originalSkeleton.bones.findIndex((bone) => bone.name === 'Hips')
  const legs = ['Left', 'Right'].map((side) => {
    const upper = originalSkeleton.bones.find(
      (bone) => bone.name === side + 'UpLeg'
    )
    const lower = originalSkeleton.bones.find(
      (bone) => bone.name === side + 'Leg'
    )
    if (!upper || !lower) throw new Error('Missing robe leg bones')
    return { upper, lower, direction: new THREE.Vector3() }
  })
  if (hips < 0) throw new Error('Missing robe hip bone')
  const hip = originalSkeleton.bones[hips]
  const toHip = originalSkeleton.boneInverses[hips]
    .clone()
    .multiply(mesh.bindMatrix)
  const down = new THREE.Vector3(0, -1, 0).transformDirection(toHip)
  const controls = Array.from({ length: COLUMNS * 2 }, (_, i) => {
    const side = Math.floor(i / COLUMNS)
    const angle = ((i % COLUMNS) / (COLUMNS - 1)) * Math.PI
    const outward = new THREE.Vector3(
      (side === 0 ? 1 : -1) * Math.sin(angle),
      0,
      Math.cos(angle)
    )
    const pivot = new THREE.Vector3(
      outward.x * config.waist_radii_m[0],
      config.waist_height_m,
      config.center_z_m + outward.z * config.waist_radii_m[1]
    ).applyMatrix4(toHip)
    outward.transformDirection(toHip)
    const bone = new THREE.Bone()
    bone.name = `RobeHem${i}`
    bone.position.copy(pivot)
    hip.add(bone)
    const inverse = new THREE.Matrix4()
      .makeTranslation(pivot)
      .premultiply(originalSkeleton.boneInverses[hips].clone().invert())
      .invert()
    return {
      bone,
      inverse,
      outward,
      axis: new THREE.Vector3().crossVectors(down, outward).normalize(),
      side,
      angle: 0,
      velocity: 0,
    }
  })
  const skeleton = new THREE.Skeleton(
    [...originalSkeleton.bones, ...controls.map((control) => control.bone)],
    [
      ...originalSkeleton.boneInverses,
      ...controls.map((control) => control.inverse),
    ]
  )
  const shared = sharedGeometry(
    original,
    config,
    hips,
    originalSkeleton.bones.length
  )
  const geometry = shared.geometry
  const originalBounds = mesh.boundingSphere
  mesh.boundingSphere = geometry.boundingSphere!.clone()
  mesh.boundingSphere.radius +=
    2 * (config.waist_height_m - config.hem_height_m)
  mesh.geometry = geometry
  mesh.bind(skeleton, mesh.bindMatrix.clone())
  baseSkeletons.set(mesh, originalSkeleton)
  const inverseHip = new THREE.Matrix4()
  const point = new THREE.Vector3()
  const anchor = new THREE.Vector3()
  const previousAnchor = new THREE.Vector3()
  let accumulator = 0
  let initialized = false
  let active = true

  function refresh() {
    inverseHip.copy(hip.matrixWorld).invert()
    anchor.setFromMatrixPosition(hip.matrixWorld)
    for (const leg of legs) {
      leg.direction
        .setFromMatrixPosition(leg.lower.matrixWorld)
        .sub(point.setFromMatrixPosition(leg.upper.matrixWorld))
        .transformDirection(inverseHip)
    }
  }

  function minimum(control: (typeof controls)[number]) {
    const leg = legs[control.side].direction
    const forward = Math.max(0, leg.dot(control.outward))
    if (forward < 0.005) return 0
    const margin = 0.12 * THREE.MathUtils.smoothstep(forward, 0, 0.3)
    return THREE.MathUtils.clamp(
      Math.atan2(forward, leg.dot(down)) + margin,
      0,
      MAX_ANGLE
    )
  }

  function draw() {
    for (const control of controls) {
      control.bone.quaternion.setFromAxisAngle(control.axis, control.angle)
      control.bone.updateMatrixWorld(true)
    }
  }

  function reset() {
    if (!active) return
    refresh()
    for (const control of controls) {
      control.angle = minimum(control)
      control.velocity = 0
    }
    previousAnchor.copy(anchor)
    accumulator = 0
    initialized = true
    draw()
  }

  function update(dt: number, _wind: unknown = null, requestedStep = STEP) {
    if (!active) return
    if (!mesh.visible) {
      initialized = false
      return
    }
    if (!initialized || dt > 0.25) {
      reset()
      return
    }
    const step = Math.max(STEP, Math.min(requestedStep, 1 / 15))
    accumulator += Math.max(0, dt)
    if (accumulator + 1e-10 < step) return
    accumulator %= step
    refresh()
    if (
      anchor.distanceToSquared(previousAnchor) >
      2.25 * hip.matrixWorld.getMaxScaleOnAxis() ** 2
    ) {
      reset()
      return
    }
    previousAnchor.copy(anchor)
    for (const control of controls) {
      const target = minimum(control)
      control.velocity =
        (control.velocity + (target - control.angle) * 65 * step) *
        Math.exp(-12 * step)
      control.angle = THREE.MathUtils.clamp(
        control.angle + control.velocity * step,
        target,
        MAX_ANGLE
      )
      if (
        (control.angle === target && control.velocity < 0) ||
        control.angle === MAX_ANGLE
      )
        control.velocity = 0
    }
    draw()
  }

  function dispose() {
    if (!active) return
    active = false
    mesh.geometry = original
    mesh.bind(originalSkeleton, mesh.bindMatrix.clone())
    baseSkeletons.delete(mesh)
    mesh.boundingSphere = originalBounds
    for (const control of controls) control.bone.removeFromParent()
    skeleton.dispose()
    if (--shared.users === 0) {
      geometry.dispose()
      geometries.delete(original)
    }
    mesh.removeEventListener('removed', dispose)
  }
  mesh.addEventListener('removed', dispose)
  return { update, reset, dispose }
}
