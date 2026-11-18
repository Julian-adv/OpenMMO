import * as THREE from 'three'

const SAMPLES_PER_SECOND = 30
const ALONG = new THREE.Vector3(0, 1, 0)
const SHAFT = new THREE.Vector3(1, 0, 0)
// World directions on a rig facing +Z, whose right side is -X.
const REST_UPPER = new THREE.Vector3(-0.12, -1, -0.05).normalize()
const FORWARD = new THREE.Vector3(0, 0, 1)
const UP = new THREE.Vector3(0, 1, 0)
// On the move the shaft leans this far forward and its butt rides this high.
const CARRY_LEAN = THREE.MathUtils.degToRad(30)
const CARRY_CLEARANCE = 0.16
// The hand keeps this share of the arm's own swing, and at the ends of that
// swing the shaft rocks this far from its lean.
const CARRY_SWING = 0.25
const CARRY_ROCK = THREE.MathUtils.degToRad(6)
// The elbow of the carrying arm points back, down and a little out.
const ELBOW_POLE = new THREE.Vector3(-0.35, -0.5, -1).normalize()

type Arm = [THREE.Object3D, THREE.Object3D, THREE.Object3D]

function armOf(rig: THREE.Object3D): Arm {
  return ['RightArm', 'RightForeArm', 'RightHand'].map((bone) => {
    const node = rig.getObjectByName(bone)
    if (!node) throw new Error(`Missing bone ${bone}`)
    return node
  }) as Arm
}

const world = new THREE.Quaternion()
const parent = new THREE.Quaternion()
const turn = new THREE.Quaternion()
const current = new THREE.Vector3()

function aim(
  rig: THREE.Object3D,
  bone: THREE.Object3D,
  axis: THREE.Vector3,
  direction: THREE.Vector3,
  roll?: THREE.Vector3
) {
  rig.updateMatrixWorld(true)
  bone.getWorldQuaternion(world)
  current.copy(axis).applyQuaternion(world)
  world.premultiply(turn.setFromUnitVectors(current, direction))
  if (roll) {
    // Turn about `direction` until the bone points where `roll` does.
    const flat = (v: THREE.Vector3) =>
      v.addScaledVector(direction, -v.dot(direction)).normalize()
    const from = flat(current.copy(ALONG).applyQuaternion(world))
    world.premultiply(turn.setFromUnitVectors(from, flat(roll.clone())))
  }
  bone.parent!.getWorldQuaternion(parent)
  bone.quaternion.copy(parent.invert().multiply(world))
}

/** Upper arm hanging, a right angle at the elbow, the shaft upright. */
function rest(rig: THREE.Object3D, [upper, fore, hand]: Arm) {
  aim(rig, upper, ALONG, REST_UPPER)
  aim(rig, fore, ALONG, FORWARD)
  aim(rig, hand, SHAFT, UP, FORWARD)
}

const position = (node: THREE.Object3D) =>
  node.getWorldPosition(new THREE.Vector3())

/** The hand held at `target` with the elbow behind it and the shaft leaning. */
function carry(
  rig: THREE.Object3D,
  arm: Arm,
  target: THREE.Vector3,
  lean: number
) {
  const [upper, fore, hand] = arm
  rig.updateMatrixWorld(true)
  const shoulder = position(upper)
  const a = position(fore).distanceTo(shoulder)
  const b = position(hand).distanceTo(position(fore))
  const reach = target.clone().sub(shoulder)
  const distance = Math.min(reach.length(), (a + b) * 0.98)
  reach.normalize()
  const along = (a * a - b * b + distance * distance) / (2 * distance)
  const out = Math.sqrt(Math.max(a * a - along * along, 0))
  const pole = ELBOW_POLE.clone()
    .addScaledVector(reach, -ELBOW_POLE.dot(reach))
    .normalize()
  const elbow = reach.clone().multiplyScalar(along).addScaledVector(pole, out)
  const forearm = reach.multiplyScalar(distance).sub(elbow).normalize()
  aim(rig, upper, ALONG, elbow.normalize())
  aim(rig, fore, ALONG, forearm)
  aim(
    rig,
    hand,
    SHAFT,
    new THREE.Vector3(0, Math.cos(lean), Math.sin(lean)),
    forearm
  )
}

function restHand(rig: THREE.Object3D, idle: THREE.AnimationClip) {
  const arm = armOf(rig)
  const mixer = new THREE.AnimationMixer(rig)
  mixer.clipAction(idle).play()
  mixer.setTime(0)
  rest(rig, arm)
  rig.updateMatrixWorld(true)
  const hand = position(arm[2])
  mixer.stopAllAction()
  mixer.uncacheRoot(rig)
  return hand
}

/** The rig's own clip with the right arm holding a spear, so the rest of the
 *  body moves exactly as it does unarmed. Baked into the spear pack.
 *
 *  Without `idle` the clip is the idle itself and the spear stands upright,
 *  its butt on the ground. With it the clip is a walk or run: the hand stays
 *  near where it rests in that idle, lowered for the leaning shaft, and keeps
 *  a share of the arm's swing so the spear rocks with the stride. */
export function spearRestClip(
  rig: THREE.Object3D,
  clip: THREE.AnimationClip,
  name = 'spear_idle',
  idle?: THREE.AnimationClip
): THREE.AnimationClip {
  const arm = armOf(rig)
  let target: THREE.Vector3 | undefined
  if (idle) {
    target = restHand(rig, idle)
    // The butt rests on the ground under the resting hand.
    target.y = target.y * Math.cos(CARRY_LEAN) + CARRY_CLEARANCE
  }
  const mixer = new THREE.AnimationMixer(rig)
  const action = mixer.clipAction(clip)
  action.play()
  const swing = (time: number) => {
    mixer.setTime(time)
    rig.updateMatrixWorld(true)
    return position(arm[2])
  }
  const count = Math.max(2, Math.round(clip.duration * SAMPLES_PER_SECOND) + 1)
  const times = Array.from(
    { length: count },
    (_, i) => (clip.duration * i) / (count - 1)
  )
  // The unarmed hand's path, about its own middle.
  const swings = target ? times.map((time) => swing(time)) : []
  const middle = swings
    .reduce((sum, hand) => sum.add(hand), new THREE.Vector3())
    .divideScalar(Math.max(swings.length, 1))
  const widest = Math.max(
    ...swings.map((hand) => Math.abs(hand.z - middle.z)),
    1e-6
  )
  const values = arm.map(() => [] as number[])
  const previous = arm.map(() => new THREE.Quaternion())
  for (const [sample, time] of times.entries()) {
    // The last key repeats the first so the loop closes.
    mixer.setTime(sample === count - 1 ? 0 : time)
    if (target) {
      const offset = swings[sample === count - 1 ? 0 : sample]
        .clone()
        .sub(middle)
      carry(
        rig,
        arm,
        target.clone().addScaledVector(offset, CARRY_SWING),
        // The point trails the hand.
        CARRY_LEAN - (CARRY_ROCK * offset.z) / widest
      )
    } else rest(rig, arm)
    for (const [i, bone] of arm.entries()) {
      if (sample > 0 && bone.quaternion.dot(previous[i]) < 0)
        bone.quaternion.set(
          -bone.quaternion.x,
          -bone.quaternion.y,
          -bone.quaternion.z,
          -bone.quaternion.w
        )
      previous[i].copy(bone.quaternion)
      values[i].push(...bone.quaternion.toArray())
    }
  }
  action.stop()
  mixer.uncacheRoot(rig)
  const replaced = new Set(arm.map((bone) => `${bone.name}.quaternion`))
  return new THREE.AnimationClip(name, clip.duration, [
    ...clip.tracks
      .filter((track) => !replaced.has(track.name))
      .map((track) => track.clone()),
    ...arm.map(
      (bone, i) =>
        new THREE.QuaternionKeyframeTrack(
          `${bone.name}.quaternion`,
          times,
          values[i]
        )
    ),
  ])
}
