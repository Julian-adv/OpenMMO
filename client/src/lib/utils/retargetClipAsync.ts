import {
  AnimationClip,
  AnimationMixer,
  QuaternionKeyframeTrack,
  VectorKeyframeTrack,
  type KeyframeTrack,
  type SkinnedMesh,
} from 'three'
import {
  retarget,
  type RetargetOptions,
} from 'three/examples/jsm/utils/SkeletonUtils.js'
import { createFrameYielder } from './frameYield'

/** Match SkeletonUtils.retargetClip while yielding between sampled poses. */
export async function retargetClipAsync(
  target: SkinnedMesh,
  source: SkinnedMesh,
  clip: AnimationClip,
  options: RetargetOptions
): Promise<AnimationClip> {
  const fps =
    Math.max(...clip.tracks.map((track) => track.times.length)) / clip.duration
  const frames = Math.round(clip.duration * (fps / 1000) * 1000)
  const delta = clip.duration / (frames - 1)
  const mixer = new AnimationMixer(source)
  const tracks: KeyframeTrack[] = []
  const bindings = target.skeleton.bones.flatMap((bone) => {
    const name = options.names?.[bone.name] || bone.name
    if (!source.skeleton.bones.some((entry) => entry.name === name)) return []
    const times = new Float32Array(frames)
    const rotations = new Float32Array(frames * 4)
    const positions = name === options.hip ? new Float32Array(frames * 3) : null
    if (positions) {
      tracks.push(
        new VectorKeyframeTrack(
          `.bones[${bone.name}].position`,
          times,
          positions
        )
      )
    }
    tracks.push(
      new QuaternionKeyframeTrack(
        `.bones[${bone.name}].quaternion`,
        times,
        rotations
      )
    )
    return [{ bone, times, rotations, positions }]
  })
  const yieldIfDue = createFrameYielder()
  mixer.clipAction(clip).play()
  mixer.update(0)
  source.updateMatrixWorld()
  try {
    for (let frame = 0; frame < frames; frame++) {
      await yieldIfDue()
      retarget(target, source, options)
      for (const { bone, times, rotations, positions } of bindings) {
        times[frame] = frame * delta
        bone.quaternion.toArray(rotations, frame * 4)
        if (positions) bone.position.toArray(positions, frame * 3)
      }
      mixer.update(frame === frames - 2 ? delta - 0.0000001 : delta)
      source.updateMatrixWorld()
    }
    return new AnimationClip(clip.name, -1, tracks)
  } finally {
    mixer.uncacheAction(clip)
  }
}
