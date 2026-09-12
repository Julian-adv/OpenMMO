import { describe, expect, it } from 'vitest'
import {
  AnimationClip,
  QuaternionKeyframeTrack,
  VectorKeyframeTrack,
} from 'three'
import { createDaggerComboClip } from './daggerSkillAnimation'
import { DAGGER_SKILL } from '../data/daggerSkill'

describe('Double Slash animation', () => {
  it('preserves the source clips and returns to the ready pose with normalized rotations', () => {
    const first = new AnimationClip('dagger_inward', 2, [
      new VectorKeyframeTrack(
        'Hips.position',
        [0, 1, 2],
        [0, 1, 0, 0, 0.8, 0, 0, 1, 0]
      ),
      new QuaternionKeyframeTrack(
        'RightHand.quaternion',
        [0, 1, 2],
        [0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1]
      ),
    ])
    const second = first.clone()
    second.name = 'dagger_outward'
    const before = JSON.stringify(AnimationClip.toJSON(first))
    const result = createDaggerComboClip(first, second)
    expect(result.duration).toBe(DAGGER_SKILL.duration)
    expect(JSON.stringify(AnimationClip.toJSON(first))).toBe(before)
    const hip = result.tracks[0]
    expect(Array.from(hip.values.slice(-3))).toEqual([0, 1, 0])
    const rotations = result.tracks[1].values
    for (let i = 0; i < rotations.length; i += 4) {
      expect(Math.hypot(...rotations.slice(i, i + 4))).toBeCloseTo(1, 5)
    }
    expect(result.validate()).toBe(true)
  })
})
