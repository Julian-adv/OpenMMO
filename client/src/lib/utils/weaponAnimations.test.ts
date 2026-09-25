import { describe, expect, it, vi } from 'vitest'
import { AnimationClip, Object3D, VectorKeyframeTrack } from 'three'
import { loadWeaponAnimations } from './weaponAnimations'
import {
  getWeaponAnimation,
  weaponAnimationFor,
  weaponAnimationClipName,
} from '../data/weaponAnimationDefs'
import itemDefs from '../data/itemDefs'

const sourceSlash = new AnimationClip('great_sword_slash', 1.25, [
  new VectorKeyframeTrack(
    'Hips.position',
    [0, 0.625, 1.25],
    [0, 1, 0, 0, 1, 0, 0, 1, 0]
  ),
])

vi.mock('./gltfCache', () => ({
  loadGLB: vi.fn(async () => ({
    scene: new Object3D(),
    animations: [sourceSlash],
  })),
}))
vi.mock('./characterAnimationUtils', () => ({
  retargetAnimationsForCharacterModel: vi.fn(
    async (_target, _source, clips) => clips
  ),
  groundRetargetedClips: vi.fn(async (_target, clips) => clips),
}))

describe('weapon type animations', () => {
  const profile = weaponAnimationFor({ weaponType: 'great_sword' })!
  it('selects movement and attack from the type dataset', () => {
    expect(weaponAnimationClipName(profile, 'moving', 'walk')).toBe(
      'great_sword_walk'
    )
    expect(weaponAnimationClipName(profile, 'moving', 'run')).toBe(
      'great_sword_run'
    )
    expect(weaponAnimationClipName(profile, 'attack', 'walk')).toBe(
      'great_sword_slash'
    )
    expect(weaponAnimationClipName(profile, 'idle', 'walk')).toBe(
      'great_sword_idle'
    )
    expect(getWeaponAnimation('iron_sword')).toBeUndefined()
    expect(getWeaponAnimation('bow')).toBeUndefined()
    expect(weaponAnimationClipName(profile, 'dead', 'walk')).toBeUndefined()
    expect(weaponAnimationClipName(profile, 'interact', 'walk')).toBeUndefined()
  })

  it('gives a differently named item the same animations and grip through weaponType', () => {
    itemDefs.test_great_sword = {
      ...itemDefs.great_sword,
      id: 'test_great_sword',
    }
    try {
      expect(getWeaponAnimation('test_great_sword')).toBe(profile)
      expect(getWeaponAnimation('test_great_sword')?.gripRotationRadians).toBe(
        '-1.907|-0.475|0.139'
      )
    } finally {
      delete itemDefs.test_great_sword
    }
  })

  it('aligns the strike with the melee impact without modifying shared source tracks', async () => {
    const clips = await loadWeaponAnimations(
      'test-knight',
      new Object3D(),
      profile
    )
    const slash = clips.get('great_sword_slash')!
    expect(slash.tracks[0].times[1]).toBeCloseTo(0.54)
    expect(slash.duration).toBeCloseTo(1.08)
    expect(sourceSlash.tracks[0].times[1]).toBe(0.625)
    expect(sourceSlash.duration).toBe(1.25)
  })

  it('keeps profiles with different timings separate in the model cache', async () => {
    const clips = await loadWeaponAnimations('test-knight', new Object3D(), {
      ...profile,
      attackImpactMs: 1250,
    })
    expect(clips.get('great_sword_slash')!.duration).toBeCloseTo(0.54)
  })
})
