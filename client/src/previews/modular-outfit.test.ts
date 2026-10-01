import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import {
  KNIGHT_MODULAR_OUTFIT,
  type ModularOutfit,
} from '../lib/utils/modularCharacter'
import {
  ROGUE_PREVIEW_OUTFIT,
  ROGUE_PREVIEW_PARTS,
  showPreviewOutfit,
} from './modular-outfit'

function fixture() {
  const mesh = (region: string) => {
    const result = new THREE.SkinnedMesh()
    result.userData.region = region
    return result
  }
  const regions = [
    'head',
    'neck',
    'torso',
    'upper_arms',
    'forearms',
    'hands',
    'legs',
    'ankles',
    'boot_ankles',
    'feet',
  ]
  const body = regions.map(mesh)
  const parts = new Map([
    ...ROGUE_PREVIEW_PARTS.map((id) => [id, [mesh(id)]] as const),
    ['pants_cloth', [mesh('main'), mesh('cuffs'), mesh('tucked_cuffs')]],
    ['boots_leather', [mesh('boots_leather')]],
  ] as [string, THREE.SkinnedMesh[]][])
  const visible = (region: string) => body[regions.indexOf(region)].visible
  return { body, parts, visible }
}

const undressed: ModularOutfit = {
  hair: 'none',
  top: 'none',
  pants: 'none',
  gloves: 'none',
  boots: 'none',
  helmet: 'none',
}

describe('rogue workshop preview', () => {
  it('shows four candidate slots and keeps bare forearms and fingers visible', () => {
    const { body, parts, visible } = fixture()
    const selected = showPreviewOutfit(body, parts, ROGUE_PREVIEW_OUTFIT)
    for (const id of ROGUE_PREVIEW_PARTS) {
      expect(selected.has(id)).toBe(true)
      expect(parts.get(id)![0].visible).toBe(true)
    }
    for (const region of ['forearms', 'hands', 'head'])
      expect(visible(region)).toBe(true)
    for (const region of [
      'torso',
      'neck',
      'upper_arms',
      'legs',
      'ankles',
      'feet',
      'boot_ankles',
    ])
      expect(visible(region)).toBe(false)
    for (const id of ['pants_cloth', 'boots_leather']) {
      expect(selected.has(id)).toBe(false)
      expect(parts.get(id)!.every((mesh) => !mesh.visible)).toBe(true)
    }
    showPreviewOutfit(body, parts, KNIGHT_MODULAR_OUTFIT)
    expect(visible('hands')).toBe(false)
    showPreviewOutfit(body, parts, undressed)
    for (const mesh of body)
      expect(mesh.visible).toBe(mesh.userData.region !== 'boot_ankles')
    for (const id of ROGUE_PREVIEW_PARTS)
      expect(parts.get(id)![0].visible).toBe(false)
  })

  it('keeps skin when candidate files are unavailable', () => {
    const { body, parts, visible } = fixture()
    for (const id of ROGUE_PREVIEW_PARTS) parts.delete(id)
    const selected = showPreviewOutfit(body, parts, ROGUE_PREVIEW_OUTFIT)
    for (const id of ROGUE_PREVIEW_PARTS) expect(selected.has(id)).toBe(false)
    for (const region of [
      'torso',
      'neck',
      'upper_arms',
      'hands',
      'legs',
      'ankles',
      'feet',
    ])
      expect(visible(region)).toBe(true)
  })

  it('tucks existing cloth cuffs into rogue boots and restores them on removal', () => {
    const { body, parts, visible } = fixture()
    showPreviewOutfit(body, parts, {
      ...undressed,
      pants: 'cloth',
      boots: 'rogue',
    })
    const [, cuffs, tucked] = parts.get('pants_cloth')!
    expect(cuffs.visible).toBe(false)
    expect(tucked.visible).toBe(true)
    showPreviewOutfit(body, parts, { ...undressed, pants: 'cloth' })
    expect(cuffs.visible).toBe(true)
    expect(tucked.visible).toBe(false)
    expect(visible('feet')).toBe(true)
    showPreviewOutfit(body, parts, {
      ...undressed,
      pants: 'rogue',
      boots: 'barbarian',
    })
    expect(visible('legs')).toBe(false)
  })
})
