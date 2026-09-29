import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import {
  applyModularFingerPose,
  bindModularPart,
  modularAnimationClips,
  parseModularHandProfile,
  modularSwordTracks,
  poseModularSword,
  showModularOutfit,
  skinnedParts,
  type ModularHandProfile,
} from './modularCharacter'

function rig(id = 'test-rig', height = 1) {
  const root = new THREE.Group()
  root.userData.rig_id = id
  const bone = new THREE.Bone()
  bone.name = 'Hips'
  bone.position.y = height
  root.add(bone)
  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute(
    'position',
    new THREE.Float32BufferAttribute([0, height, 0], 3)
  )
  geometry.setAttribute(
    'skinIndex',
    new THREE.Uint16BufferAttribute([0, 0, 0, 0], 4)
  )
  geometry.setAttribute(
    'skinWeight',
    new THREE.Float32BufferAttribute([1, 0, 0, 0], 4)
  )
  const mesh = new THREE.SkinnedMesh(geometry, new THREE.MeshBasicMaterial())
  root.add(mesh)
  root.updateMatrixWorld(true)
  mesh.bind(new THREE.Skeleton([bone]))
  return { root, bone, mesh }
}

const profile: ModularHandProfile = {
  rig_id: 'test-rig',
  iron_sword: { position: [0, 0.05, -0.03], quaternion: [0, 0, 0, 1] },
  finger_pose_sources: { walk: { RightHandPinky1: 'RightHandRing1' } },
}

describe('modular outfit coverage', () => {
  it('restores body and underclothes after changing equipment, including grouped primitives', () => {
    const region = (name: string) => {
      const mesh = new THREE.SkinnedMesh()
      mesh.userData.region = name
      return mesh
    }
    const torso = region('torso')
    const hands = region('hands')
    const feet = region('feet')
    const legs = new THREE.Group()
    legs.userData.region = 'legs'
    const legPrimitive = new THREE.SkinnedMesh()
    legs.add(legPrimitive)
    const body = [torso, hands, feet, legPrimitive]
    const shirt = [region('torso'), region('sleeves'), region('underarms')]
    const pants = [
      region('main'),
      region('cuffs'),
      region('waist'),
      region('tucked_cuffs'),
    ]
    const parts = new Map([
      ['top_linen', shirt],
      ['pants_cloth', pants],
      ['top_leather', [region('armor')]],
      ['gloves_leather', [region('gloves')]],
      ['boots_leather', [region('boots')]],
      ['hair_crop', [region('hair')]],
      ['hair_sidepart', [region('hair')]],
    ])
    showModularOutfit(body, parts, {
      hair: 'hair_sidepart',
      top: 'leather',
      gloves: true,
      boots: true,
    })
    expect(body.every((mesh) => !mesh.visible)).toBe(true)
    expect(shirt.map((mesh) => mesh.visible)).toEqual([false, true, true])
    expect(pants.map((mesh) => mesh.visible)).toEqual([
      true,
      false,
      false,
      true,
    ])
    expect(parts.get('hair_crop')![0].visible).toBe(false)
    expect(parts.get('hair_sidepart')![0].visible).toBe(true)
    showModularOutfit(body, parts, {
      hair: 'hair_crop',
      top: 'linen',
      gloves: false,
      boots: false,
    })
    expect(shirt.every((mesh) => mesh.visible)).toBe(true)
    expect([
      torso.visible,
      hands.visible,
      feet.visible,
      legPrimitive.visible,
    ]).toEqual([false, true, true, false])
    expect(pants.map((mesh) => mesh.visible)).toEqual([true, true, true, false])
    showModularOutfit(body, parts, {
      hair: 'none',
      top: 'none',
      gloves: false,
      boots: false,
    })
    expect([
      torso.visible,
      hands.visible,
      feet.visible,
      legPrimitive.visible,
    ]).toEqual([true, true, true, false])
    expect(pants.map((mesh) => mesh.visible)).toEqual([true, true, true, false])
    expect(
      [...parts]
        .filter(([id]) => id !== 'pants_cloth')
        .flatMap(([, meshes]) => meshes)
        .every((mesh) => !mesh.visible)
    ).toBe(true)
    for (const boots of [true, false, true]) {
      showModularOutfit(body, parts, {
        hair: 'none',
        top: 'none',
        gloves: false,
        boots,
      })
      expect(pants.map((mesh) => mesh.visible)).toEqual([
        true,
        !boots,
        true,
        boots,
      ])
    }
  })
})

describe('baked modular animations', () => {
  function pack() {
    const source = rig()
    source.root.userData.animation_stage = 'modular-baked-v1'
    source.root.userData.variant = 'corrected'
    return {
      scene: source.root,
      animations: [
        new THREE.AnimationClip('jump', 1, [
          new THREE.VectorKeyframeTrack(
            'Hips.position',
            [0, 0.5, 1],
            [0, 1, 0, 0, 2, 0, 0, 1, 0]
          ),
        ]),
      ],
    }
  }

  it('plays baked values directly without changing their height or timing', () => {
    const body = rig()
    const source = pack()
    const [clip] = modularAnimationClips(body.root, source, 'corrected')
    const mixer = new THREE.AnimationMixer(body.root)
    mixer.clipAction(clip).play()
    mixer.update(0.5)
    expect(body.bone.position.y).toBe(2)
    expect(clip).toBe(source.animations[0])
    expect(clip.duration).toBe(1)
  })

  it('rejects a different rig, bind pose, processing stage or comparison variant', () => {
    const body = rig()
    expect(() =>
      modularAnimationClips(rig('other').root, pack(), 'corrected')
    ).toThrow('리그')
    expect(() =>
      modularAnimationClips(rig('test-rig', 2).root, pack(), 'corrected')
    ).toThrow('기준 골격')
    expect(() =>
      modularAnimationClips(body.root, pack(), 'comparison')
    ).toThrow('보정 단계')
    const source = pack()
    delete source.scene.userData.animation_stage
    expect(() => modularAnimationClips(body.root, source, 'corrected')).toThrow(
      '보정 단계'
    )
  })

  it('rejects weapon tracks and duplicate clips in a body-only pack', () => {
    const body = rig()
    const source = pack()
    source.animations[0].tracks[0].name = 'sword.position'
    expect(() => modularAnimationClips(body.root, source, 'corrected')).toThrow(
      '본 트랙'
    )
    const duplicate = pack()
    duplicate.animations.push(duplicate.animations[0].clone())
    expect(() =>
      modularAnimationClips(body.root, duplicate, 'corrected')
    ).toThrow('클립')
  })
})

describe('modular parts', () => {
  it('shares one skeleton across separate body material primitives', () => {
    const body = rig()
    const second = new THREE.SkinnedMesh(body.mesh.geometry, body.mesh.material)
    body.root.add(second)
    second.bind(
      new THREE.Skeleton([body.bone], body.mesh.skeleton.boneInverses)
    )
    bindModularPart(body.root, rig().root)
    expect(
      new Set(skinnedParts(body.root).map((mesh) => mesh.skeleton)).size
    ).toBe(1)
  })
  it('joins a moving character without duplicating bones or changing the source', () => {
    const body = rig()
    const part = rig()
    body.root.position.x = 3
    body.bone.position.y = 1.6
    const mixer = new THREE.AnimationMixer(body.root)
    const action = mixer
      .clipAction(
        new THREE.AnimationClip('walk', 2, [
          new THREE.VectorKeyframeTrack(
            'Hips.position',
            [0, 2],
            [0, 1, 0, 0, 2, 0]
          ),
        ])
      )
      .play()
    mixer.update(0.7)
    const position = body.bone.position.clone()
    const [attached] = bindModularPart(body.root, part.root)
    body.root.updateMatrixWorld(true)
    expect(attached.skeleton).toBe(body.mesh.skeleton)
    expect(skinnedParts(body.root)).toHaveLength(2)
    let bones = 0
    body.root.traverse((node) => {
      if (node instanceof THREE.Bone) bones++
    })
    expect(bones).toBe(1)
    expect(action.time).toBe(0.7)
    expect(body.bone.position).toEqual(position)
    const actual = attached
      .getVertexPosition(0, new THREE.Vector3())
      .applyMatrix4(attached.matrixWorld)
    const expected = body.mesh
      .getVertexPosition(0, new THREE.Vector3())
      .applyMatrix4(body.mesh.matrixWorld)
    expect(actual.distanceTo(expected)).toBeLessThan(1e-6)
    expect(part.mesh.parent).toBe(part.root)
    expect(part.bone.position.y).toBe(1)
    mixer.update(0.3)
    expect(action.time).toBe(1)
    expect(body.bone.position.y).toBe(1.5)
  })

  it('rejects mismatched rigs and bind matrices before altering the body', () => {
    const body = rig()
    expect(() => bindModularPart(body.root, rig('other').root)).toThrow(
      '리그 ID'
    )
    expect(() => bindModularPart(body.root, rig('test-rig', 2).root)).toThrow(
      '기준 행렬'
    )
    expect(skinnedParts(body.root)).toHaveLength(1)
  })
})

describe('modular hand profile', () => {
  it('animates the raised-hand attachment and returns to the carry before the loop seam', () => {
    const inward = new THREE.Quaternion().setFromAxisAngle(
      new THREE.Vector3(0, 0, 1),
      Math.PI / 18
    )
    const animated = parseModularHandProfile({
      ...profile,
      iron_sword_by_clip: {
        walk: {
          ...profile.iron_sword,
          keyframes: [
            { phase: 0, ...profile.iron_sword },
            {
              phase: 0.2,
              position: [0.01, 0.05, -0.03],
              quaternion: inward.toArray(),
            },
            { phase: 0.6, ...profile.iron_sword },
            { phase: 1, ...profile.iron_sword },
          ],
        },
      },
    })
    const source = new THREE.AnimationClip('walk', 2, [])
    const posed = source.clone()
    posed.tracks.push(
      ...modularSwordTracks(animated, 'test-rig', source, 'sword')
    )
    expect(source.tracks).toHaveLength(0)
    const root = new THREE.Group(),
      sword = new THREE.Object3D(),
      expected = new THREE.Object3D()
    sword.name = 'sword'
    root.add(sword)
    const mixer = new THREE.AnimationMixer(root)
    mixer.clipAction(posed).play()
    for (const phase of [0, 0.1, 0.2, 0.45, 0.6, 0.9, 0.999]) {
      mixer.setTime(phase * source.duration)
      poseModularSword(expected, animated, 'test-rig', 'walk', phase)
      expect(sword.position.distanceTo(expected.position)).toBeLessThan(1e-7)
      expect(
        sword.quaternion.clone().normalize().angleTo(expected.quaternion)
      ).toBeLessThan(1e-6)
    }
    poseModularSword(sword, animated, 'test-rig', 'walk', 1)
    expect(sword.position.toArray()).toEqual(profile.iron_sword.position)
    expect(sword.quaternion.toArray()).toEqual(profile.iron_sword.quaternion)
    poseModularSword(sword, animated, 'test-rig', 'combat_idle', 0.2)
    expect(sword.quaternion.toArray()).toEqual(profile.iron_sword.quaternion)
  })

  it('rejects unordered or incomplete attachment keyframes', () => {
    for (const phases of [
      [0, 0.2, 0.2, 1],
      [0.1, 1],
      [0, 0.9],
      [0, NaN, 1],
    ]) {
      expect(() =>
        parseModularHandProfile({
          ...profile,
          iron_sword_by_clip: {
            walk: {
              ...profile.iron_sword,
              keyframes: phases.map((phase) => ({
                phase,
                ...profile.iron_sword,
              })),
            },
          },
        })
      ).toThrow('키프레임')
    }
  })

  it('selects a walking attachment and falls back to the combat grip', () => {
    const walking = {
      ...profile,
      iron_sword_by_clip: {
        walk: { position: [0.01, 0.02, 0.03], quaternion: [0, 0, 1, 0] },
      },
    }
    const parsed = parseModularHandProfile(walking)
    const prop = new THREE.Group()
    poseModularSword(prop, parsed, 'test-rig', 'walk')
    expect(prop.position.toArray()).toEqual(
      walking.iron_sword_by_clip.walk.position
    )
    expect(prop.quaternion.toArray()).toEqual(
      walking.iron_sword_by_clip.walk.quaternion
    )
    poseModularSword(prop, parsed, 'test-rig', 'combat_idle')
    expect(prop.position.toArray()).toEqual(profile.iron_sword.position)
    expect(prop.quaternion.toArray()).toEqual(profile.iron_sword.quaternion)
    expect(() =>
      parseModularHandProfile({
        ...walking,
        iron_sword_by_clip: {
          walk: { position: [0, 0, 0], quaternion: [0, 0, 0, 0] },
        },
      })
    ).toThrow('동작별')
  })

  it('opens selected joints after copying, preserving the wrist and source clips', () => {
    const curled = new THREE.Quaternion().setFromAxisAngle(
      new THREE.Vector3(1, 0, 0),
      1.2
    )
    const makeTrack = (bone: string) =>
      new THREE.QuaternionKeyframeTrack(
        `${bone}.quaternion`,
        [0, 1],
        [...curled.toArray(), ...curled.toArray()]
      )
    const clip = new THREE.AnimationClip('combat_idle', 1, [
      makeTrack('RightHand'),
      makeTrack('RightHandRing2'),
      makeTrack('RightHandIndex2'),
    ])
    const customized: ModularHandProfile = {
      ...profile,
      finger_pose_sources: {
        combat_idle: { RightHandPinky2: 'RightHandRing2' },
      },
      finger_relaxation: {
        combat_idle: {
          RightHandIndex2: { rest_quaternion: [0, 0, 0, 1], amount: 0.25 },
          RightHandPinky2: { rest_quaternion: [0, 0, 0, 1], amount: 0.18 },
        },
      },
    }
    const adjusted = applyModularFingerPose(
      clip,
      parseModularHandProfile(customized),
      'test-rig'
    )
    for (const [bone, expected] of [
      ['RightHandIndex2', 0.9],
      ['RightHandPinky2', 0.984],
    ] as const) {
      const track = adjusted.tracks.find(
        (track) => track.name === `${bone}.quaternion`
      )!
      const rotation = new THREE.Quaternion().fromArray(track.values)
      expect(rotation.angleTo(new THREE.Quaternion())).toBeCloseTo(expected, 5)
      expect(rotation.length()).toBeCloseTo(1, 6)
    }
    expect(
      adjusted.tracks.find((track) => track.name === 'RightHand.quaternion')!
        .values
    ).toEqual(clip.tracks[0].values)
    expect(clip.tracks).toHaveLength(3)
    expect(
      new THREE.Quaternion()
        .fromArray(clip.tracks[2].values)
        .angleTo(new THREE.Quaternion())
    ).toBeCloseTo(1.2, 5)
    const noCopy = {
      ...customized,
      finger_pose_sources: {},
      finger_relaxation: {
        combat_idle: {
          RightHandIndex2:
            customized.finger_relaxation!.combat_idle.RightHandIndex2,
        },
      },
    }
    expect(applyModularFingerPose(clip, noCopy, 'test-rig')).not.toBe(clip)
  })

  it('rejects relaxation outside finger joints and invalid interpolation amounts', () => {
    for (const [bone, amount] of [
      ['RightHand', 0.2],
      ['RightHandIndex2', NaN],
      ['RightHandIndex2', 1.1],
    ] as const) {
      expect(() =>
        parseModularHandProfile({
          ...profile,
          finger_relaxation: {
            walk: { [bone]: { rest_quaternion: [0, 0, 0, 1], amount } },
          },
        })
      ).toThrow('펴기')
    }
  })

  it('copies finger motion without mutating shared clips or keyframe buffers', () => {
    const ring = new THREE.QuaternionKeyframeTrack(
      'RightHandRing1.quaternion',
      [0, 1],
      [0, 0, 0, 1, 0.6, 0, 0, 0.8]
    )
    const pinky = new THREE.QuaternionKeyframeTrack(
      'RightHandPinky1.quaternion',
      [0, 1],
      [0, 0, 0, 1, 0, 0, 0, 1]
    )
    const clip = new THREE.AnimationClip('walk', 1, [ring, pinky])
    const corrected = applyModularFingerPose(clip, profile, 'test-rig')
    const track = corrected.tracks.find((track) => track.name === pinky.name)!
    expect(track.values).toEqual(ring.values)
    expect(track.values).not.toBe(ring.values)
    expect(track.times).not.toBe(ring.times)
    track.values[4] = 0.2
    expect(pinky.values[4]).toBe(0)
    expect(ring.values[4]).toBeCloseTo(0.6)
    expect(clip.tracks).toEqual([ring, pinky])
    expect(() => applyModularFingerPose(clip, profile, 'other')).toThrow('리그')
  })

  it('rejects missing source tracks and malformed transforms', () => {
    expect(() =>
      applyModularFingerPose(
        new THREE.AnimationClip('walk', 1, []),
        profile,
        'test-rig'
      )
    ).toThrow('트랙')
    expect(parseModularHandProfile(profile)).toBe(profile)
    expect(() =>
      parseModularHandProfile({
        ...profile,
        iron_sword: { position: [NaN, 0, 0], quaternion: [0, 0, 0, 1] },
      })
    ).toThrow('형식')
    expect(() =>
      parseModularHandProfile({
        ...profile,
        iron_sword: { position: [0, 0, 0], quaternion: [0, 0, 0, 0] },
      })
    ).toThrow('정규화')
    const prop = new THREE.Group()
    poseModularSword(prop, profile, 'test-rig')
    expect(prop.position.toArray()).toEqual(profile.iron_sword.position)
    expect(() => poseModularSword(prop, profile, 'other')).toThrow('리그')
  })
})
