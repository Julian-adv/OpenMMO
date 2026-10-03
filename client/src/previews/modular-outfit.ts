import type * as THREE from 'three'
import {
  showModularOutfit,
  type ModularOutfit,
} from '../lib/utils/modularCharacter'

export {
  ROGUE_MODULAR_OUTFIT as ROGUE_PREVIEW_OUTFIT,
  ROGUE_MODULAR_PARTS as ROGUE_PREVIEW_PARTS,
} from '../lib/utils/modularCharacter'

export type PreviewOutfit = Omit<ModularOutfit, 'top'> & {
  top: ModularOutfit['top'] | 'caveman'
}

export const CAVEMAN_PREVIEW_OUTFIT: PreviewOutfit = {
  hair: 'hair_crop',
  top: 'caveman',
  pants: 'none',
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
  })
  const meshes = parts.get('top_caveman') ?? []
  if (outfit.top === 'caveman' && meshes.length) {
    selected.add('top_caveman')
    for (const mesh of meshes) mesh.visible = true
  }
  return selected
}
