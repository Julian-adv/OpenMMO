import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { seatedFishingClip } from './seatedFishing'

function quat(bone: string, times: number[]): THREE.QuaternionKeyframeTrack {
  const values: number[] = []
  for (let i = 0; i < times.length; i++) values.push(0, 0, 0, i + 1)
  return new THREE.QuaternionKeyframeTrack(
    `.bones[${bone}].quaternion`,
    times,
    values
  )
}

const BONES = ['Hips', 'Spine', 'Spine1', 'RightArm', 'LeftUpLeg']

function clip(name: string, times: number[]) {
  return new THREE.AnimationClip(
    name,
    times[times.length - 1],
    BONES.map((bone) => quat(bone, times))
  )
}

const named = (merged: THREE.AnimationClip, bone: string) =>
  merged.tracks.find((t) => t.name === `.bones[${bone}].quaternion`)

describe('seatedFishingClip', () => {
  const fishing = clip('fishing_cast', [0, 0.5, 1])
  const seated = clip('sit_idle', [0, 1, 2])
  const merged = seatedFishingClip(fishing, seated)

  it('keeps the fishing clip’s identity so the finish check still matches', () => {
    expect(merged.name).toBe('fishing_cast')
    expect(merged.duration).toBe(fishing.duration)
  })

  it('casts with the upper body', () => {
    for (const bone of ['Spine1', 'RightArm']) {
      expect(named(merged, bone)?.times.length).toBe(3)
    }
  })

  it('holds the legs and waist in the seated pose', () => {
    // One key each: the seated half is frozen, not played alongside.
    for (const bone of ['Hips', 'Spine', 'LeftUpLeg']) {
      expect(named(merged, bone)?.times).toEqual(new Float32Array([0]))
    }
  })

  it('covers every bone exactly once', () => {
    expect(merged.tracks.length).toBe(BONES.length)
  })

  it('reuses the merge for the same pair', () => {
    expect(seatedFishingClip(fishing, seated)).toBe(merged)
  })

  it('reads raw GLB track names too', () => {
    const raw = new THREE.AnimationClip('fishing_idle', 1, [
      new THREE.QuaternionKeyframeTrack(
        'Hips.quaternion',
        [0, 1],
        [0, 0, 0, 1, 0, 0, 0, 1]
      ),
    ])
    const seatedRaw = seatedFishingClip(raw, seated)
    expect(seatedRaw.tracks.map((t) => t.name)).not.toContain('Hips.quaternion')
  })
})
