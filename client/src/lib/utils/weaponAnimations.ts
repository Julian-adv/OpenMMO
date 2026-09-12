import type { Object3D, AnimationClip } from 'three'
import type { WeaponAnimationDefinition } from '../data/weaponAnimationDefs'
import { loadGLB } from './gltfCache'
import { PLAYER_ATTACK_IMPACT_DELAY_MS } from '../data/combatTiming'
import {
  groundRetargetedClips,
  retargetAnimationsForCharacterModel,
} from './characterAnimationUtils'

const packs = new Map<string, Promise<Map<string, AnimationClip>>>()

export function loadWeaponAnimations(
  modelPath: string,
  target: Object3D,
  profile: WeaponAnimationDefinition
) {
  const cacheKey = `${modelPath}:${JSON.stringify(profile)}`
  let pending = packs.get(cacheKey)
  if (!pending) {
    pending = loadGLB(`/models/${profile.pack}`)
      .then(async (pack) => {
        const clips = await retargetAnimationsForCharacterModel(
          target,
          pack.scene,
          pack.animations
        )
        return groundRetargetedClips(target, clips)
      })
      .then(
        (clips) =>
          new Map(
            clips.map((clip) => {
              const timed =
                clip.name === profile.attack &&
                (profile.attackImpactMs ?? 0) > 0
                  ? clip.clone()
                  : clip
              if (timed !== clip) {
                const scale =
                  PLAYER_ATTACK_IMPACT_DELAY_MS / profile.attackImpactMs!
                for (const track of timed.tracks) track.scale(scale)
                timed.resetDuration()
              }
              return [timed.name, timed]
            })
          )
      )
      .catch((error) => {
        packs.delete(cacheKey)
        throw error
      })
    packs.set(cacheKey, pending)
  }
  return pending
}
