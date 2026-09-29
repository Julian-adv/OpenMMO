import { describe, it, expect, vi } from 'vitest'
import * as THREE from 'three'
import * as retargeting from './retargetClipAsync'
import { yieldTask } from './frameYield'
import {
  computeCorpseGroundOffset,
  computeSoleGroundOffset,
  groundRetargetedClips,
  retargetAnimationsForCharacterModel,
} from './characterAnimationUtils'

// A one-bone skinned mesh whose vertices sit at the given local Ys, all fully
// weighted to the bone. `posedBoneY` moves the bone AFTER bind, standing in for
// a death clip that poses the skeleton away from its rest pose.
function makeSkinned(localYs: number[], posedBoneY = 0): THREE.Group {
  const n = localYs.length
  const positions = new Float32Array(n * 3)
  const skinIndex = new Uint16Array(n * 4)
  const skinWeight = new Float32Array(n * 4)
  for (let i = 0; i < n; i++) {
    positions[i * 3 + 1] = localYs[i]
    skinWeight[i * 4] = 1
  }
  const geom = new THREE.BufferGeometry()
  geom.setAttribute('position', new THREE.BufferAttribute(positions, 3))
  geom.setAttribute('skinIndex', new THREE.Uint16BufferAttribute(skinIndex, 4))
  geom.setAttribute(
    'skinWeight',
    new THREE.Float32BufferAttribute(skinWeight, 4)
  )

  const bone = new THREE.Bone()
  const mesh = new THREE.SkinnedMesh(geom, new THREE.MeshBasicMaterial())
  mesh.add(bone)
  mesh.bind(new THREE.Skeleton([bone]))

  const root = new THREE.Group()
  root.add(mesh)
  bone.position.y = posedBoneY
  root.updateMatrixWorld(true)
  return root
}

const CLEARANCE = 0.01

describe('computeCorpseGroundOffset', () => {
  it('ignores body regions hidden by modular clothes when grounding', () => {
    const root = makeSkinned([0.1])
    const mesh = root.children[0] as THREE.SkinnedMesh
    mesh.skeleton.bones[0].name = 'RightFoot'
    const hidden = makeSkinned([-2])
    ;(hidden.children[0] as THREE.SkinnedMesh).skeleton.bones[0].name =
      'RightFoot'
    hidden.visible = false
    root.add(hidden)
    expect(computeCorpseGroundOffset(root)).toBeCloseTo(-0.1 + CLEARANCE)
    expect(computeSoleGroundOffset(root)).toBeCloseTo(-0.1 + 0.005)
  })

  it('grounds on the lowest vertex, wherever the body sits', () => {
    // One vertex dangling to -0.50 m (like a tail tip) below a body at 0.30 m.
    const root = makeSkinned([-0.5, ...new Array(100).fill(0.3)])
    expect(computeCorpseGroundOffset(root)).toBeCloseTo(0.5 + CLEARANCE, 5)
  })

  it('rests a uniformly-raised corpse on the floor', () => {
    const root = makeSkinned(new Array(100).fill(0.25))
    expect(computeCorpseGroundOffset(root)).toBeCloseTo(-0.25 + CLEARANCE, 5)
  })

  it('measures the current pose, not the bind pose', () => {
    const flat = new Array(100).fill(0.2)
    const rest = computeCorpseGroundOffset(makeSkinned(flat, 0))
    const posed = computeCorpseGroundOffset(makeSkinned(flat, 0.5))
    expect(posed).toBeCloseTo(rest - 0.5, 5)
  })

  it('returns 0 when there is no skinned geometry', () => {
    const root = new THREE.Group()
    root.add(
      new THREE.Mesh(new THREE.BoxGeometry(), new THREE.MeshBasicMaterial())
    )
    expect(computeCorpseGroundOffset(root)).toBe(0)
  })
})

// A rig whose bone offsets are all `scale` times the 1 m reference. The
// animation packs are authored on a 10x rig under an Armature scaled 0.1, so
// their raw bone positions are 10x anything rigged at 1:1 — playing them
// unretargeted stretches the limbs to that size.
function makeRig(scale: number, hipY = 1.0): THREE.Group {
  const hips = new THREE.Bone()
  hips.name = 'Hips'
  hips.position.set(0, hipY * scale, 0)
  const arm = new THREE.Bone()
  arm.name = 'LeftArm'
  arm.position.set(0, 0.25 * scale, 0)
  const hand = new THREE.Bone()
  hand.name = 'LeftHand'
  hand.position.set(0, 0.3 * scale, 0)
  hips.add(arm)
  arm.add(hand)

  const bones = [hips, arm, hand]
  const positions = new Float32Array(bones.length * 3)
  const skinIndex = new Uint16Array(bones.length * 4)
  const skinWeight = new Float32Array(bones.length * 4)
  for (let i = 0; i < bones.length; i++) {
    positions[i * 3 + 1] = i * 0.25 * scale
    skinIndex[i * 4] = i
    skinWeight[i * 4] = 1
  }
  const geom = new THREE.BufferGeometry()
  geom.setAttribute('position', new THREE.BufferAttribute(positions, 3))
  geom.setAttribute('skinIndex', new THREE.Uint16BufferAttribute(skinIndex, 4))
  geom.setAttribute(
    'skinWeight',
    new THREE.Float32BufferAttribute(skinWeight, 4)
  )

  const mesh = new THREE.SkinnedMesh(geom, new THREE.MeshBasicMaterial())
  mesh.add(hips)
  mesh.bind(new THREE.Skeleton(bones))

  const root = new THREE.Group()
  root.add(mesh)
  root.updateMatrixWorld(true)
  return root
}

function packLikeSource(): THREE.Group {
  const root = makeRig(10)
  root.scale.setScalar(0.1)
  root.updateMatrixWorld(true)
  return root
}

function packClip(name: string): THREE.AnimationClip {
  return new THREE.AnimationClip(name, 1, [
    new THREE.VectorKeyframeTrack(
      'Hips.position',
      [0, 1],
      [0, 10, 0, 0, 11, 0]
    ),
    new THREE.VectorKeyframeTrack(
      'LeftArm.position',
      [0, 1],
      [0, 2.5, 0, 0, 2.5, 0]
    ),
    new THREE.VectorKeyframeTrack(
      'LeftHand.position',
      [0, 1],
      [0, 3, 0, 0, 3, 0]
    ),
    new THREE.QuaternionKeyframeTrack(
      'LeftArm.quaternion',
      [0, 1],
      [0, 0, 0, 1, 0, 0, 0, 1]
    ),
  ])
}

describe('retargetAnimationsForCharacterModel', () => {
  it('shares an in-flight clip conversion across identical skeletons', async () => {
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    const original = retargeting.retargetClipAsync
    const convert = vi
      .spyOn(retargeting, 'retargetClipAsync')
      .mockImplementation(async (...args) => {
        await gate
        return original(...args)
      })
    try {
      const source = packLikeSource()
      const clips = [packClip('concurrent-retarget')]
      const requests = [1, 2].map(() =>
        retargetAnimationsForCharacterModel(makeRig(1), source, clips)
      )
      await yieldTask()
      expect(convert).toHaveBeenCalledOnce()
      release()
      const [first, second] = await Promise.all(requests)
      expect(first[0]).toBe(second[0])
    } finally {
      release()
      convert.mockRestore()
    }
  })
  it.each([0, Math.PI / 2])(
    'keeps a mounted character at its world position with rotation %s',
    async (rotation) => {
      const target = makeRig(1, 1.1)
      const hips = target.getObjectByName('Hips')!
      // GLB bones can be siblings of the skinned mesh.
      target.add(hips)
      const parent = new THREE.Group()
      parent.position.set(1200, 50, 2300)
      parent.rotation.y = rotation
      parent.add(target)
      parent.updateMatrixWorld(true)
      const before = hips.getWorldPosition(new THREE.Vector3())
      const source = makeRig(1)
      const [clip] = await retargetAnimationsForCharacterModel(target, source, [
        new THREE.AnimationClip(`ride-world-${rotation}`, 1, [
          new THREE.VectorKeyframeTrack(
            'Hips.position',
            [0, 1],
            [0, 0, 0, 0, 0, 0]
          ),
        ]),
      ])
      expect(hips.getWorldPosition(new THREE.Vector3())).toEqual(before)
      const mixer = new THREE.AnimationMixer(target)
      mixer.clipAction(clip).play()
      mixer.update(0.1)
      parent.updateMatrixWorld(true)
      const mountedPosition = hips.getWorldPosition(new THREE.Vector3())
      expect(mountedPosition.x).toBeCloseTo(1200, 5)
      expect(mountedPosition.y).toBeCloseTo(50.1, 5)
      expect(mountedPosition.z).toBeCloseTo(2300, 5)
    }
  )

  it('drops the source rig bone positions that stretch the limbs', async () => {
    const [clip] = await retargetAnimationsForCharacterModel(
      makeRig(1),
      packLikeSource(),
      [packClip('slash1')]
    )

    const positionTracks = clip.tracks
      .filter((track) => track.name.endsWith('.position'))
      .map((track) => track.name)
    expect(positionTracks).toEqual(['Hips.position'])
    expect(clip.tracks.length).toBeGreaterThan(1)
  })

  it('keeps each target model on its own retarget', async () => {
    const source = packLikeSource()
    const [tall] = await retargetAnimationsForCharacterModel(
      makeRig(1, 1.2),
      source,
      [packClip('slash2')]
    )
    const [short] = await retargetAnimationsForCharacterModel(
      makeRig(1, 0.8),
      source,
      [packClip('slash2')]
    )

    const hipY = (clip: THREE.AnimationClip) =>
      clip.tracks.find((track) => track.name === 'Hips.position')?.values[1]
    expect(hipY(tall)).not.toBeCloseTo(hipY(short) as number, 3)
  })
})

describe('groundRetargetedClips', () => {
  function makeFootRig() {
    const root = new THREE.Group()
    const hips = new THREE.Bone()
    hips.name = 'Hips'
    hips.position.y = 1
    root.add(hips)
    const feet = ['LeftFoot', 'RightFoot'].map((name) => {
      const foot = new THREE.Bone()
      foot.name = name
      foot.position.y = -1
      hips.add(foot)
      return foot
    })
    const geometry = new THREE.BufferGeometry()
    geometry.setAttribute(
      'position',
      new THREE.Float32BufferAttribute([-0.1, 0, 0, 0.1, 0, 0], 3)
    )
    geometry.setAttribute(
      'skinIndex',
      new THREE.Uint16BufferAttribute([0, 0, 0, 0, 1, 0, 0, 0], 4)
    )
    geometry.setAttribute(
      'skinWeight',
      new THREE.Float32BufferAttribute([1, 0, 0, 0, 1, 0, 0, 0], 4)
    )
    const mesh = new THREE.SkinnedMesh(geometry, new THREE.MeshBasicMaterial())
    root.add(mesh)
    root.updateMatrixWorld(true)
    mesh.bind(new THREE.Skeleton(feet))
    return { root, mesh }
  }

  it('aligns a floating jump baseline without changing its trajectory or source', async () => {
    const { root, mesh } = makeFootRig()
    const clip = new THREE.AnimationClip('jump', 2, [
      new THREE.VectorKeyframeTrack(
        'Hips.position',
        [0, 1, 2],
        [0, 1.45, 0, 0.2, 2.45, 0, 0.4, 1.45, 0]
      ),
      new THREE.QuaternionKeyframeTrack(
        'LeftFoot.quaternion',
        [0, 2],
        [0, 0, 0, 1, 0, 0, 0, 1]
      ),
    ])
    const original = clip.tracks[0].values.slice()
    const [grounded] = await groundRetargetedClips(root, [clip], {
      baselineClips: ['jump'],
    })
    expect(clip.tracks[0].values).toEqual(original)
    const track = grounded.tracks[0]
    expect(track.times).toEqual(clip.tracks[0].times)
    for (let i = 0; i < track.values.length; i++) {
      expect(track.values[i] - original[i]).toBeCloseTo(
        i % 3 === 1 ? -0.45 : 0,
        5
      )
    }
    expect(grounded.tracks[1].values).toEqual(clip.tracks[1].values)
    root.position.y += computeSoleGroundOffset(root)
    const mixer = new THREE.AnimationMixer(root)
    const action = mixer.clipAction(grounded).setLoop(THREE.LoopOnce, 1)
    action.clampWhenFinished = true
    action.play()
    for (const [time, expected] of [
      [0, 0.005],
      [1, 1.005],
      [2, 0.005],
    ]) {
      mixer.setTime(time)
      root.updateMatrixWorld(true)
      expect(
        mesh
          .getVertexPosition(0, new THREE.Vector3())
          .applyMatrix4(mesh.matrixWorld).y
      ).toBeCloseTo(expected, 5)
    }
  })

  it('plants a floating walk by its soles, keeping swing feet and other clips intact', async () => {
    const { root, mesh } = makeFootRig()
    const clip = new THREE.AnimationClip('walk', 1, [
      new THREE.VectorKeyframeTrack(
        'Hips.position',
        [0, 0.5, 1],
        [0, 1.1, 0, 0, 1.12, 0, 0, 1.1, 0]
      ),
      new THREE.VectorKeyframeTrack(
        'LeftFoot.position',
        [0, 0.5, 1],
        [0, -1, 0, 0, -0.92, 0, 0, -1, 0]
      ),
      new THREE.VectorKeyframeTrack(
        'RightFoot.position',
        [0, 0.5, 1],
        [0, -0.92, 0, 0, -1, 0, 0, -0.92, 0]
      ),
    ])
    const raw = clip.tracks[0].values.slice()
    const [grounded] = await groundRetargetedClips(root, [clip], {
      plantedClips: ['walk'],
    })
    expect(clip.tracks[0].values).toEqual(raw)
    expect(grounded.tracks[1].values).toEqual(clip.tracks[1].values)
    const jump = clip.clone()
    jump.name = 'jump'
    const [untouched] = await groundRetargetedClips(root, [jump], {
      plantedClips: ['walk'],
    })
    expect(untouched.tracks[0].values).toEqual(raw)
    root.position.y += computeSoleGroundOffset(root)
    const mixer = new THREE.AnimationMixer(root)
    mixer.clipAction(grounded).play()
    for (const time of [0, 0.125, 0.25, 0.5, 0.75]) {
      mixer.setTime(time)
      root.updateMatrixWorld(true)
      const heights = [0, 1].map(
        (i) =>
          mesh
            .getVertexPosition(i, new THREE.Vector3())
            .applyMatrix4(mesh.matrixWorld).y
      )
      expect(Math.min(...heights)).toBeCloseTo(0.005, 4)
      if (time === 0 || time === 0.5)
        expect(Math.max(...heights)).toBeCloseTo(0.085, 4)
    }
  })

  const sunkClip = () =>
    new THREE.AnimationClip('dying', 1, [
      new THREE.VectorKeyframeTrack(
        'Hips.position',
        [0, 1],
        [0, -0.5, 0, 0, -0.4, 0]
      ),
    ])

  it('lifts a clip that plays below the floor', async () => {
    const rig = makeRig(1)
    const [grounded] = await groundRetargetedClips(rig, [sunkClip()])
    const hips = grounded.tracks.find((t) => t.name === 'Hips.position')!
    const raw = sunkClip().tracks[0].values

    const lift = hips.values[1] - raw[1]
    expect(lift).toBeGreaterThan(0)
    // A constant shift: the motion inside the clip is untouched.
    expect(hips.values[4] - raw[4]).toBeCloseTo(lift, 5)
  })

  it('leaves an already grounded clip alone', async () => {
    const rig = makeRig(1)
    const [once] = await groundRetargetedClips(rig, [sunkClip()])
    const [twice] = await groundRetargetedClips(rig, [once])
    const hipsOnce = once.tracks.find((t) => t.name === 'Hips.position')!
    const hipsTwice = twice.tracks.find((t) => t.name === 'Hips.position')!
    expect(hipsTwice.values[1]).toBeCloseTo(hipsOnce.values[1], 5)
  })
})

describe('groundRetargetedClips rest clip', () => {
  const fallingClip = () =>
    new THREE.AnimationClip('dying', 1, [
      new THREE.VectorKeyframeTrack(
        'Hips.position',
        [0, 0.5, 1],
        [0, 1, 0, 0, 0.2, 0, 0, -0.4, 0]
      ),
    ])

  async function lowestAtEnd(clip: THREE.AnimationClip, rig: THREE.Group) {
    const mesh = rig.children[0] as THREE.SkinnedMesh
    const mixer = new THREE.AnimationMixer(rig)
    mixer.clipAction(clip).play()
    mixer.setTime(clip.duration - 1e-4)
    rig.updateMatrixWorld(true)
    const v = new THREE.Vector3()
    let lowest = Infinity
    const position = mesh.geometry.getAttribute('position')
    for (let i = 0; i < position.count; i++) {
      v.fromBufferAttribute(position, i)
      mesh.applyBoneTransform(i, v)
      mesh.localToWorld(v)
      lowest = Math.min(lowest, v.y)
    }
    return lowest
  }

  it('lands the last pose on the floor', async () => {
    const rig = makeRig(1)
    const [grounded] = await groundRetargetedClips(rig, [fallingClip()], {
      restClip: 'dying',
    })
    expect(await lowestAtEnd(grounded, makeRig(1))).toBeCloseTo(0, 2)
  })

  it('sinks the last pose by the rest offset', async () => {
    const rig = makeRig(1)
    const [grounded] = await groundRetargetedClips(rig, [fallingClip()], {
      restClip: 'dying',
      restOffset: -0.05,
    })
    expect(await lowestAtEnd(grounded, makeRig(1))).toBeCloseTo(-0.05, 2)
  })
})
