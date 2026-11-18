import * as THREE from 'three'

const SAMPLES_PER_SECOND = 10
// World directions on a rig facing +Z: the upper arm hangs, the forearm is
// level and forward for a right angle at the elbow, and the hand turns the
// shaft, its local X, upright.
const UPPER = new THREE.Vector3(-0.12, -1, -0.05).normalize()
const FOREARM = new THREE.Vector3(0, 0, 1)
const UP = new THREE.Vector3(0, 1, 0)
const ALONG = new THREE.Vector3(0, 1, 0)
const SHAFT = new THREE.Vector3(1, 0, 0)

/** The rig's own idle with the right arm holding a spear upright, so the rest
 *  of the body stands exactly as it does unarmed. Baked into the spear pack. */
export function spearRestClip(
  rig: THREE.Object3D,
  idle: THREE.AnimationClip,
  name = 'spear_idle'
): THREE.AnimationClip {
  const bones = ['RightArm', 'RightForeArm', 'RightHand'].map((bone) => {
    const node = rig.getObjectByName(bone)
    if (!node) throw new Error(`Missing bone ${bone}`)
    return node
  })
  const aims = [
    [ALONG, UPPER],
    [ALONG, FOREARM],
    [SHAFT, UP],
  ] as const
  const mixer = new THREE.AnimationMixer(rig)
  const action = mixer.clipAction(idle)
  action.play()
  const count = Math.max(2, Math.round(idle.duration * SAMPLES_PER_SECOND) + 1)
  const times = Array.from(
    { length: count },
    (_, i) => (idle.duration * i) / (count - 1)
  )
  const values = bones.map(() => [] as number[])
  const world = new THREE.Quaternion()
  const parent = new THREE.Quaternion()
  const turn = new THREE.Quaternion()
  const current = new THREE.Vector3()
  const previous = bones.map(() => new THREE.Quaternion())
  for (const [sample, time] of times.entries()) {
    // The last key repeats the first so the loop closes.
    mixer.setTime(sample === count - 1 ? 0 : time)
    for (const [i, bone] of bones.entries()) {
      rig.updateMatrixWorld(true)
      bone.getWorldQuaternion(world)
      current.copy(aims[i][0]).applyQuaternion(world)
      turn.setFromUnitVectors(current, aims[i][1])
      bone.parent!.getWorldQuaternion(parent)
      bone.quaternion.copy(parent.invert().multiply(turn.multiply(world)))
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
  const replaced = new Set(bones.map((bone) => `${bone.name}.quaternion`))
  return new THREE.AnimationClip(name, idle.duration, [
    ...idle.tracks
      .filter((track) => !replaced.has(track.name))
      .map((track) => track.clone()),
    ...bones.map(
      (bone, i) =>
        new THREE.QuaternionKeyframeTrack(
          `${bone.name}.quaternion`,
          times,
          values[i]
        )
    ),
  ])
}
