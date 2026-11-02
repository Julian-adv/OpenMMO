import * as THREE from 'three'

export const standingMotions = new Set([
  'rest',
  'idle1',
  'idle_armed',
  'combat_idle',
  'torch_idle1',
  'torch_combat_idle',
  'torch_sword_idle1',
])

const parentRotation = new THREE.Quaternion()
const worldRotation = new THREE.Quaternion()
const correction = new THREE.Quaternion()
const lateral = new THREE.Vector3()
const horizontal = new THREE.Vector3()
const heading = new THREE.Vector3()
const level = new THREE.Vector3()
const up = new THREE.Vector3(0, 1, 0)

function setWorldQuaternion(bone: THREE.Bone, rotation: THREE.Quaternion) {
  bone.quaternion.copy(
    bone.parent!.getWorldQuaternion(parentRotation).invert().multiply(rotation)
  )
  bone.updateMatrixWorld(true)
}

export function femaleBootGround(
  model: THREE.Object3D,
  body: THREE.SkinnedMesh,
  boots: THREE.SkinnedMesh[],
  config: {
    local_quaternion_offsets: Record<string, number[]>
    seated_hand_lift_m?: number
  },
  stride = 1
) {
  const offsets = Object.entries(config.local_quaternion_offsets).map(
    ([name, values]) => {
      const bone = model.getObjectByName(name)
      if (!(bone instanceof THREE.Bone) || values.length !== 4)
        throw new Error('부츠의 발 자세 정보가 올바르지 않습니다.')
      return {
        bone,
        rotation: new THREE.Quaternion().fromArray(values),
        original: bone.quaternion.clone(),
      }
    }
  )
  const legStates = ['LeftUpLeg', 'LeftLeg', 'RightUpLeg', 'RightLeg'].map(
    (name) => {
      const bone = model.getObjectByName(name) as THREE.Bone
      return { bone, original: bone.quaternion.clone() }
    }
  )
  const armStates = config.seated_hand_lift_m
    ? ['Left', 'Right'].flatMap((side) =>
        ['Arm', 'ForeArm', 'Hand'].map((suffix) => {
          const bone = model.getObjectByName(`${side}${suffix}`) as THREE.Bone
          return { bone, original: bone.quaternion.clone() }
        })
      )
    : []
  const states = [...offsets, ...legStates, ...armStates]
  const footAxes = offsets
    .filter(({ bone }) => bone.name.endsWith('Foot'))
    .map(({ bone, rotation }) => {
      const inverse =
        body.skeleton.boneInverses[body.skeleton.bones.indexOf(bone)]
      const undo = rotation.clone().invert()
      const axis = (x: number, z: number) =>
        new THREE.Vector3(x, 0, z)
          .transformDirection(inverse)
          .applyQuaternion(undo)
      return { bone, direction: axis(1, 0), forward: axis(0, 1) }
    })
  let applied = false
  const sample = (mesh: THREE.SkinnedMesh, height: number) => {
    const position = mesh.geometry.attributes.position
    const indices: number[] = []
    for (let i = 0; i < position.count; i += stride)
      if (position.getY(i) < height) indices.push(i)
    return { mesh, indices }
  }
  const feet = sample(body, 0.144)
  const soles = boots.map((mesh) => sample(mesh, 0.324))
  const solesBySide = Object.fromEntries(
    ['Left', 'Right'].map((side) => [
      side,
      soles.map(({ mesh, indices }) => ({
        mesh,
        indices: indices.filter((index) =>
          side === 'Left'
            ? mesh.geometry.attributes.position.getX(index) > 0
            : mesh.geometry.attributes.position.getX(index) < 0
        ),
      })),
    ])
  )
  const point = new THREE.Vector3()
  const aim = (bone: THREE.Bone, child: THREE.Bone, target: THREE.Vector3) => {
    const origin = bone.getWorldPosition(new THREE.Vector3())
    const rotation = new THREE.Quaternion()
      .setFromUnitVectors(
        child.getWorldPosition(new THREE.Vector3()).sub(origin).normalize(),
        target.clone().sub(origin).normalize()
      )
      .multiply(bone.getWorldQuaternion(worldRotation))
    setWorldQuaternion(bone, rotation)
  }
  const liftChain = (
    upper: THREE.Bone,
    lower: THREE.Bone,
    end: THREE.Bone,
    amount: number
  ) => {
    const origin = upper.getWorldPosition(new THREE.Vector3())
    const middlePosition = lower.getWorldPosition(new THREE.Vector3())
    const endPosition = end.getWorldPosition(new THREE.Vector3())
    const rotation = end.getWorldQuaternion(new THREE.Quaternion())
    const a = origin.distanceTo(middlePosition),
      b = middlePosition.distanceTo(endPosition)
    const target = endPosition.clone().add(new THREE.Vector3(0, amount, 0))
    const direction = target.clone().sub(origin)
    const distance = THREE.MathUtils.clamp(
      direction.length(),
      Math.abs(a - b) + 1e-6,
      a + b - 1e-6
    )
    direction.normalize()
    const along = (a * a - b * b + distance * distance) / (2 * distance)
    const bend = middlePosition.clone().sub(origin)
    bend.addScaledVector(direction, -bend.dot(direction)).normalize()
    const joint = origin
      .clone()
      .addScaledVector(direction, along)
      .addScaledVector(bend, Math.sqrt(Math.max(0, a * a - along * along)))
    aim(upper, lower, joint)
    aim(lower, end, target)
    setWorldQuaternion(end, rotation)
  }
  const minimum = (entries: typeof soles) => {
    let result = Infinity
    for (const { mesh, indices } of entries)
      for (const index of indices)
        result = Math.min(result, mesh.getVertexPosition(index, point).y)
    return result
  }
  const restore = () => {
    if (!applied) return
    for (const { bone, original } of states) bone.quaternion.copy(original)
    applied = false
  }
  const pose = (motion: string) => {
    for (const state of states) state.original.copy(state.bone.quaternion)
    for (const state of offsets) state.bone.quaternion.multiply(state.rotation)
    applied = true
    model.updateMatrixWorld(true)
    if (standingMotions.has(motion)) {
      for (const { bone, direction, forward } of footAxes) {
        const rotation = bone.getWorldQuaternion(worldRotation)
        lateral.copy(direction).applyQuaternion(rotation)
        horizontal
          .crossVectors(up, heading.copy(forward).applyQuaternion(rotation))
          .normalize()
        rotation.premultiply(correction.setFromUnitVectors(lateral, horizontal))
        heading.copy(forward).applyQuaternion(rotation)
        rotation.premultiply(
          correction.setFromUnitVectors(
            heading,
            level.copy(heading).setY(0).normalize()
          )
        )
        setWorldQuaternion(bone, rotation)
      }
    }
  }
  const apply = (seatOffset?: number, motion = 'rest') => {
    restore()
    model.updateMatrixWorld(true)
    const originalFloor = minimum([feet])
    pose(motion)
    if (seatOffset !== undefined) {
      for (const side of ['Left', 'Right']) {
        const entries = solesBySide[side]
        const amount = Math.max(0, -minimum(entries) - seatOffset)
        if (amount > 0)
          liftChain(
            model.getObjectByName(`${side}UpLeg`) as THREE.Bone,
            model.getObjectByName(`${side}Leg`) as THREE.Bone,
            model.getObjectByName(`${side}Foot`) as THREE.Bone,
            amount
          )
        if (config.seated_hand_lift_m)
          liftChain(
            model.getObjectByName(`${side}Arm`) as THREE.Bone,
            model.getObjectByName(`${side}ForeArm`) as THREE.Bone,
            model.getObjectByName(`${side}Hand`) as THREE.Bone,
            config.seated_hand_lift_m
          )
      }
      model.updateMatrixWorld(true)
      return 0
    }
    const lift = originalFloor - minimum(soles)
    return Number.isFinite(lift) ? Math.max(0, lift) : 0
  }
  return {
    offsets,
    restore,
    apply,
    lift: (motion: string) => apply(undefined, motion),
    pose(motion: string) {
      restore()
      pose(motion)
    },
  }
}
