import { AnimationClip, MathUtils, Quaternion } from 'three'
import { DAGGER_SKILL } from '../data/daggerSkill'

export function createDaggerComboClip(
  inward: AnimationClip,
  outward: AnimationClip
): AnimationClip {
  const frames = Math.ceil(DAGGER_SKILL.duration * 120)
  const times = Float32Array.from(
    { length: frames + 1 },
    (_, i) => (i * DAGGER_SKILL.duration) / frames
  )
  const tracks = inward.tracks.map((track) => {
    const blendedRotation = new Quaternion()
    const nextRotation = new Quaternion()
    const other = outward.tracks.find(
      (candidate) => candidate.name === track.name
    )
    if (!other) throw new Error(`Missing dagger track: ${track.name}`)
    const first = track.InterpolantFactoryMethodLinear()
    const second = other.InterpolantFactoryMethodLinear()
    const size = track.getValueSize()
    const values = new Float32Array(times.length * size)
    const ready = track.values.slice(0, size)
    for (let i = 0; i < times.length; i++) {
      const time = times[i]
      const enter = MathUtils.smoothstep(time, 0, 0.065)
      const join = MathUtils.smoothstep(time, 0.28, 0.36)
      const recovery = MathUtils.smoothstep(time, 0.6, DAGGER_SKILL.duration)
      const weights = [
        1 - enter + recovery * enter,
        enter * (1 - join) * (1 - recovery),
        enter * join * (1 - recovery),
      ]
      const poses = [
        ready,
        first.evaluate(
          MathUtils.lerp(22 / 24, 36 / 24, Math.min(time / 0.34, 1))
        ),
        second.evaluate(
          MathUtils.lerp(
            16 / 24,
            29 / 24,
            MathUtils.clamp((time - 0.28) / 0.32, 0, 1)
          )
        ),
      ]
      const offset = i * size
      let accumulated = 0
      for (let pose = 0; pose < poses.length; pose++) {
        const weight = weights[pose]
        if (weight <= 0) continue
        if (track.name.endsWith('.quaternion')) {
          if (accumulated === 0) values.set(poses[pose], offset)
          else {
            blendedRotation
              .fromArray(values, offset)
              .slerp(
                nextRotation.fromArray(poses[pose]),
                weight / (accumulated + weight)
              )
              .toArray(values, offset)
          }
        } else {
          for (let k = 0; k < size; k++)
            values[offset + k] += poses[pose][k] * weight
        }
        accumulated += weight
      }
    }
    const result = track.clone()
    result.times = times
    result.values = values
    return result
  })
  return new AnimationClip(
    DAGGER_SKILL.clip,
    DAGGER_SKILL.duration,
    tracks
  ).optimize()
}
