import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import {
  CAVEMAN_MODULAR_OUTFIT,
  KNIGHT_MODULAR_OUTFIT,
  PRIEST_MODULAR_OUTFIT,
  RANGER_MODULAR_OUTFIT,
  ROGUE_MODULAR_OUTFIT,
  ROGUE_MODULAR_PARTS,
  showModularOutfit,
  type ModularOutfit,
} from '../lib/utils/modularCharacter'

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
    ...ROGUE_MODULAR_PARTS.map((id) => [id, [mesh(id)]] as const),
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

describe('linen shirt with underwear', () => {
  it('opens the hem, fills the waist gap and restores both when equipment changes', () => {
    const { body, parts, visible } = fixture()
    const shirtSource = new THREE.CylinderGeometry(0.1, 0.1, 0.2, 16, 4, true)
    shirtSource.translate(0, 1.16, -0.015)
    const shirt = new THREE.SkinnedMesh(shirtSource)
    shirt.userData.region = 'torso'
    const sleeves = new THREE.SkinnedMesh(shirtSource)
    sleeves.userData.region = 'sleeves'
    parts.set('top_linen', [shirt, sleeves])
    parts.set('top_priest', [new THREE.SkinnedMesh()])
    parts.set('pants_priest', [new THREE.SkinnedMesh()])
    const torso = body.find((mesh) => mesh.userData.region === 'torso')!
    const skinSource = new THREE.PlaneGeometry(0.3, 0.5)
    skinSource.translate(0, 1.305, 0)
    torso.geometry = skinSource
    const outfit = { ...undressed, top: 'linen' as const }

    showModularOutfit(body, parts, outfit)
    expect(visible('torso')).toBe(true)
    expect(visible('upper_arms')).toBe(false)
    expect(visible('legs')).toBe(true)
    torso.geometry.computeBoundingBox()
    expect(torso.geometry.boundingBox!.min.y).toBeCloseTo(1.055)
    expect(torso.geometry.boundingBox!.max.y).toBeCloseTo(1.09)
    const opened = shirt.geometry
    const position = opened.getAttribute('position')
    const sourcePosition = shirtSource.getAttribute('position')
    for (let i = 0; i < position.count; i++) {
      expect(position.getY(i)).toBe(sourcePosition.getY(i))
      if (position.getY(i) < 1.085)
        expect(
          Math.hypot(
            position.getX(i) / 0.185,
            (position.getZ(i) + 0.015) / 0.14
          )
        ).toBeCloseTo(1)
      if (position.getY(i) >= 1.2)
        expect([position.getX(i), position.getZ(i)]).toEqual([
          sourcePosition.getX(i),
          sourcePosition.getZ(i),
        ])
    }
    expect(opened.index!.array).toEqual(shirtSource.index!.array)
    expect(opened.attributes.uv.array).toEqual(shirtSource.attributes.uv.array)
    expect(sleeves.geometry).toBe(shirtSource)

    for (const pants of ['cloth', 'plate', 'priest'] as const) {
      showModularOutfit(body, parts, { ...outfit, pants })
      expect(shirt.geometry).toBe(shirtSource)
      expect(torso.geometry).toBe(skinSource)
      expect(visible('torso')).toBe(false)
      showModularOutfit(body, parts, outfit)
      expect(shirt.geometry).toBe(opened)
    }
    showModularOutfit(body, parts, { ...outfit, top: 'priest' })
    expect(visible('torso')).toBe(false)
    expect(shirt.geometry).toBe(shirtSource)
    showModularOutfit(body, parts, undressed)
    expect(visible('torso')).toBe(true)
    expect(torso.geometry).toBe(skinSource)
    expect(shirt.geometry).toBe(shirtSource)
  })

  it('keeps the waist body when the linen shirt is unavailable', () => {
    const { body, parts, visible } = fixture()
    const torso = body.find((mesh) => mesh.userData.region === 'torso')!
    const source = new THREE.PlaneGeometry(0.3, 0.5)
    source.translate(0, 1.305, 0)
    torso.geometry = source
    showModularOutfit(body, parts, { ...undressed, top: 'linen' })
    expect(visible('torso')).toBe(true)
    expect(torso.geometry).toBe(source)
  })
})

describe('priest trousers preview', () => {
  it('hides skin behind the rogue undershirt and restores it with equipment changes', () => {
    const { body, parts } = fixture()
    parts.get('top_rogue')![0].userData.fitting_status = 'candidate_tripo_v1'
    parts.set('pants_priest', [new THREE.SkinnedMesh()])
    const torso = body.find((mesh) => mesh.userData.region === 'torso')!
    const source = new THREE.PlaneGeometry(0.3, 0.4, 2, 4)
    source.translate(0, 1.25, 0)
    torso.geometry = source
    const outfit = {
      ...undressed,
      top: 'rogue' as const,
      pants: 'priest' as const,
    }
    showModularOutfit(body, parts, outfit)
    const covered = torso.geometry
    covered.computeBoundingBox()
    expect(torso.visible).toBe(true)
    expect(covered.boundingBox!.max.y).toBeCloseTo(1.105)
    expect(covered.boundingBox!.min.y).toBeCloseTo(1.05)
    showModularOutfit(body, parts, { ...outfit, pants: 'none' })
    torso.geometry.computeBoundingBox()
    expect(torso.geometry.boundingBox!.max.y).toBeCloseTo(1.14)
    showModularOutfit(body, parts, outfit)
    expect(torso.geometry).toBe(covered)
    showModularOutfit(body, parts, { ...outfit, top: 'none' })
    expect(torso.visible).toBe(true)
    expect(torso.geometry).toBe(source)
    showModularOutfit(body, parts, outfit)
    parts.delete('top_rogue')
    showModularOutfit(body, parts, outfit)
    expect(torso.visible).toBe(true)
    expect(torso.geometry).toBe(source)
  })

  it('trims the waist under a loaded rogue top while preserving boot trims and restoration', () => {
    const { body, parts } = fixture()
    const source = new THREE.PlaneGeometry(0.3, 0.96, 2, 12)
    source.translate(0, 0.68, 0.08)
    const pants = new THREE.SkinnedMesh(source)
    parts.set('pants_priest', [pants])
    const outfit = {
      ...undressed,
      top: 'rogue' as const,
      pants: 'priest' as const,
    }
    const bounds = () => {
      pants.geometry.computeBoundingBox()
      return pants.geometry.boundingBox!
    }
    showModularOutfit(body, parts, outfit)
    const trimmed = pants.geometry
    expect(trimmed).not.toBe(source)
    expect(bounds().max.y).toBeCloseTo(1.105)
    expect(bounds().min.y).toBeCloseTo(0.2)
    showModularOutfit(body, parts, { ...outfit, boots: 'leather' })
    expect(bounds().max.y).toBeCloseTo(1.105)
    expect(bounds().min.y).toBeCloseTo(0.235)
    showModularOutfit(body, parts, outfit)
    expect(pants.geometry).toBe(trimmed)
    for (const top of ['none', 'linen', 'priest'] as const) {
      showModularOutfit(body, parts, { ...outfit, top })
      expect(pants.geometry).toBe(source)
    }
    showModularOutfit(body, parts, outfit)
    parts.delete('top_rogue')
    showModularOutfit(body, parts, outfit)
    expect(pants.geometry).toBe(source)
  })

  it('fits the waist inside a loaded plate top, combines boot trims and restores it', () => {
    const { body, parts } = fixture()
    const source = new THREE.CylinderGeometry(0.21, 0.23, 0.96, 32, 24, true)
    source.translate(0, 0.68, -0.035)
    const pants = new THREE.SkinnedMesh(source)
    parts.set('pants_priest', [pants])
    parts.set('top_plate', [new THREE.SkinnedMesh()])
    const outfit = {
      ...undressed,
      top: 'plate' as const,
      pants: 'priest' as const,
    }
    const bounds = () => {
      pants.geometry.computeBoundingBox()
      return pants.geometry.boundingBox!
    }
    showModularOutfit(body, parts, outfit)
    const fitted = pants.geometry
    expect(fitted).not.toBe(source)
    expect(bounds().max.y).toBeLessThanOrEqual(1.060001)
    expect(bounds().min.y).toBeCloseTo(0.2)
    const position = fitted.attributes.position
    for (let i = 0; i < position.count; i++)
      if (position.getY(i) > 1.04)
        expect(
          Math.hypot(position.getX(i) / 0.16, (position.getZ(i) + 0.035) / 0.1)
        ).toBeLessThanOrEqual(1.000001)
    showModularOutfit(body, parts, { ...outfit, boots: 'leather' })
    expect(bounds().max.y).toBeLessThanOrEqual(1.060001)
    expect(bounds().min.y).toBeCloseTo(0.235)
    showModularOutfit(body, parts, outfit)
    expect(pants.geometry).toBe(fitted)
    for (const top of ['none', 'linen', 'priest'] as const) {
      showModularOutfit(body, parts, { ...outfit, top })
      expect(pants.geometry).toBe(source)
    }
    showModularOutfit(body, parts, outfit)
    parts.delete('top_plate')
    showModularOutfit(body, parts, outfit)
    expect(pants.geometry).toBe(source)
  })

  it.each([
    ['leather', 0.235],
    ['plate', 0.22],
    ['barbarian', 0.479],
    ['caveman', null],
    ['ranger', null],
  ] as const)(
    'trims the hem inside %s boots and restores it when boots are removed or unavailable',
    (boots, height) => {
      const { body, parts } = fixture()
      const original = new THREE.PlaneGeometry(0.2, 0.5, 2, 4)
      original.translate(0.17, 0.35, 0)
      const pants = new THREE.SkinnedMesh(original)
      parts.set('pants_priest', [pants])
      parts.set('boots_plate', [new THREE.SkinnedMesh()])
      parts.set('boots_barbarian', [new THREE.SkinnedMesh()])
      parts.set('boots_caveman', [new THREE.SkinnedMesh()])
      parts.set('boots_ranger', [new THREE.SkinnedMesh()])
      const outfit = {
        ...undressed,
        pants: 'priest' as const,
        boots,
      }
      showModularOutfit(body, parts, outfit)
      const trimmed = pants.geometry
      trimmed.computeBoundingBox()
      if (height === null) {
        expect(trimmed.boundingBox!.min.y).toBeGreaterThan(0.43)
        expect(trimmed.boundingBox!.min.y).toBeLessThan(0.48)
      } else expect(trimmed.boundingBox!.min.y).toBeCloseTo(height)
      expect(trimmed.boundingBox!.max.y).toBeCloseTo(0.6)
      original.computeBoundingBox()
      expect(original.boundingBox!.min.y).toBeCloseTo(0.1)
      for (const replacement of ['none', 'rogue'] as const) {
        showModularOutfit(body, parts, { ...outfit, boots: replacement })
        expect(pants.geometry).toBe(original)
        showModularOutfit(body, parts, outfit)
        expect(pants.geometry).toBe(trimmed)
      }
      const otherBoots = boots === 'leather' ? 'plate' : 'leather'
      showModularOutfit(body, parts, { ...outfit, boots: otherBoots })
      pants.geometry.computeBoundingBox()
      expect(pants.geometry.boundingBox!.min.y).toBeCloseTo(
        otherBoots === 'leather' ? 0.235 : 0.22
      )
      parts.delete(`boots_${boots}`)
      showModularOutfit(body, parts, outfit)
      expect(pants.geometry).toBe(original)
    }
  )

  it('keeps lower legs beneath barbarian greaves and covers them again for other boots', () => {
    const { body, parts, visible } = fixture()
    const legs = body.find((mesh) => mesh.userData.region === 'legs')!
    const original = new THREE.PlaneGeometry(0.2, 0.7, 2, 6)
    original.translate(0.17, 0.4, 0)
    legs.geometry = original
    parts.set('pants_priest', [new THREE.SkinnedMesh()])
    parts.set('boots_barbarian', [new THREE.SkinnedMesh()])
    const outfit = {
      ...undressed,
      pants: 'priest' as const,
      boots: 'barbarian' as const,
    }
    showModularOutfit(body, parts, outfit)
    expect(visible('legs')).toBe(true)
    expect(visible('ankles')).toBe(true)
    expect(visible('boot_ankles')).toBe(false)
    legs.geometry.computeBoundingBox()
    expect(legs.geometry.boundingBox!.max.y).toBeCloseTo(0.479)
    expect(legs.geometry.boundingBox!.min.y).toBeCloseTo(0.05)
    showModularOutfit(body, parts, { ...outfit, boots: 'leather' })
    expect(visible('legs')).toBe(false)
    expect(legs.geometry).toBe(original)
    showModularOutfit(body, parts, outfit)
    parts.delete('boots_barbarian')
    showModularOutfit(body, parts, outfit)
    expect(visible('legs')).toBe(false)
    showModularOutfit(body, parts, undressed)
    expect(visible('legs')).toBe(true)
    expect(legs.geometry).toBe(original)
  })

  it('replaces cloth trousers and restores bare legs when removed', () => {
    const { body, parts, visible } = fixture()
    const pants = new THREE.SkinnedMesh()
    parts.set('pants_priest', [pants])
    const outfit = { ...undressed, pants: PRIEST_MODULAR_OUTFIT.pants }
    const selected = showModularOutfit(body, parts, outfit)
    expect(selected.has('pants_priest')).toBe(true)
    expect(selected.has('pants_cloth')).toBe(false)
    expect(pants.visible).toBe(true)
    for (const region of ['legs', 'ankles', 'boot_ankles'])
      expect(visible(region)).toBe(false)
    expect(visible('feet')).toBe(true)
    showModularOutfit(body, parts, undressed)
    expect(pants.visible).toBe(false)
    for (const region of ['legs', 'ankles', 'feet'])
      expect(visible(region)).toBe(true)
  })

  it('keeps legs visible if the priest trousers fail to load', () => {
    const { body, parts, visible } = fixture()
    const selected = showModularOutfit(body, parts, {
      ...undressed,
      pants: PRIEST_MODULAR_OUTFIT.pants,
    })
    expect(selected.has('pants_priest')).toBe(false)
    expect(selected.has('pants_cloth')).toBe(false)
    for (const region of ['legs', 'ankles', 'feet'])
      expect(visible(region)).toBe(true)
  })
})

describe('ranger workshop preview', () => {
  it.each([
    'none',
    'linen',
    'leather',
    'plate',
    'barbarian',
    'rogue',
    'caveman',
    'ranger',
  ] as const)(
    'keeps forearm overlap with a %s top and restores skin when gloves are unavailable',
    (top) => {
      const { body, parts, visible } = fixture()
      const forearms = body.find((mesh) => mesh.userData.region === 'forearms')!
      const original = new THREE.PlaneGeometry(0.1, 0.3, 2, 6)
      original.translate(-0.38, 1.12, 0)
      forearms.geometry = original
      const gloves = [new THREE.SkinnedMesh(), new THREE.SkinnedMesh()]
      parts.set('gloves_ranger', gloves)
      parts.set(`top_${top}`, [new THREE.SkinnedMesh()])
      const outfit = { ...undressed, top, gloves: 'ranger' as const }
      const selected = showModularOutfit(body, parts, outfit)
      expect(selected.has('gloves_ranger')).toBe(true)
      expect(gloves.every((mesh) => mesh.visible)).toBe(true)
      expect(visible('hands')).toBe(true)
      expect(forearms.geometry).not.toBe(original)
      const position = forearms.geometry.attributes.position
      expect(position.count).toBeGreaterThan(0)
      expect(
        Math.min(
          ...Array.from({ length: position.count }, (_, i) => position.getY(i))
        )
      ).toBeGreaterThan(1.12)
      const wrist = new THREE.Vector3(-0.445653, 0.993593, -0.045557)
      const axis = wrist
        .clone()
        .sub(new THREE.Vector3(-0.311339, 1.247962, -0.056036))
        .normalize()
      const skinEnd = Math.max(
        ...Array.from({ length: position.count }, (_, i) =>
          new THREE.Vector3()
            .fromBufferAttribute(position, i)
            .sub(wrist)
            .dot(axis)
        )
      )
      expect(skinEnd + 0.208).toBeGreaterThan(0.015)
      showModularOutfit(body, parts, undressed)
      expect(gloves.every((mesh) => !mesh.visible)).toBe(true)
      expect(forearms.geometry).toBe(original)
      showModularOutfit(body, parts, outfit)
      parts.delete('gloves_ranger')
      expect(showModularOutfit(body, parts, outfit).has('gloves_ranger')).toBe(
        false
      )
      expect(forearms.geometry).toBe(original)
      expect(visible('hands')).toBe(true)
    }
  )

  it('keeps exposed arms and restores skin and neck when removed', () => {
    const { body, parts, visible } = fixture()
    const top = new THREE.SkinnedMesh()
    parts.set('top_ranger', [top])
    const pants = new THREE.SkinnedMesh()
    parts.set('pants_ranger', [pants])
    const neck = body.find((mesh) => mesh.userData.region === 'neck')!
    const original = new THREE.PlaneGeometry(0.2, 0.2, 4, 4)
    original.translate(0, 1.54, 0)
    neck.geometry = original
    const selected = showModularOutfit(body, parts, RANGER_MODULAR_OUTFIT)
    expect(selected.has('top_ranger')).toBe(true)
    expect(selected.has('pants_ranger')).toBe(true)
    expect(selected.has('pants_cloth')).toBe(false)
    expect(pants.visible).toBe(true)
    expect(top.visible).toBe(true)
    for (const region of ['torso', 'upper_arms', 'legs', 'ankles'])
      expect(visible(region)).toBe(false)
    for (const region of ['neck', 'forearms', 'hands'])
      expect(visible(region)).toBe(true)
    const points = neck.geometry.attributes.position
    expect(points.count).toBeGreaterThan(0)
    for (let i = 0; i < points.count; i++)
      expect(points.getY(i)).toBeGreaterThanOrEqual(1.54 - 1e-6)
    neck.geometry.computeBoundingBox()
    expect(neck.geometry.boundingBox!.min.x).toBeCloseTo(-0.09)
    expect(neck.geometry.boundingBox!.max.x).toBeCloseTo(0.09)
    showModularOutfit(body, parts, undressed)
    expect(top.visible).toBe(false)
    expect(pants.visible).toBe(false)
    expect(neck.geometry).toBe(original)
    for (const region of [
      'torso',
      'upper_arms',
      'neck',
      'forearms',
      'hands',
      'legs',
      'ankles',
    ])
      expect(visible(region)).toBe(true)
    showModularOutfit(body, parts, RANGER_MODULAR_OUTFIT)
    expect(top.visible).toBe(true)
    expect(visible('torso')).toBe(false)
  })

  it('preserves exposed skin when the ranger top is unavailable', () => {
    const { body, parts, visible } = fixture()
    const selected = showModularOutfit(body, parts, RANGER_MODULAR_OUTFIT)
    expect(selected.has('top_ranger')).toBe(false)
    expect(selected.has('pants_ranger')).toBe(false)
    expect(selected.has('pants_cloth')).toBe(false)
    for (const region of [
      'torso',
      'upper_arms',
      'neck',
      'forearms',
      'hands',
      'legs',
      'ankles',
    ])
      expect(visible(region)).toBe(true)
  })

  it('masks legs only when ranger pants load and restores them on replacement', () => {
    const { body, parts, visible } = fixture()
    const pants = new THREE.SkinnedMesh()
    parts.set('pants_ranger', [pants])
    showModularOutfit(body, parts, RANGER_MODULAR_OUTFIT)
    expect(visible('torso')).toBe(true)
    expect(visible('legs')).toBe(false)
    expect(visible('feet')).toBe(true)
    showModularOutfit(body, parts, { ...undressed, pants: 'cloth' })
    expect(pants.visible).toBe(false)
    expect(parts.get('pants_cloth')![0].visible).toBe(true)
    showModularOutfit(body, parts, undressed)
    expect(visible('legs')).toBe(true)
    expect(visible('ankles')).toBe(true)
  })
})

describe('caveman workshop preview', () => {
  it('tucks plate pants inside caveman boots and restores the pants for other boots', () => {
    const { body, parts } = fixture()
    const pants = [new THREE.SkinnedMesh(), new THREE.SkinnedMesh()]
    const originals = pants.map((mesh, i) => {
      const geometry = new THREE.PlaneGeometry(0.2, 0.8, 2, 4)
      geometry.translate(i ? -0.17 : 0.17, 0.4, 0)
      mesh.geometry = geometry
      return geometry
    })
    parts.set('pants_plate', pants)
    parts.set('boots_caveman', [new THREE.SkinnedMesh()])
    const outfit = { ...CAVEMAN_MODULAR_OUTFIT, pants: 'plate' as const }
    const selected = showModularOutfit(body, parts, outfit)
    expect(selected.has('pants_plate')).toBe(true)
    for (const mesh of pants) {
      expect(mesh.visible).toBe(true)
      const points = mesh.geometry.attributes.position
      expect(points.count).toBeGreaterThan(0)
      const heights = Array.from({ length: points.count }, (_, i) =>
        points.getY(i)
      )
      expect(Math.min(...heights)).toBeCloseTo(0.43)
      expect(Math.max(...heights)).toBeCloseTo(0.8)
    }
    for (const boots of ['none', 'plate'] as const) {
      showModularOutfit(body, parts, { ...outfit, boots })
      for (const [i, mesh] of pants.entries())
        expect(mesh.geometry).toBe(originals[i])
      showModularOutfit(body, parts, outfit)
    }
    showModularOutfit(body, parts, { ...outfit, boots: 'leather' })
    for (const mesh of pants) {
      const points = mesh.geometry.attributes.position
      expect(
        Math.min(
          ...Array.from({ length: points.count }, (_, i) => points.getY(i))
        )
      ).toBeCloseTo(0.235)
    }
    showModularOutfit(body, parts, outfit)
    for (const mesh of pants) {
      const points = mesh.geometry.attributes.position
      expect(
        Math.min(
          ...Array.from({ length: points.count }, (_, i) => points.getY(i))
        )
      ).toBeCloseTo(0.43)
    }
    showModularOutfit(body, parts, { ...outfit, boots: 'barbarian' })
    for (const mesh of pants) {
      const points = mesh.geometry.attributes.position
      expect(
        Math.min(
          ...Array.from({ length: points.count }, (_, i) => points.getY(i))
        )
      ).toBeCloseTo(0.46)
    }
    parts.delete('boots_caveman')
    showModularOutfit(body, parts, outfit)
    for (const [i, mesh] of pants.entries())
      expect(mesh.geometry).toBe(originals[i])
  })

  it('clips covered forearms, keeps hands and restores skin when bracers are removed', () => {
    const { body, parts, visible } = fixture()
    const forearm = body.find((mesh) => mesh.userData.region === 'forearms')!
    const original = new THREE.BufferGeometry()
    original.setAttribute(
      'position',
      new THREE.Float32BufferAttribute(
        [0.31, 1.23, -0.05, 0.34, 1.16, -0.05, 0.35, 1.15, -0.05],
        3
      )
    )
    original.setIndex([0, 1, 2])
    forearm.geometry = original
    const bracer = new THREE.SkinnedMesh()
    const proxy = new THREE.SkinnedMesh()
    parts.set('gloves_caveman', [bracer])
    parts.set('gloves_barbarian', [proxy])
    const selected = showModularOutfit(body, parts, CAVEMAN_MODULAR_OUTFIT)
    expect(selected.has('gloves_caveman')).toBe(true)
    expect(selected.has('gloves_barbarian')).toBe(false)
    expect(proxy.visible).toBe(false)
    expect(visible('hands')).toBe(true)
    expect(visible('forearms')).toBe(true)
    expect(forearm.geometry).not.toBe(original)
    const elbow = new THREE.Vector3(0.311339, 1.247962, -0.056036)
    const wrist = new THREE.Vector3(0.445653, 0.993593, -0.045557)
    const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(
      elbow.clone().sub(wrist).normalize(),
      elbow.lerp(wrist, 0.23)
    )
    const points = forearm.geometry.attributes.position
    expect(points.count).toBeGreaterThan(0)
    for (let i = 0; i < points.count; i++)
      expect(
        plane.distanceToPoint(
          new THREE.Vector3().fromBufferAttribute(points, i)
        )
      ).toBeGreaterThanOrEqual(-1e-6)
    showModularOutfit(body, parts, {
      ...CAVEMAN_MODULAR_OUTFIT,
      gloves: 'none',
    })
    expect(forearm.geometry).toBe(original)
    expect(bracer.visible).toBe(false)
    expect(visible('hands')).toBe(true)
    showModularOutfit(body, parts, CAVEMAN_MODULAR_OUTFIT)
    parts.delete('gloves_caveman')
    showModularOutfit(body, parts, CAVEMAN_MODULAR_OUTFIT)
    expect(forearm.geometry).toBe(original)
    expect(visible('hands')).toBe(true)
    expect(proxy.visible).toBe(false)
  })

  it('covers skin inside the boots and restores the original legs when removed', () => {
    const { body, parts, visible } = fixture()
    const legs = body.find((mesh) => mesh.userData.region === 'legs')!
    const original = new THREE.PlaneGeometry(0.2, 0.8, 2, 4)
    original.translate(0.17, 0.4, 0)
    legs.geometry = original
    const boots = new THREE.SkinnedMesh()
    parts.set('boots_caveman', [boots])
    const selected = showModularOutfit(body, parts, CAVEMAN_MODULAR_OUTFIT)
    expect(selected.has('boots_caveman')).toBe(true)
    expect(selected.has('boots_leather')).toBe(false)
    expect(parts.get('boots_leather')![0].visible).toBe(false)
    for (const region of ['feet', 'ankles', 'boot_ankles'])
      expect(visible(region)).toBe(false)
    const position = legs.geometry.getAttribute('position')
    expect(position.count).toBeGreaterThan(0)
    for (let i = 0; i < position.count; i++)
      expect(position.getY(i)).toBeGreaterThanOrEqual(0.42999)
    showModularOutfit(body, parts, { ...CAVEMAN_MODULAR_OUTFIT, boots: 'none' })
    expect(legs.geometry).toBe(original)
    expect(visible('feet')).toBe(true)
    expect(visible('ankles')).toBe(true)
    expect(boots.visible).toBe(false)
  })

  it('keeps bare skin when the requested caveman boots failed to load', () => {
    const { body, parts, visible } = fixture()
    const selected = showModularOutfit(body, parts, CAVEMAN_MODULAR_OUTFIT)
    expect(selected.has('boots_caveman')).toBe(false)
    expect(selected.has('boots_leather')).toBe(false)
    expect(visible('legs')).toBe(true)
    expect(visible('feet')).toBe(true)
    expect(visible('ankles')).toBe(true)
  })

  it('shows the pelt skirt with exposed legs, hides the proxy and restores other outfits', () => {
    const { body, parts, visible } = fixture()
    const skirt = new THREE.SkinnedMesh()
    const proxy = new THREE.SkinnedMesh()
    parts.set('pants_caveman', [skirt])
    parts.set('pants_barbarian', [proxy])
    const selected = showModularOutfit(body, parts, CAVEMAN_MODULAR_OUTFIT)
    expect(selected.has('pants_caveman')).toBe(true)
    expect(selected.has('pants_barbarian')).toBe(false)
    expect(skirt.visible).toBe(true)
    expect(proxy.visible).toBe(false)
    expect(visible('legs')).toBe(true)
    expect(visible('feet')).toBe(true)
    showModularOutfit(body, parts, KNIGHT_MODULAR_OUTFIT)
    expect(skirt.visible).toBe(false)
    expect(visible('legs')).toBe(false)
    showModularOutfit(body, parts, CAVEMAN_MODULAR_OUTFIT)
    expect(skirt.visible).toBe(true)
    expect(visible('legs')).toBe(true)
  })
  it('restores bare skin after another top and hides the pelt when switching', () => {
    const { body, parts, visible } = fixture()
    const pelt = new THREE.SkinnedMesh()
    parts.set('top_caveman', [pelt])
    showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
    expect(visible('torso')).toBe(false)
    const selected = showModularOutfit(body, parts, CAVEMAN_MODULAR_OUTFIT)
    expect(selected.has('top_caveman')).toBe(true)
    expect(pelt.visible).toBe(true)
    for (const mesh of body)
      expect(mesh.visible).toBe(mesh.userData.region !== 'boot_ankles')
    showModularOutfit(body, parts, KNIGHT_MODULAR_OUTFIT)
    expect(pelt.visible).toBe(false)
    expect(visible('torso')).toBe(false)
  })

  it('keeps bare skin when the top is unavailable', () => {
    const { body, parts } = fixture()
    const selected = showModularOutfit(body, parts, CAVEMAN_MODULAR_OUTFIT)
    expect(selected.has('top_caveman')).toBe(false)
    for (const mesh of body)
      expect(mesh.visible).toBe(mesh.userData.region !== 'boot_ankles')
  })
})

describe('rogue workshop preview', () => {
  it('shows four candidate slots and keeps bare forearms and fingers visible', () => {
    const { body, parts, visible } = fixture()
    const selected = showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
    for (const id of ROGUE_MODULAR_PARTS) {
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
    showModularOutfit(body, parts, KNIGHT_MODULAR_OUTFIT)
    expect(visible('hands')).toBe(false)
    showModularOutfit(body, parts, undressed)
    for (const mesh of body)
      expect(mesh.visible).toBe(mesh.userData.region !== 'boot_ankles')
    for (const id of ROGUE_MODULAR_PARTS)
      expect(parts.get(id)![0].visible).toBe(false)
  })

  it('keeps skin when candidate files are unavailable', () => {
    const { body, parts, visible } = fixture()
    for (const id of ROGUE_MODULAR_PARTS) parts.delete(id)
    const selected = showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
    for (const id of ROGUE_MODULAR_PARTS) expect(selected.has(id)).toBe(false)
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
      showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
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
      showModularOutfit(body, parts, KNIGHT_MODULAR_OUTFIT)
      expect(visible('neck')).toBe(false)
      expect(neck.geometry).toBe(source)
      showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
      parts.delete('top_rogue')
      showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
      expect(visible('neck')).toBe(true)
      expect(neck.geometry).toBe(source)
    }
  )

  it('tucks existing cloth cuffs into rogue boots and restores them on removal', () => {
    const { body, parts, visible } = fixture()
    showModularOutfit(body, parts, {
      ...undressed,
      pants: 'cloth',
      boots: 'rogue',
    })
    const [, cuffs, tucked] = parts.get('pants_cloth')!
    expect(cuffs.visible).toBe(false)
    expect(tucked.visible).toBe(true)
    showModularOutfit(body, parts, { ...undressed, pants: 'cloth' })
    expect(cuffs.visible).toBe(true)
    expect(tucked.visible).toBe(false)
    expect(visible('feet')).toBe(true)
    showModularOutfit(body, parts, {
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
    showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
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
    showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
    expect(visible('torso')).toBe(false)
    expect(torso.geometry).toBe(source)
    top.userData.fitting_status = 'candidate_tripo_v1'
    showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
    parts.delete('top_rogue')
    showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
    expect(visible('torso')).toBe(true)
    expect(torso.geometry).toBe(source)
  })

  it.each(['rogue', 'linen', 'leather', 'plate'] as const)(
    'hides the high trouser waistband under %s and restores it with a bare waist',
    (top) => {
      const { body, parts } = fixture()
      const pants = parts.get('pants_rogue')![0]
      pants.userData.fitting_status = 'candidate_tripo_pants_v1'
      const source = new THREE.BufferGeometry()
      source.setAttribute(
        'position',
        new THREE.Float32BufferAttribute(
          [-0.1, 1, 0.1, 0.1, 1, 0.1, 0.1, 1.16, 0.1, -0.1, 1.16, 0.1],
          3
        )
      )
      source.setIndex([0, 1, 2, 0, 2, 3])
      const original = source.attributes.position.array.slice()
      pants.geometry = source
      showModularOutfit(body, parts, { ...ROGUE_MODULAR_OUTFIT, top })
      const trimmed = pants.geometry
      expect(trimmed).not.toBe(source)
      expect(trimmed.index!.count).toBeGreaterThan(0)
      const heights = Array.from(
        { length: trimmed.attributes.position.count },
        (_, i) => trimmed.attributes.position.getY(i)
      )
      expect(Math.max(...heights)).toBeLessThan(1.11)
      expect(Math.min(...heights)).toBeCloseTo(1)
      for (const bareTop of ['none', 'barbarian'] as const) {
        showModularOutfit(body, parts, {
          ...ROGUE_MODULAR_OUTFIT,
          top: bareTop,
        })
        expect(pants.geometry).toBe(source)
        expect(source.attributes.position.array).toEqual(original)
      }
      showModularOutfit(body, parts, { ...ROGUE_MODULAR_OUTFIT, top })
      expect(pants.geometry).toBe(trimmed)
      if (top === 'rogue') {
        parts.delete('top_rogue')
        showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
        expect(pants.geometry).toBe(source)
      }
    }
  )

  it('restores waist skin when the Tripo trousers are removed', () => {
    const { body, parts } = fixture()
    parts.get('top_rogue')![0].userData.fitting_status = 'candidate_tripo_v1'
    parts.get('pants_rogue')![0].userData.fitting_status =
      'candidate_tripo_pants_v1'
    const torso = body.find((mesh) => mesh.userData.region === 'torso')!
    torso.geometry = new THREE.BufferGeometry()
    torso.geometry.setAttribute(
      'position',
      new THREE.Float32BufferAttribute(
        [-0.1, 1.05, 0.09, 0.1, 1.05, 0.09, 0, 1.14, 0.09],
        3
      )
    )
    torso.geometry.setIndex([0, 1, 2])
    showModularOutfit(body, parts, ROGUE_MODULAR_OUTFIT)
    const heights = () => {
      const position = torso.geometry.attributes.position
      return Math.max(
        ...Array.from({ length: position.count }, (_, i) => position.getY(i))
      )
    }
    expect(heights()).toBeCloseTo(1.105)
    showModularOutfit(body, parts, { ...ROGUE_MODULAR_OUTFIT, pants: 'none' })
    expect(heights()).toBeCloseTo(1.14)
  })
})
