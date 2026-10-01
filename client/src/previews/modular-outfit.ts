import type * as THREE from 'three'
import { trimModularClothing } from '../lib/utils/modularClothing'
import {
  showModularOutfit,
  type ModularOutfit,
} from '../lib/utils/modularCharacter'

type RogueSlot = 'top' | 'pants' | 'gloves' | 'boots'
export type PreviewOutfit = Omit<ModularOutfit, RogueSlot> & {
  [Slot in RogueSlot]: ModularOutfit[Slot] | 'rogue'
}

export const ROGUE_PREVIEW_PARTS = [
  'top_rogue',
  'pants_rogue',
  'gloves_rogue',
  'boots_rogue',
] as const

export const ROGUE_PREVIEW_OUTFIT: PreviewOutfit = {
  hair: 'hair_crop',
  top: 'rogue',
  pants: 'rogue',
  gloves: 'rogue',
  boots: 'rogue',
  helmet: 'none',
}

export function showPreviewOutfit(
  body: THREE.SkinnedMesh[],
  parts: ReadonlyMap<string, THREE.SkinnedMesh[]>,
  outfit: PreviewOutfit
): Set<string> {
  const available = <Slot extends RogueSlot>(slot: Slot) =>
    outfit[slot] === 'rogue' && !parts.get(`${slot}_rogue`)?.length
      ? 'none'
      : outfit[slot]
  const top = available('top')
  const pants = available('pants')
  const gloves = available('gloves')
  const boots = available('boots')
  const collar = parts
    .get('top_rogue')
    ?.some((mesh) => mesh.userData.fitting_status === 'candidate_tripo_v1')
    ? 'tripo_collar'
    : 'collar'
  const selected = showModularOutfit(body, parts, {
    ...outfit,
    top: top === 'rogue' ? 'none' : top,
    pants: pants === 'rogue' ? 'cloth' : pants,
    gloves: gloves === 'rogue' ? 'none' : gloves,
    boots: boots === 'rogue' ? 'leather' : boots,
  })
  for (const [style, proxy] of [
    [pants, 'pants_cloth'],
    [boots, 'boots_leather'],
  ]) {
    if (style !== 'rogue') continue
    selected.delete(proxy)
    for (const mesh of parts.get(proxy) ?? []) mesh.visible = false
  }
  for (const [slot, style] of Object.entries({ top, pants, gloves, boots })) {
    if (style === 'rogue') selected.add(`${slot}_rogue`)
  }
  for (const id of ROGUE_PREVIEW_PARTS)
    for (const mesh of parts.get(id) ?? []) mesh.visible = selected.has(id)
  const hidden = new Set([
    ...(top === 'rogue' ? ['torso', 'upper_arms'] : []),
    ...(pants === 'rogue' ? ['legs', 'ankles', 'boot_ankles'] : []),
  ])
  for (const mesh of body) {
    for (let node: THREE.Object3D | null = mesh; node; node = node.parent)
      if (typeof node.userData.region === 'string') {
        if (node.userData.region === 'neck')
          trimModularClothing(mesh, top === 'rogue' ? collar : undefined, true)
        if (hidden.has(node.userData.region)) mesh.visible = false
        if (
          node.userData.region === 'torso' &&
          top === 'rogue' &&
          collar === 'tripo_collar'
        ) {
          trimModularClothing(mesh, 'tripo_waist', true)
          mesh.visible = true
        }
        break
      }
  }
  return selected
}
