import * as THREE from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { applyModularFingerPose, skinnedParts } from './modularCharacter'
import { femaleBootGround, standingMotions } from './female-boot-ground'
import { FEMALE_BOOTS_PARTS } from './femaleCharacter'

const gripMotions = [
  'idle1',
  'idle2',
  'idle3',
  'idle4',
  'idle5',
  'walk',
  'run',
  'combat_idle',
]
const LIFT_SAMPLES_PER_SECOND = 10
const LIFT_SAMPLES_PER_CLIP = 40
const LIFT_VERTEX_STRIDE = 8

export interface FemaleClipBake {
  name: string
  /** Pinned sword-grip finger tracks that replace the clip's own. */
  grip: THREE.KeyframeTrack[]
  /** Per boots part, its variants of foot rotations and Hips position, keyed like the bare tracks. */
  boots: Record<string, THREE.KeyframeTrack[]>
}

/** Asset-pipeline bake (tools/prepare-female-knight.mjs): heel pose, standing foot levelling, ground lift and sword grip. */
export function bakeFemaleClips(
  scene: THREE.Object3D,
  clips: THREE.AnimationClip[],
  grip: Record<string, [number, number, number, number]>
): FemaleClipBake[] {
  const rig = clone(scene)
  const meshes = skinnedParts(rig)
  const grounds = FEMALE_BOOTS_PARTS.map((part) =>
    femaleBootGround(
      rig,
      meshes.find((mesh) => mesh.userData.female_body)!,
      meshes.filter((mesh) => mesh.userData.female_part === part),
      scene.userData.female_parts[part].heel_pose,
      LIFT_VERTEX_STRIDE
    )
  )
  rig.updateMatrixWorld(true)
  const hips = rig.getObjectByName('Hips')!
  const up = new THREE.Vector3(0, 1, 0).applyQuaternion(
    hips.parent!.getWorldQuaternion(new THREE.Quaternion()).invert()
  )
  const profile = {
    rig_id: scene.userData.rig_id,
    finger_pose_sources: {},
    finger_poses: {
      sword_grip: {
        clips: gripMotions,
        quaternions: grip,
      },
    },
  }
  const pinned = new Set(Object.keys(grip).map((bone) => `${bone}.quaternion`))
  const mixer = new THREE.AnimationMixer(rig)
  const quaternion = new THREE.Quaternion()
  return clips.map((raw) => {
    const gripped = applyModularFingerPose(raw, profile, profile.rig_id)
    const grip =
      gripped === raw
        ? []
        : gripped.tracks.filter((track) => pinned.has(track.name))
    mixer.clipAction(raw).play()
    const track = (name: string) =>
      raw.tracks.find((track) => track.name === name)
    const variants: Record<string, THREE.KeyframeTrack[]> = {}
    for (const [v, ground] of grounds.entries()) {
      const sample = (time: number) => {
        ground.restore()
        mixer.setTime(Math.min(time, Math.max(0, raw.duration - 1e-4)))
      }
      const boots: THREE.KeyframeTrack[] = []
      const standing = new Map<number, Map<string, number[]>>()
      for (const { bone, rotation } of ground.offsets) {
        const name = `${bone.name}.quaternion`
        const source = track(name)
        if (!source) continue
        const values = new Float32Array(source.values.length)
        for (let i = 0; i < source.times.length; i++) {
          if (standingMotions.has(raw.name)) {
            let pose = standing.get(source.times[i])
            if (!pose) {
              sample(source.times[i])
              ground.pose(raw.name)
              pose = new Map(
                ground.offsets.map((state) => [
                  state.bone.name,
                  state.bone.quaternion.toArray(),
                ])
              )
              standing.set(source.times[i], pose)
            }
            values.set(pose.get(bone.name)!, i * 4)
          } else
            quaternion
              .fromArray(source.values, i * 4)
              .multiply(rotation)
              .toArray(values, i * 4)
        }
        boots.push(
          new THREE.QuaternionKeyframeTrack(name, source.times, values)
        )
      }
      const position = track('Hips.position')
      if (position) {
        const steps = Math.max(
          LIFT_SAMPLES_PER_CLIP,
          Math.ceil(raw.duration * LIFT_SAMPLES_PER_SECOND)
        )
        const lifts: number[] = []
        for (let i = 0; i <= steps; i++) {
          sample((i * raw.duration) / steps)
          lifts.push(ground.lift(raw.name))
        }
        const { times, values: source } = position
        const values = new Float32Array(source.length)
        for (let i = 0; i < times.length; i++) {
          const at = raw.duration > 0 ? (times[i] / raw.duration) * steps : 0
          const low = Math.min(Math.floor(at), steps)
          const high = Math.min(low + 1, steps)
          const lift = THREE.MathUtils.lerp(lifts[low], lifts[high], at - low)
          for (let axis = 0; axis < 3; axis++)
            values[i * 3 + axis] =
              source[i * 3 + axis] + up.getComponent(axis) * lift
        }
        boots.push(
          new THREE.VectorKeyframeTrack('Hips.position', times, values)
        )
      }
      ground.restore()
      variants[FEMALE_BOOTS_PARTS[v]] = boots
    }
    mixer.stopAllAction()
    mixer.uncacheClip(raw)
    return { name: raw.name, grip, boots: variants }
  })
}
