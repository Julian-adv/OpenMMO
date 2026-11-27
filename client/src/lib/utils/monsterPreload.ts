import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { getMonsterDef, splitClipNames } from '../data/monsterDefs'
import type { MonsterDefinition } from '../data/monsterDefs'
import { loadSharedPackClipsForModel } from './characterAnimationUtils'

/** Every clip a monster type plays: its def's anim* fields. */
export function monsterClipNames(def: MonsterDefinition | undefined): string[] {
  return Object.entries(def ?? {})
    .filter(([key, value]) => key.startsWith('anim') && !!value)
    .flatMap(([, value]) => splitClipNames(value as string))
}

/** Grounding for a shared-pack monster's retargeted clips, as its type sets it. */
export function sharedPackGrounding(def: MonsterDefinition) {
  return {
    restClip: def.animDie,
    restOffset: (def.corpseGroundOffset ?? 0) / (def.scale ?? 1),
  }
}

/** Load the models of `types` with `load` (Monster.svelte's loader, so its
 * cache is the one warmed) and retarget their borrowed clips now, so the
 * first of them to appear already moves. */
export function preloadMonsterTypes(
  types: string[],
  load: (url: string) => PromiseLike<GLTF>
) {
  for (const type of new Set(types)) {
    const def = getMonsterDef(type)
    if (!def) continue
    void Promise.resolve(load(`/models/${def.model}`))
      .then((gltf) =>
        def.sharedAnims
          ? loadSharedPackClipsForModel(
              def.model,
              gltf.scene,
              monsterClipNames(def),
              sharedPackGrounding(def)
            )
          : undefined
      )
      .catch((error) => console.warn(`Preload of ${type} failed`, error))
  }
}
