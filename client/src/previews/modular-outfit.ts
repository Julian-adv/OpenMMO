import type * as THREE from 'three'
import { trimModularClothing } from '../lib/utils/modularClothing'
import {
  showModularOutfit,
  type ModularOutfit,
} from '../lib/utils/modularCharacter'

export {
  ROGUE_MODULAR_OUTFIT as ROGUE_PREVIEW_OUTFIT,
  ROGUE_MODULAR_PARTS as ROGUE_PREVIEW_PARTS,
} from '../lib/utils/modularCharacter'

export type PreviewOutfit = Omit<
  ModularOutfit,
  'top' | 'pants' | 'gloves' | 'boots'
> & {
  top: ModularOutfit['top'] | 'caveman'
  pants: ModularOutfit['pants'] | 'caveman'
  gloves: ModularOutfit['gloves'] | 'caveman'
  boots: ModularOutfit['boots'] | 'caveman'
}

export const CAVEMAN_PREVIEW_OUTFIT: PreviewOutfit = {
  hair: 'hair_crop',
  top: 'caveman',
  pants: 'caveman',
  gloves: 'caveman',
  boots: 'caveman',
  helmet: 'none',
}

export function showPreviewOutfit(
  body: THREE.SkinnedMesh[],
  parts: ReadonlyMap<string, THREE.SkinnedMesh[]>,
  outfit: PreviewOutfit
): Set<string> {
  const cavemanBoots =
    outfit.boots === 'caveman' && !!parts.get('boots_caveman')?.length
  const cavemanGloves =
    outfit.gloves === 'caveman' && !!parts.get('gloves_caveman')?.length
  const selected = showModularOutfit(body, parts, {
    ...outfit,
    top: outfit.top === 'caveman' ? 'none' : outfit.top,
    pants:
      outfit.pants === 'caveman'
        ? parts.get('pants_caveman')?.length
          ? 'barbarian'
          : 'none'
        : outfit.pants,
    gloves:
      outfit.gloves === 'caveman'
        ? cavemanGloves
          ? 'barbarian'
          : 'none'
        : outfit.gloves,
    boots:
      outfit.boots === 'caveman'
        ? cavemanBoots
          ? 'leather'
          : 'none'
        : outfit.boots,
  })
  if (outfit.pants === 'caveman') {
    selected.delete('pants_barbarian')
    for (const mesh of parts.get('pants_barbarian') ?? []) mesh.visible = false
  }
  if (cavemanGloves) {
    selected.delete('gloves_barbarian')
    for (const mesh of parts.get('gloves_barbarian') ?? []) mesh.visible = false
    for (const mesh of body) {
      let node: THREE.Object3D | null = mesh
      while (node && typeof node.userData.region !== 'string')
        node = node.parent
      if (node?.userData.region === 'forearms')
        trimModularClothing(mesh, 'bracers')
    }
  }
  if (cavemanBoots) {
    selected.delete('boots_leather')
    for (const mesh of parts.get('boots_leather') ?? []) mesh.visible = false
    for (const mesh of parts.get('pants_plate') ?? [])
      if (mesh.visible) trimModularClothing(mesh, 'caveman_boots')
    for (const mesh of body) {
      let node: THREE.Object3D | null = mesh
      while (node && typeof node.userData.region !== 'string')
        node = node.parent
      const region = node?.userData.region
      if (['feet', 'ankles', 'boot_ankles'].includes(region))
        mesh.visible = false
      if (region === 'legs') trimModularClothing(mesh, 'caveman_boots')
    }
  }
  for (const slot of ['top', 'pants', 'gloves', 'boots'] as const) {
    const id = `${slot}_caveman`
    const meshes = parts.get(id) ?? []
    if (outfit[slot] === 'caveman' && meshes.length) {
      selected.add(id)
      for (const mesh of meshes) mesh.visible = true
    }
  }
  return selected
}
