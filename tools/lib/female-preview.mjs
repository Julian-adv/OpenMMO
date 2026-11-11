import { stat } from 'node:fs/promises'

export const root = new URL('../../', import.meta.url)
export const directory = new URL('assets/modular_human_female_01/preview/', root)
export const sources = {
  base: 'assets/modular_human_female_01/rig_v1/female-base.glb',
  bob: 'assets/modular_human_female_01/hair/tripo_knight_bob_v1/fitted_v1/hair_female_knight_bob.glb',
  warrior: 'assets/modular_human_female_01/hair/tripo_long_side_v3/fitted_v1/hair_female_long_side.glb',
  long_bangs: 'assets/modular_human_female_01/hair/tripo_long_bangs_v2/fitted_v1/hair_female_long_bangs.glb',
  valkyrie: 'assets/items/valkyrie_boots/fitted_v1/boots_female_valkyrie.glb',
  animations: 'assets/modular_human_female_01/rig_v1/female-base.glb',
}

export function dependencies(name) {
  return [sources[name], 'tools/build-female-preview.mjs', 'tools/lib/female-preview.mjs',
    ...(name === 'animations' ? [
      'client/public/models/characters/modular_male/animations/offhand.glb',
      'client/src/previews/female-torch.ts',
      'client/src/lib/utils/characterAnimationUtils.ts',
      'client/src/lib/utils/modularCharacter.ts',
      'client/src/lib/utils/frameYield.ts',
      'tools/lib/headless-three.mjs',
    ] : []),
  ]
}

export async function signature(name) {
  return JSON.stringify(await Promise.all(dependencies(name).map(async path => {
    const info = await stat(new URL(path, root))
    return [path, info.size, info.mtimeMs]
  })))
}

export const output = name => new URL(`${name}.${name === 'animations' ? 'json' : 'glb'}`, directory)
