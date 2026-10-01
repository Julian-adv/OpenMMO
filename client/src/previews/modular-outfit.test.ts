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
    for (const region of ['forearms', 'hands', 'head', 'neck'])
      expect(visible(region)).toBe(true)
    for (const region of [
      'torso',
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

  it.each([
    ['candidate_v8', 1.61],
    ['candidate_tripo_v1', 1.54],
  ] as const)(
    'keeps the neckline of %s and restores skin when changing tops',
    (candidate, cutoff) => {
      const { body, parts, visible } = fixture()
      parts.get('top_rogue')![0].userData.fitting_status = candidate
      const neck = body.find((mesh) => mesh.userData.region === 'neck')!
      const source = new THREE.BufferGeometry()
      source.setAttribute(
        'position',
        new THREE.Float32BufferAttribute(
          [-0.08, 1.55, 0, 0.08, 1.55, 0, 0, 1.66, 0],
          3
        )
      )
      source.setIndex([0, 1, 2])
      neck.geometry = source
      showPreviewOutfit(body, parts, ROGUE_PREVIEW_OUTFIT)
      expect(visible('neck')).toBe(true)
      const position = neck.geometry.getAttribute('position')
      expect(position.count).toBeGreaterThan(0)
      for (let i = 0; i < position.count; i++)
        expect(position.getY(i)).toBeGreaterThanOrEqual(cutoff - 1e-6)
      if (candidate === 'candidate_tripo_v1')
        expect(
          Math.min(
            ...Array.from({ length: position.count }, (_, i) =>
              position.getY(i)
            )
          )
        ).toBeLessThan(1.61)
      expect(neck.geometry.index!.count).toBeGreaterThan(0)
      showPreviewOutfit(body, parts, KNIGHT_MODULAR_OUTFIT)
      expect(visible('neck')).toBe(false)
      expect(neck.geometry).toBe(source)
      showPreviewOutfit(body, parts, ROGUE_PREVIEW_OUTFIT)
      parts.delete('top_rogue')
      showPreviewOutfit(body, parts, ROGUE_PREVIEW_OUTFIT)
      expect(visible('neck')).toBe(true)
      expect(neck.geometry).toBe(source)
    }
  )

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

  it('keeps only the lower torso under the Tripo hem and restores it on removal', () => {
    const { body, parts, visible } = fixture()
    const top = parts.get('top_rogue')![0]
    top.userData.fitting_status = 'candidate_tripo_v1'
    const torso = body.find((mesh) => mesh.userData.region === 'torso')!
    const source = new THREE.BufferGeometry()
    source.setAttribute(
      'position',
      new THREE.Float32BufferAttribute(
        [-0.1, 1.08, 0, 0.1, 1.08, 0, 0, 1.4, 0],
        3
      )
    )
    source.setIndex([0, 1, 2])
    torso.geometry = source
    showPreviewOutfit(body, parts, ROGUE_PREVIEW_OUTFIT)
    expect(visible('torso')).toBe(true)
    expect(visible('upper_arms')).toBe(false)
    const position = torso.geometry.getAttribute('position')
    const heights = Array.from({ length: position.count }, (_, i) =>
      position.getY(i)
    )
    expect(Math.min(...heights)).toBeCloseTo(1.08)
    expect(Math.max(...heights)).toBeCloseTo(1.14)
    expect(torso.geometry.index!.count).toBeGreaterThan(0)
    top.userData.fitting_status = 'candidate_v8'
    showPreviewOutfit(body, parts, ROGUE_PREVIEW_OUTFIT)
    expect(visible('torso')).toBe(false)
    expect(torso.geometry).toBe(source)
    top.userData.fitting_status = 'candidate_tripo_v1'
    showPreviewOutfit(body, parts, ROGUE_PREVIEW_OUTFIT)
    parts.delete('top_rogue')
    showPreviewOutfit(body, parts, ROGUE_PREVIEW_OUTFIT)
    expect(visible('torso')).toBe(true)
    expect(torso.geometry).toBe(source)
  })
})
