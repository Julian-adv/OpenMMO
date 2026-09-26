import { describe, expect, it, vi } from 'vitest'
import * as THREE from 'three'
import { retargetClip } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { retargetClipAsync } from './retargetClipAsync'

const { yieldIfDue } = vi.hoisted(() => ({ yieldIfDue: vi.fn(async () => {}) }))
vi.mock('./frameYield', () => ({ createFrameYielder: () => yieldIfDue }))

function rig(scale: number) {
  const hips = new THREE.Bone()
  hips.name = 'Hips'
  hips.position.y = scale
  const arm = new THREE.Bone()
  arm.name = 'Arm'
  arm.position.set(scale, scale, 0)
  hips.add(arm)
  const mesh = new THREE.SkinnedMesh(new THREE.BufferGeometry())
  mesh.add(hips)
  mesh.bind(new THREE.Skeleton([hips, arm]))
  mesh.updateMatrixWorld(true)
  return mesh
}

describe('retargetClipAsync', () => {
  it.each([0.5, 2])(
    'matches Three.js sampled tracks for rig scale %s',
    async (scale) => {
      const times = [0, 0.25, 0.5, 0.75, 1]
      const rotations = times.flatMap((time) =>
        new THREE.Quaternion()
          .setFromAxisAngle(new THREE.Vector3(0, 0, 1), time)
          .toArray()
      )
      const clip = new THREE.AnimationClip('walk', 1, [
        new THREE.VectorKeyframeTrack(
          'Hips.position',
          times,
          times.flatMap((time) => [time, 1 + time, 0])
        ),
        new THREE.QuaternionKeyframeTrack('Arm.quaternion', times, rotations),
      ])
      const options = {
        names: { Hips: 'Hips', Arm: 'Arm' },
        hip: 'Hips',
        preserveBoneMatrix: true,
        useTargetMatrix: false,
      }
      const expected = retargetClip(rig(scale), rig(1), clip, { ...options })
      yieldIfDue.mockClear()
      const actual = await retargetClipAsync(rig(scale), rig(1), clip, {
        ...options,
      })
      expect(actual.name).toBe(expected.name)
      expect(actual.duration).toBe(expected.duration)
      expect(actual.tracks.map((track) => track.name)).toEqual(
        expected.tracks.map((track) => track.name)
      )
      for (let i = 0; i < expected.tracks.length; i++) {
        expect(actual.tracks[i].times).toEqual(expected.tracks[i].times)
        expect(actual.tracks[i].values).toEqual(expected.tracks[i].values)
      }
      expect(yieldIfDue).toHaveBeenCalledTimes(times.length)
    }
  )
})
