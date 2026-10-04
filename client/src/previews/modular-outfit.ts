import type * as THREE from 'three'
import {
  showModularOutfit,
  type ModularOutfit,
} from '../lib/utils/modularCharacter'

export {
  ROGUE_MODULAR_OUTFIT as ROGUE_PREVIEW_OUTFIT,
  ROGUE_MODULAR_PARTS as ROGUE_PREVIEW_PARTS,
} from '../lib/utils/modularCharacter'

export type PreviewOutfit = Omit<ModularOutfit, 'top' | 'pants'> & {
  top: ModularOutfit['top'] | 'caveman'
  pants: ModularOutfit['pants'] | 'caveman'
}

export const CAVEMAN_PREVIEW_OUTFIT: PreviewOutfit = {
  hair: 'hair_crop',
  top: 'caveman',
  pants: 'caveman',
  gloves: 'none',
  boots: 'none',
  helmet: 'none',
}

export function showPreviewOutfit(
  body: THREE.SkinnedMesh[],
  parts: ReadonlyMap<string, THREE.SkinnedMesh[]>,
  outfit: PreviewOutfit
): Set<string> {
  const selected = showModularOutfit(body, parts, {
    ...outfit,
    top: outfit.top === 'caveman' ? 'none' : outfit.top,
    pants:
      outfit.pants === 'caveman'
        ? parts.get('pants_caveman')?.length
          ? 'barbarian'
          : 'none'
        : outfit.pants,
  })
  if (outfit.pants === 'caveman') {
    selected.delete('pants_barbarian')
    for (const mesh of parts.get('pants_barbarian') ?? []) mesh.visible = false
  }
  for (const slot of ['top', 'pants'] as const) {
    const id = `${slot}_caveman`
    const meshes = parts.get(id) ?? []
    if (outfit[slot] === 'caveman' && meshes.length) {
      selected.add(id)
      for (const mesh of meshes) mesh.visible = true
    }
  }
  return selected
}
