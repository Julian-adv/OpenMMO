import * as THREE from 'three'
import { createPeltCloth } from './pelt-cloth'
import { createPeltBend } from './pelt-bend'
import type { WindSample } from '../shaders/grass-material'

export interface PeltPhysics {
  kind: 'plate' | 'strap' | 'fur'
  bone: string
  pivot: [number, number, number]
  outward: [number, number, number]
  length: number
  cloth?: { columns: number; rows: number; pinned_rows: number }
  bend?: {
    droop: number
    contact?: boolean
    joint?: {
      at: number
      width: number
      damping: number
      stiffness: number
      inertia: number
    }
    crease?: {
      width: number
      angle: number
      damping: number
      stiffness: number
    }
  }
  motion?: {
    damping: number
    stiffness: number
    gravity: number
    inertia: number
    max_angle?: number
    min_angle?: number
    flex_clearance?: number
  }
  colliders: {
    bone: string
    center: [number, number, number]
    radii: [number, number, number]
  }[]
}

const STEP = 1 / 60
const MAX_ANGLE = 1.35
const rigs = new WeakMap<THREE.SkinnedMesh, PeltRig>()

export function createPeltRig(
  mesh: THREE.SkinnedMesh,
  config: PeltPhysics,
  updateWorldMatrices = true
) {
  const original = mesh.geometry
  const geometry = original.clone()
  const position = geometry.getAttribute('position') as THREE.BufferAttribute
  const rest = original.getAttribute('position') as THREE.BufferAttribute
  const normals = geometry.getAttribute('normal') as THREE.BufferAttribute
  const restNormals = original.getAttribute('normal') as THREE.BufferAttribute
  const tangents = geometry.getAttribute('tangent') as THREE.BufferAttribute
  const restTangents = original.getAttribute('tangent') as THREE.BufferAttribute
  const findBone = (name: string) => {
    const index = mesh.skeleton.bones.findIndex((bone) => bone.name === name)
    if (index < 0) throw new Error(`Missing pelt bone: ${name}`)
    return index
  }
  const bone = findBone(config.bone)
  const pivot = new THREE.Vector3(...config.pivot)
  const outward = new THREE.Vector3(...config.outward).normalize()
  const axis = new THREE.Vector3().crossVectors(
    outward,
    new THREE.Vector3(0, 1, 0)
  )
  const shapes = config.colliders.map((shape) => ({
    bone: findBone(shape.bone),
    rest: new THREE.Matrix4()
      .makeScale(...shape.radii)
      .setPosition(...shape.center),
    inverse: new THREE.Matrix4(),
    world: new THREE.Matrix4(),
    worldInverse: new THREE.Matrix4(),
  }))
  const cloth = config.cloth
    ? createPeltCloth(geometry, original, config)
    : null
  const bend = config.bend
    ? createPeltBend(
        geometry,
        pivot,
        outward,
        config.length,
        config.bend.droop,
        config.bend.joint,
        config.bend.crease
      )
    : null
  const frame = new THREE.Matrix4()
  const inverseFrame = new THREE.Matrix4()
  const matrix = new THREE.Matrix4()
  const rotation = new THREE.Quaternion()
  const local = new THREE.Vector3()
  const test = new THREE.Vector3()
  const next = new THREE.Vector3()
  const point = new THREE.Vector3()
  const previous = new THREE.Vector3()
  const target = new THREE.Vector3()
  const anchor = new THREE.Vector3()
  const lastAnchor = new THREE.Vector3()
  const anchorShift = new THREE.Vector3()
  const contactDirection = new THREE.Vector3()
  const maxAngle = config.motion?.max_angle ?? MAX_ANGLE
  const samples = new Map<string, THREE.Vector3>()
  const sample = (p: THREE.Vector3) => {
    if (cloth || p.y > pivot.y - 0.055) return
    samples.set(
      `${Math.round(p.x / 0.025)},${Math.round(p.y / 0.025)},${Math.round(p.z / 0.025)}`,
      p.clone().sub(pivot)
    )
  }
  for (let i = 0; i < rest.count; i++)
    sample(local.fromBufferAttribute(rest, i))
  const index = geometry.index
  if (index) {
    for (let i = 0; i < index.count; i += 3) {
      local.set(0, 0, 0)
      for (let j = 0; j < 3; j++)
        local.add(test.fromBufferAttribute(rest, index.getX(i + j)))
      sample(local.divideScalar(3))
    }
  }
  const skinIndex = geometry.getAttribute('skinIndex') as THREE.BufferAttribute
  const skinWeight = geometry.getAttribute(
    'skinWeight'
  ) as THREE.BufferAttribute
  for (let i = 0; i < rest.count; i++) {
    skinIndex.setXYZW(i, bone, 0, 0, 0)
    skinWeight.setXYZW(i, 1, 0, 0, 0)
  }
  for (const attr of [position, normals, tangents])
    attr?.setUsage(THREE.DynamicDrawUsage)
  const culled = mesh.frustumCulled
  mesh.geometry = geometry
  mesh.frustumCulled = false
  let initialized = false
  let active = true
  let accumulator = 0
  let angle = 0
  let drawnAngle = NaN
  let jointAngle = 0
  let jointVelocity = 0
  let drawnJointAngle = NaN
  let creaseAngle = 0
  let creaseVelocity = 0
  let drawnCreaseAngle = NaN
  let scale = 1

  function skin(target: THREE.Matrix4, index: number) {
    if (updateWorldMatrices)
      mesh.skeleton.bones[index].updateWorldMatrix(true, false)
    return target
      .copy(mesh.matrixWorld)
      .multiply(mesh.bindMatrixInverse)
      .multiply(mesh.skeleton.bones[index].matrixWorld)
      .multiply(mesh.skeleton.boneInverses[index])
      .multiply(mesh.bindMatrix)
  }

  function refresh() {
    if (updateWorldMatrices) {
      mesh.updateWorldMatrix(true, false)
      mesh.updateMatrixWorld(true)
    }
    skin(frame, bone)
    inverseFrame.copy(frame).invert()
    scale = frame.getMaxScaleOnAxis()
    anchor.copy(pivot).applyMatrix4(frame)
    for (const shape of shapes) {
      skin(matrix, shape.bone).multiply(shape.rest)
      shape.world.copy(matrix)
      shape.worldInverse.copy(matrix).invert()
      shape.inverse.copy(shape.worldInverse).multiply(frame)
    }
  }

  function contact(point: THREE.Vector3, direction: THREE.Vector3) {
    for (const shape of shapes) {
      local.copy(point).applyMatrix4(shape.worldInverse)
      const c = local.lengthSq() - 1.16
      if (c >= 0) continue
      test.copy(direction).transformDirection(shape.worldInverse)
      const b = local.dot(test)
      local.addScaledVector(test, -b + Math.sqrt(Math.max(0, b * b - c)))
      point.copy(local.applyMatrix4(shape.world))
    }
  }

  function penetrates(at: number) {
    rotation.setFromAxisAngle(axis, at)
    for (const p of samples.values()) {
      if (bend)
        bend.deform(
          local.copy(p).add(pivot),
          at,
          jointTarget(at),
          creaseTarget(at, jointTarget(at))
        )
      else local.copy(p).applyQuaternion(rotation).add(pivot)
      for (const shape of shapes) {
        if (config.bend?.contact && shape.bone !== bone) continue
        if (test.copy(local).applyMatrix4(shape.inverse).lengthSq() < 1.06)
          return true
      }
    }
    return false
  }

  function collisionAngle() {
    const minimum = config.motion?.min_angle ?? 0
    if (config.bend?.contact) {
      const thigh = shapes.find((shape) =>
        mesh.skeleton.bones[shape.bone].name.endsWith('UpLeg')
      )
      if (thigh && Math.abs(outward.x) > 0.5) {
        skin(matrix, thigh.bone).premultiply(inverseFrame)
        local.set(0, -1, 0).transformDirection(matrix)
        return THREE.MathUtils.clamp(
          Math.max(0, Math.atan2(local.dot(outward), -local.y)) +
            minimum +
            Math.abs(local.dot(axis)) * (config.motion?.flex_clearance ?? 0),
          0,
          maxAngle
        )
      }
    }
    if (!penetrates(minimum)) return minimum
    for (let step = 1; step <= 14; step++) {
      let high = Math.min(minimum + step * 0.1, maxAngle)
      if (penetrates(high)) continue
      let low = minimum + (step - 1) * 0.1
      for (let i = 0; i < 7; i++) {
        const middle = (low + high) / 2
        if (penetrates(middle)) low = middle
        else high = middle
      }
      return high
    }
    return maxAngle
  }

  function tip(target: THREE.Vector3, at: number) {
    return target
      .copy(outward)
      .multiplyScalar(Math.sin(at) * config.length)
      .add(pivot)
      .addScaledVector(THREE.Object3D.DEFAULT_UP, -Math.cos(at) * config.length)
      .applyMatrix4(frame)
  }

  function deform() {
    if (
      !config.bend?.contact &&
      Math.abs(angle - drawnAngle) < 1e-7 &&
      Math.abs(jointAngle - drawnJointAngle) < 1e-7 &&
      Math.abs(creaseAngle - drawnCreaseAngle) < 1e-7
    )
      return
    rotation.setFromAxisAngle(axis, angle)
    for (let i = 0; i < rest.count; i++) {
      local.fromBufferAttribute(rest, i)
      if (bend) bend.deform(local, angle, jointAngle, creaseAngle)
      else local.sub(pivot).applyQuaternion(rotation).add(pivot)
      position.setXYZ(i, local.x, local.y, local.z)
      if (restNormals && !bend) {
        local.fromBufferAttribute(restNormals, i)
        local.applyQuaternion(rotation)
        normals.setXYZ(i, local.x, local.y, local.z)
      }
      if (restTangents && !bend) {
        local.fromBufferAttribute(restTangents, i)
        local.applyQuaternion(rotation)
        tangents.setXYZ(i, local.x, local.y, local.z)
      }
    }
    for (const attr of [position, normals, tangents])
      if (attr) attr.needsUpdate = true
    bend?.constrain(angle, config.bend?.contact ? bendContact : undefined)
    drawnAngle = angle
    drawnJointAngle = jointAngle
    drawnCreaseAngle = creaseAngle
  }

  function bendContact(p: THREE.Vector3) {
    p.applyMatrix4(frame)
    contactDirection.copy(outward).transformDirection(frame)
    for (const shape of shapes) {
      local.copy(p).applyMatrix4(shape.worldInverse)
      if (local.lengthSq() >= 1.06) continue
      if (local.lengthSq() < 1e-10)
        local.copy(contactDirection).transformDirection(shape.worldInverse)
      local.setLength(Math.sqrt(1.06))
      p.copy(local.applyMatrix4(shape.world))
    }
    p.applyMatrix4(inverseFrame)
  }

  function reset() {
    if (!active) return
    refresh()
    if (cloth) cloth.reset(frame, contact)
    else {
      angle = collisionAngle()
      jointAngle = jointTarget()
      jointVelocity = 0
      creaseAngle = creaseTarget()
      creaseVelocity = 0
      tip(point, angle)
      previous.copy(point)
    }
    lastAnchor.copy(anchor)
    accumulator = 0
    initialized = true
    if (!cloth) deform()
  }

  function creaseTarget(at = angle, jointAt = jointAngle) {
    const crease = config.bend?.crease
    return crease
      ? THREE.MathUtils.clamp(
          crease.angle + at * 0.1 - jointAt * 0.15,
          0.1,
          0.4
        )
      : 0
  }

  function jointTarget(at = angle) {
    const joint = config.bend?.joint
    return joint ? -at * 0.7 - 0.08 : 0
  }

  function update(dt: number, wind: WindSample | null = null, step = STEP) {
    if (!active) return
    if (!mesh.visible) {
      initialized = false
      return
    }
    refresh()
    if (
      !initialized ||
      dt > 0.25 ||
      anchor.distanceTo(lastAnchor) > 1.5 * scale
    ) {
      reset()
      return
    }
    anchorShift.copy(anchor).sub(lastAnchor)
    lastAnchor.copy(anchor)
    if (!(dt > 0)) return
    if (cloth) {
      cloth.update(frame, dt, wind, contact, step)
      return
    }
    const inertia = config.motion?.inertia ?? 1
    point.addScaledVector(anchorShift, 1 - inertia)
    previous.addScaledVector(anchorShift, 1 - inertia)
    accumulator = Math.min(accumulator + dt, step * 4)
    if (accumulator + 1e-10 < step) return
    const minimum = collisionAngle()
    const strap = config.kind === 'strap'
    const damping = config.motion?.damping ?? (strap ? 7 : 12)
    const stiffness = config.motion?.stiffness ?? (strap ? 24 : 65)
    const gravity = config.motion?.gravity ?? 6
    const step2 = step * step
    const decay = Math.exp(-damping * step)
    tip(target, minimum)
    while (accumulator + 1e-10 >= step) {
      accumulator -= step
      next.copy(point).sub(previous).multiplyScalar(decay).add(point)
      next.addScaledVector(local.copy(target).sub(point), stiffness * step2)
      next.y -= gravity * scale * step2
      if (wind) {
        next.x += wind.windDirX * wind.windStrength * 0.3 * scale * step2
        next.z += wind.windDirZ * wind.windStrength * 0.3 * scale * step2
      }
      local.copy(next).applyMatrix4(inverseFrame).sub(pivot)
      const beforeAngle = angle
      angle = THREE.MathUtils.clamp(
        Math.atan2(local.dot(outward), -local.y),
        minimum,
        maxAngle
      )
      const joint = config.bend?.joint
      if (joint) {
        jointVelocity -= ((angle - beforeAngle) * joint.inertia) / step
        jointVelocity += (jointTarget() - jointAngle) * joint.stiffness * step
        jointVelocity *= Math.exp(-joint.damping * step)
        const proposed = jointAngle + jointVelocity * step
        jointAngle = THREE.MathUtils.clamp(proposed, -0.8, 0.35)
        if (proposed !== jointAngle) jointVelocity = 0
      }
      const crease = config.bend?.crease
      if (crease) {
        creaseVelocity +=
          (creaseTarget() - creaseAngle) * crease.stiffness * step
        creaseVelocity *= Math.exp(-crease.damping * step)
        const proposed = creaseAngle + creaseVelocity * step
        creaseAngle = THREE.MathUtils.clamp(proposed, 0, 0.45)
        if (proposed !== creaseAngle) creaseVelocity = 0
      }
      previous.copy(point)
      tip(point, angle)
    }
    deform()
  }

  function dispose() {
    if (!active) return
    active = false
    mesh.geometry = original
    mesh.frustumCulled = culled
    geometry.dispose()
    mesh.removeEventListener('removed', dispose)
    rigs.delete(mesh)
  }
  mesh.addEventListener('removed', dispose)
  return { update, reset, dispose }
}

export type PeltRig = ReturnType<typeof createPeltRig>

export function updatePeltPhysics(
  root: THREE.Object3D,
  dt: number,
  wind: WindSample | null = null,
  step = STEP,
  reset = false
) {
  const meshes: THREE.SkinnedMesh[] = []
  root.traverse((node) => {
    if (
      node instanceof THREE.SkinnedMesh &&
      node.userData.pelt_physics &&
      (node.visible || rigs.has(node))
    )
      meshes.push(node)
  })
  if (!meshes.length) return
  root.updateWorldMatrix(true, false)
  root.updateMatrixWorld(true)
  for (const node of meshes) {
    let rig = rigs.get(node)
    if (!rig && node.visible) {
      rig = createPeltRig(
        node,
        node.userData.pelt_physics as PeltPhysics,
        false
      )
      rigs.set(node, rig)
    }
    if (reset) rig?.reset()
    rig?.update(dt, wind, step)
  }
}

export function resetPeltPhysics(root: THREE.Object3D) {
  root.updateWorldMatrix(true, false)
  root.updateMatrixWorld(true)
  root.traverse((node) => {
    if (node instanceof THREE.SkinnedMesh) rigs.get(node)?.reset()
  })
}

export function disposePeltPhysics(root: THREE.Object3D) {
  root.traverse((node) => {
    if (node instanceof THREE.SkinnedMesh) rigs.get(node)?.dispose()
  })
}
