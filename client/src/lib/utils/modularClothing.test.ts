import * as THREE from 'three'
import { describe, expect, it, vi } from 'vitest'
import { clipSkinnedGeometry, trimModularClothing } from './modularClothing'
import { DEFAULT_MODULAR_OUTFIT, showModularOutfit } from './modularCharacter'
import { cavemanBootRim, cavemanPantsBootDistance } from './cavemanBootCuff'

it('disposes unchanged underwear geometry once', () => {
  const geometry = new THREE.PlaneGeometry(0.2, 0.2)
  const mesh = new THREE.SkinnedMesh(geometry)
  const dispose = vi.fn()
  geometry.addEventListener('dispose', dispose)
  trimModularClothing(mesh, 'underwear_seat', true)
  expect(mesh.geometry).toBe(geometry)
  geometry.dispose()
  expect(dispose).toHaveBeenCalledTimes(1)
})

function cloth() {
  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute(
    'position',
    new THREE.Float32BufferAttribute([-1, 0, 0, 1, 0, 0, 1, 1, 0, -1, 1, 0], 3)
  )
  geometry.setAttribute(
    'uv',
    new THREE.Float32BufferAttribute([0, 0, 1, 0, 1, 1, 0, 1], 2)
  )
  geometry.setAttribute(
    'skinIndex',
    new THREE.Uint16BufferAttribute(
      [0, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
      4
    )
  )
  geometry.setAttribute(
    'skinWeight',
    new THREE.Float32BufferAttribute(
      [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
      4
    )
  )
  geometry.setIndex([0, 1, 2, 0, 2, 3])
  return geometry
}

function area(geometry: THREE.BufferGeometry) {
  let result = 0
  const points = [new THREE.Vector3(), new THREE.Vector3(), new THREE.Vector3()]
  for (let i = 0; i < geometry.index!.count; i += 3) {
    points.forEach((p, j) =>
      p.fromBufferAttribute(
        geometry.attributes.position,
        geometry.index!.getX(i + j)
      )
    )
    result += points[1].sub(points[0]).cross(points[2].sub(points[0])).z / 2
  }
  return result
}

it('rebuilds render bindings when a clothing variant removes or restores tangents', () => {
  const source = cloth()
  source.computeVertexNormals()
  source.computeTangents()
  const mesh = new THREE.SkinnedMesh(source, new THREE.MeshStandardMaterial())
  const other = new THREE.SkinnedMesh(source, mesh.material)
  const releaseBindings = vi.fn()
  const releaseOtherBindings = vi.fn()
  const disposeSource = vi.fn()
  const disposeMaterial = vi.fn()
  mesh.addEventListener('dispose', releaseBindings)
  other.addEventListener('dispose', releaseOtherBindings)
  source.addEventListener('dispose', disposeSource)
  mesh.material.addEventListener('dispose', disposeMaterial)

  trimModularClothing(mesh, 'priest_fur')
  expect(mesh.geometry.hasAttribute('tangent')).toBe(false)
  expect(releaseBindings).toHaveBeenCalledTimes(1)
  trimModularClothing(mesh, 'priest_fur')
  expect(releaseBindings).toHaveBeenCalledTimes(1)

  trimModularClothing(mesh)
  expect(mesh.geometry).toBe(source)
  expect(mesh.geometry.hasAttribute('tangent')).toBe(true)
  expect(releaseBindings).toHaveBeenCalledTimes(2)
  expect(other.geometry).toBe(source)
  expect(releaseOtherBindings).not.toHaveBeenCalled()
  expect(disposeSource).not.toHaveBeenCalled()
  expect(disposeMaterial).not.toHaveBeenCalled()
})

describe('modular clothing cuts', () => {
  it.each([1, -1])(
    'fits the priest hem to the caveman boot rim on side %s',
    (side) => {
      const source = new THREE.CylinderGeometry(0.1, 0.1, 0.25, 32, 3, true)
      source.translate(side * 0.1664, 0.5, -0.045)
      const count = source.attributes.position.count
      source.setAttribute(
        'skinIndex',
        new THREE.Uint16BufferAttribute(
          Array.from({ length: count }, () => [1, 6, 0, 0]).flat(),
          4
        )
      )
      source.setAttribute(
        'skinWeight',
        new THREE.Float32BufferAttribute(
          Array.from({ length: count }, () => [0.5, 0.5, 0, 0]).flat(),
          4
        )
      )
      const mesh = new THREE.SkinnedMesh(source)
      trimModularClothing(mesh, 'caveman_pants_boots')
      const { position, uv, skinIndex, skinWeight } = mesh.geometry.attributes
      const point = new THREE.Vector3()
      let boundary = 0
      for (let i = 0; i < position.count; i++) {
        point.fromBufferAttribute(position, i)
        expect(cavemanPantsBootDistance(point)).toBeGreaterThanOrEqual(-1e-6)
        const rim = cavemanBootRim(point)
        if (Math.abs(cavemanPantsBootDistance(point)) < 1e-5) {
          boundary++
          expect(rim.radius).toBeLessThanOrEqual(rim.innerRadius + 1e-6)
          expect(skinIndex.getX(i)).toBe(side === 1 ? 2 : 7)
          expect(skinWeight.getX(i)).toBeCloseTo(1)
        }
        expect(
          skinWeight.getX(i) +
            skinWeight.getY(i) +
            skinWeight.getZ(i) +
            skinWeight.getW(i)
        ).toBeCloseTo(1)
        expect(Number.isFinite(uv.getX(i)) && Number.isFinite(uv.getY(i))).toBe(
          true
        )
      }
      expect(boundary).toBeGreaterThan(16)
      source.computeBoundingBox()
      expect(source.boundingBox!.min.y).toBeCloseTo(0.375)
      trimModularClothing(mesh)
      expect(mesh.geometry).toBe(source)
    }
  )

  it('removes detached rogue pockets while retaining UV-split trousers, belt loops and skinning', () => {
    const source = new THREE.BufferGeometry()
    source.setAttribute(
      'position',
      new THREE.Float32BufferAttribute(
        [
          -0.1, 0.3, 0, 0.1, 0.95, 0, -0.1, 0.95, 0, -0.1, 0.95, 0, 0.1, 0.95,
          0, 0.1, 1.1, 0, 0.19, 0.94, 0.1, 0.25, 0.94, 0.1, 0.25, 1.08, 0.1,
          0.19, 1.08, 0.1, 0.05, 1.02, 0.1, 0.07, 1.02, 0.1, 0.05, 1.1, 0.1,
        ],
        3
      )
    )
    const count = source.attributes.position.count
    source.setAttribute(
      'uv',
      new THREE.Float32BufferAttribute(
        Array.from({ length: count }, (_, i) => [i, 0]).flat(),
        2
      )
    )
    source.setAttribute(
      'skinIndex',
      new THREE.Uint16BufferAttribute(
        Array.from({ length: count }, () => [1, 2, 0, 0]).flat(),
        4
      )
    )
    source.setAttribute(
      'skinWeight',
      new THREE.Float32BufferAttribute(
        Array.from({ length: count }, () => [0.7, 0.3, 0, 0]).flat(),
        4
      )
    )
    source.setIndex([0, 1, 2, 3, 4, 5, 6, 7, 8, 6, 8, 9, 10, 11, 12])
    const mesh = new THREE.SkinnedMesh(source)
    const second = new THREE.SkinnedMesh(source)
    const originalIndex = source.index!.array.slice()
    trimModularClothing(mesh, 'priest_rogue_pockets')
    expect(mesh.geometry.index!.count).toBe(9)
    const retained = Array.from(mesh.geometry.attributes.uv.array).filter(
      (_, i) => i % 2 === 0
    )
    expect(retained).toEqual([0, 1, 2, 3, 4, 5, 10, 11, 12])
    for (const [i, original] of retained.entries())
      for (const name of ['position', 'skinIndex', 'skinWeight']) {
        const attr = mesh.geometry.attributes[name]
        for (let j = 0; j < attr.itemSize; j++)
          expect(attr.getComponent(i, j)).toBe(
            source.attributes[name].getComponent(original, j)
          )
      }
    trimModularClothing(second, 'priest_rogue_pockets')
    expect(second.geometry).toBe(mesh.geometry)
    trimModularClothing(mesh)
    expect(mesh.geometry).toBe(source)
    expect(source.index!.array).toEqual(originalIndex)
  })

  it('tucks both the belt and lower fur under a priest robe without changing topology or the source', () => {
    const source = cloth()
    const positions = source.attributes.position
    for (let i = 0; i < positions.count; i++)
      positions.setXYZ(
        i,
        positions.getX(i) * 0.24,
        0.7 + positions.getY(i) * 0.44,
        0.17
      )
    const mesh = new THREE.SkinnedMesh(source)
    const originalPositions = positions.array.slice()
    trimModularClothing(mesh, 'priest_fur')
    const fitted = mesh.geometry.attributes.position
    for (const name of ['uv', 'skinIndex', 'skinWeight'])
      expect(mesh.geometry.attributes[name].array).toEqual(
        source.attributes[name].array
      )
    expect(mesh.geometry.index!.array).toEqual(source.index!.array)
    for (let i = 0; i < fitted.count; i++) {
      expect(fitted.getY(i)).toBeCloseTo(positions.getY(i))
      expect(Math.abs(fitted.getX(i))).toBeLessThan(Math.abs(positions.getX(i)))
      expect(fitted.getZ(i)).toBeLessThan(0.13)
    }
    trimModularClothing(mesh)
    expect(mesh.geometry).toBe(source)
    expect(positions.array).toEqual(originalPositions)
  })

  it('tucks plate hips under a priest robe while preserving skinning, lower armor and independent boot cuts', () => {
    const source = cloth()
    const positions = source.attributes.position
    for (let i = 0; i < positions.count; i++)
      positions.setXYZ(
        i,
        positions.getX(i) * 0.22,
        0.3 + positions.getY(i) * 0.74,
        0.08
      )
    const first = new THREE.SkinnedMesh(source)
    const second = new THREE.SkinnedMesh(source)
    const originalPositions = positions.array.slice()
    trimModularClothing(first, 'priest_plate_waist')
    const tucked = first.geometry
    for (const name of ['uv', 'skinIndex', 'skinWeight'])
      expect(tucked.attributes[name].array).toEqual(
        source.attributes[name].array
      )
    expect(tucked.index!.array).toEqual(source.index!.array)
    for (let i = 0; i < positions.count; i++) {
      const fitted = tucked.attributes.position
      expect(fitted.getY(i)).toBeCloseTo(positions.getY(i))
      if (positions.getY(i) < 0.92) {
        expect(fitted.getX(i)).toBeCloseTo(positions.getX(i))
        expect(fitted.getZ(i)).toBeCloseTo(positions.getZ(i))
      } else expect(Math.abs(fitted.getX(i))).toBeLessThan(0.16)
    }
    trimModularClothing(second, 'priest_plate_waist')
    expect(second.geometry).toBe(tucked)
    trimModularClothing(first, ['greaves', 'priest_plate_waist'])
    first.geometry.computeBoundingBox()
    expect(first.geometry.boundingBox!.min.y).toBeCloseTo(0.46)
    trimModularClothing(first, 'greaves')
    first.geometry.computeBoundingBox()
    expect(first.geometry.boundingBox!.min.y).toBeCloseTo(0.46)
    expect(first.geometry.boundingBox!.max.x).toBeCloseTo(0.22)
    expect(second.geometry).toBe(tucked)
    trimModularClothing(first)
    expect(first.geometry).toBe(source)
    expect(source.attributes.position.array).toEqual(originalPositions)
  })

  it('preserves the exposed neck width with a ranger top and restores skin after removal or a missing top', () => {
    const source = cloth()
    const position = source.attributes.position
    for (let i = 0; i < position.count; i++)
      position.setXYZ(
        i,
        position.getX(i) * 0.12,
        1.5 + position.getY(i) * 0.12,
        0
      )
    const neck = new THREE.SkinnedMesh(source)
    neck.userData.region = 'neck'
    const top = new THREE.SkinnedMesh()
    const parts = new Map([['top_ranger', [top]]])
    const outfit = { ...DEFAULT_MODULAR_OUTFIT, top: 'ranger' } as const
    showModularOutfit([neck], parts, outfit)
    neck.geometry.computeBoundingBox()
    const bounds = neck.geometry.boundingBox!
    expect(bounds.min.x).toBeCloseTo(-0.09)
    expect(bounds.max.x).toBeCloseTo(0.09)
    expect(bounds.min.y).toBeCloseTo(1.54)
    expect(bounds.max.y).toBeCloseTo(1.62)
    expect(area(neck.geometry)).toBeCloseTo(0.18 * 0.08 - 0.05 * 0.04)
    showModularOutfit([neck], parts, { ...outfit, top: 'none' })
    expect(neck.geometry).toBe(source)
    showModularOutfit([neck], parts, outfit)
    parts.delete('top_ranger')
    showModularOutfit([neck], parts, outfit)
    expect(neck.geometry).toBe(source)
  })

  it('fits rogue pants into ranger boots while preserving the waist cut and restoring each independently', () => {
    const source = cloth()
    const position = source.attributes.position
    for (let i = 0; i < position.count; i++)
      position.setXYZ(
        i,
        0.159 + position.getX(i) * 0.07,
        0.1 + position.getY(i) * 1.1,
        0.05
      )
    const pants = new THREE.SkinnedMesh(source)
    pants.userData.fitting_status = 'candidate_tripo_pants_v1'
    const parts = new Map([
      ['pants_rogue', [pants]],
      ['boots_ranger', [new THREE.SkinnedMesh()]],
    ])
    const outfit = {
      ...DEFAULT_MODULAR_OUTFIT,
      pants: 'rogue',
      boots: 'ranger',
      top: 'plate',
    } as const
    const bounds = () => {
      pants.geometry.computeBoundingBox()
      return pants.geometry.boundingBox!
    }
    showModularOutfit([], parts, outfit)
    expect(bounds().min.y).toBeGreaterThan(0.455)
    expect(bounds().max.y).toBeCloseTo(1.105)
    showModularOutfit([], parts, { ...outfit, top: 'none' })
    expect(bounds().min.y).toBeGreaterThan(0.455)
    expect(bounds().max.y).toBeCloseTo(1.2)
    showModularOutfit([], parts, { ...outfit, boots: 'none' })
    expect(bounds().min.y).toBeCloseTo(0.1)
    expect(bounds().max.y).toBeCloseTo(1.105)
    parts.delete('boots_ranger')
    showModularOutfit([], parts, outfit)
    expect(bounds().min.y).toBeCloseTo(0.1)
    expect(bounds().max.y).toBeCloseTo(1.105)
    showModularOutfit([], parts, { ...outfit, top: 'none', boots: 'none' })
    expect(pants.geometry).toBe(source)
  })

  it('combines ranger boot and plate waist cuts and restores each independently', () => {
    const source = cloth()
    const position = source.attributes.position
    for (let i = 0; i < position.count; i++)
      position.setXYZ(
        i,
        position.getX(i) * 0.01,
        0.1 + position.getY(i) * 1.06,
        0.1
      )
    const pants = new THREE.SkinnedMesh(source)
    const boots = new THREE.SkinnedMesh()
    const parts = new Map([
      ['pants_ranger', [pants]],
      ['boots_ranger', [boots]],
    ])
    const outfit = { ...DEFAULT_MODULAR_OUTFIT, pants: 'ranger' } as const
    const bounds = () => {
      pants.geometry.computeBoundingBox()
      return pants.geometry.boundingBox!
    }
    showModularOutfit([], parts, { ...outfit, top: 'plate', boots: 'ranger' })
    const hem = bounds().min.y
    expect(hem).toBeGreaterThan(0.455)
    expect(hem).toBeLessThan(0.482)
    expect(bounds().max.y).toBeCloseTo(1.06)
    showModularOutfit([], parts, { ...outfit, top: 'none', boots: 'ranger' })
    expect(bounds().min.y).toBeCloseTo(hem)
    expect(bounds().max.y).toBeCloseTo(1.16)
    showModularOutfit([], parts, { ...outfit, top: 'plate', boots: 'none' })
    expect(bounds().min.y).toBeCloseTo(0.1)
    expect(bounds().max.y).toBeCloseTo(1.06)
    expect(boots.visible).toBe(false)
    parts.delete('boots_ranger')
    showModularOutfit([], parts, { ...outfit, top: 'none', boots: 'ranger' })
    expect(pants.geometry).toBe(source)
  })

  it('restores exposed skin after removing ranger boots or failing to load them', () => {
    const legs = new THREE.SkinnedMesh(cloth())
    const feet = new THREE.SkinnedMesh(cloth())
    legs.userData.region = 'legs'
    feet.userData.region = 'feet'
    const source = legs.geometry
    const parts = new Map([['boots_ranger', [new THREE.SkinnedMesh()]]])
    const outfit = {
      ...DEFAULT_MODULAR_OUTFIT,
      pants: 'none',
      boots: 'ranger',
    } as const
    showModularOutfit([legs, feet], parts, outfit)
    legs.geometry.computeBoundingBox()
    expect(legs.geometry.boundingBox!.min.y).toBeCloseTo(0.43)
    expect(legs.visible).toBe(true)
    expect(feet.visible).toBe(false)
    showModularOutfit([legs, feet], parts, { ...outfit, boots: 'none' })
    expect(legs.geometry).toBe(source)
    expect(feet.visible).toBe(true)
    parts.delete('boots_ranger')
    showModularOutfit([legs, feet], parts, outfit)
    expect(legs.geometry).toBe(source)
    expect(feet.visible).toBe(true)
  })

  it('follows the rear plate hem while preserving the front waist height', () => {
    const samples = [
      { x: 0.01, z: 0.1 },
      { x: 0.01, z: -0.12 },
      { x: 0.165, z: -0.08 },
    ].map(({ x, z }) => {
      const geometry = cloth()
      const positions = geometry.attributes.position
      for (let i = 0; i < positions.count; i++)
        positions.setXYZ(
          i,
          x + positions.getX(i) * 0.005,
          0.9 + positions.getY(i) * 0.26,
          z
        )
      return new THREE.SkinnedMesh(geometry)
    })
    const originals = samples.map((mesh) => mesh.geometry)
    const parts = new Map([['pants_ranger', samples]])
    showModularOutfit([], parts, {
      ...DEFAULT_MODULAR_OUTFIT,
      top: 'plate',
      pants: 'ranger',
    })
    const bounds = samples.map((mesh) => {
      mesh.geometry.computeBoundingBox()
      return mesh.geometry.boundingBox!
    })
    expect(bounds[0].max.y).toBeCloseTo(1.06)
    expect(bounds[1].max.y).toBeLessThan(1)
    expect(bounds[2].max.y).toBeGreaterThan(1.05)
    expect(bounds.every((box) => Math.abs(box.min.y - 0.9) < 1e-6)).toBe(true)
    showModularOutfit([], parts, {
      ...DEFAULT_MODULAR_OUTFIT,
      top: 'none',
      pants: 'ranger',
    })
    expect(samples.map((mesh) => mesh.geometry)).toEqual(originals)
  })

  it('trims the ranger waist only with plate armor and restores it on outfit changes', () => {
    const source = cloth()
    const positions = source.attributes.position
    for (let i = 0; i < positions.count; i++)
      positions.setY(i, 0.98 + positions.getY(i) * 0.18)
    const originalPositions = positions.array.slice()
    const pants = new THREE.SkinnedMesh(source)
    const parts = new Map([['pants_ranger', [pants]]])
    const outfit = { ...DEFAULT_MODULAR_OUTFIT, pants: 'ranger' } as const
    showModularOutfit([], parts, { ...outfit, top: 'plate' })
    const trimmed = pants.geometry
    expect(pants.visible).toBe(true)
    expect(area(trimmed)).toBeCloseTo(2 * (1.06 - 0.98))
    for (let i = 0; i < trimmed.attributes.position.count; i++) {
      expect(trimmed.attributes.position.getY(i)).toBeLessThanOrEqual(1.060001)
      const weights = trimmed.attributes.skinWeight
      expect(
        [0, 1, 2, 3].reduce((sum, j) => sum + weights.getComponent(i, j), 0)
      ).toBeCloseTo(1)
    }
    for (const top of [
      'none',
      'linen',
      'leather',
      'barbarian',
      'rogue',
      'caveman',
      'ranger',
    ] as const) {
      if (top === 'rogue' || top === 'caveman' || top === 'ranger')
        parts.set(`top_${top}`, [new THREE.SkinnedMesh()])
      showModularOutfit([], parts, { ...outfit, top })
      expect(pants.geometry).toBe(source)
      showModularOutfit([], parts, { ...outfit, top: 'plate' })
      expect(pants.geometry).toBe(trimmed)
    }
    showModularOutfit([], parts, { ...outfit, top: 'plate', pants: 'cloth' })
    expect(pants.visible).toBe(false)
    expect(pants.geometry).toBe(source)
    const replacement = new THREE.SkinnedMesh(source)
    parts.set('pants_ranger', [replacement])
    showModularOutfit([], parts, { ...outfit, top: 'plate' })
    expect(replacement.geometry).toBe(trimmed)
    expect(source.attributes.position.array).toEqual(originalPositions)
  })

  it.each([
    { side: -1, top: 'linen' },
    { side: 1, top: 'linen' },
    { side: -1, top: 'plate' },
    { side: 1, top: 'plate' },
  ] as const)(
    'shortens the $top sleeve to each glove opening and restores the bare arm (side: $side)',
    ({ side, top }) => {
      const source = cloth()
      const position = source.attributes.position
      for (let i = 0; i < position.count; i++)
        position.setXY(
          i,
          side * (0.4 + position.getX(i) * 0.1),
          0.95 + position.getY(i) * 0.4
        )
      const sleeve = new THREE.SkinnedMesh(source)
      const arm = new THREE.SkinnedMesh(source)
      sleeve.userData.region = top === 'linen' ? 'sleeves' : 'top_plate'
      arm.userData.region = 'forearms'
      const parts = new Map([[`top_${top}`, [sleeve]]])
      let previous = 0
      for (const gloves of ['barbarian', 'plate', 'leather', 'none'] as const) {
        showModularOutfit([arm], parts, {
          ...DEFAULT_MODULAR_OUTFIT,
          top,
          gloves,
        })
        const covered = Math.abs(area(sleeve.geometry))
        expect(covered).toBeGreaterThan(previous)
        previous = covered
        expect(arm.visible).toBe(gloves !== 'none')
        if (gloves !== 'none')
          expect(covered + Math.abs(area(arm.geometry))).toBeCloseTo(
            Math.abs(area(source))
          )
      }
      expect(sleeve.geometry).toBe(source)
      showModularOutfit([arm], parts, {
        ...DEFAULT_MODULAR_OUTFIT,
        top: 'none',
        gloves: 'barbarian',
      })
      expect(arm.visible).toBe(true)
      expect(arm.geometry).toBe(source)
    }
  )

  it.each([
    { side: -1, top: 'plate' },
    { side: 1, top: 'plate' },
    { side: -1, top: 'linen' },
    { side: 1, top: 'linen' },
    { side: -1, top: 'leather' },
    { side: 1, top: 'leather' },
  ] as const)(
    'hides covered skin and overlaps the $top sleeve with a ranger cuff (side: $side)',
    ({ side, top }) => {
      const source = cloth()
      const position = source.attributes.position
      for (let i = 0; i < position.count; i++)
        position.setXY(
          i,
          side * (0.4 + position.getX(i) * 0.1),
          0.95 + position.getY(i) * 0.4
        )
      const sleeve = new THREE.SkinnedMesh(source)
      sleeve.userData.region = 'sleeves'
      const arm = new THREE.SkinnedMesh(source)
      arm.userData.region = 'forearms'
      const hand = new THREE.SkinnedMesh()
      hand.userData.region = 'hands'
      const sleeveId = top === 'plate' ? 'top_plate' : 'top_linen'
      const gloves = [new THREE.SkinnedMesh()]
      const parts = new Map([
        [sleeveId, [sleeve]],
        ['gloves_ranger', gloves],
        ['top_leather', [new THREE.SkinnedMesh()]],
        ['top_ranger', [new THREE.SkinnedMesh()]],
      ])
      const outfit = {
        ...DEFAULT_MODULAR_OUTFIT,
        top,
        gloves: 'ranger' as const,
      }
      const body = [arm, hand]
      showModularOutfit(body, parts, { ...outfit, top: 'none' })
      const bareGeometry = arm.geometry
      showModularOutfit(body, parts, outfit)
      expect(arm.visible).toBe(false)
      expect(hand.visible).toBe(true)
      expect(sleeve.visible).toBe(true)
      const wrist = new THREE.Vector3(side * 0.445653, 0.993593, -0.045557)
      const axis = wrist
        .clone()
        .sub(new THREE.Vector3(side * 0.311339, 1.247962, -0.056036))
        .normalize()
      const points = sleeve.geometry.attributes.position
      const end = Math.max(
        ...Array.from({ length: points.count }, (_, i) =>
          new THREE.Vector3()
            .fromBufferAttribute(points, i)
            .sub(wrist)
            .dot(axis)
        )
      )
      expect(end + 0.208).toBeGreaterThan(0.015)
      showModularOutfit(body, parts, { ...outfit, top: 'ranger' })
      expect(arm.visible).toBe(true)
      expect(arm.geometry).toBe(bareGeometry)
      showModularOutfit(body, parts, { ...outfit, gloves: 'plate' })
      expect(arm.visible).toBe(true)
      parts.delete('gloves_ranger')
      showModularOutfit(body, parts, outfit)
      expect(arm.visible).toBe(false)
      expect(sleeve.geometry).toBe(source)
      parts.set('gloves_ranger', gloves)
      parts.delete(sleeveId)
      showModularOutfit(body, parts, outfit)
      expect(arm.visible).toBe(true)
      expect(arm.geometry).toBe(bareGeometry)
    }
  )

  it('preserves the lower torso when trimming one-piece plate armor', () => {
    const source = cloth()
    const position = source.attributes.position
    for (let i = 0; i < position.count; i++)
      position.setXY(i, position.getX(i) * 0.24, 0.96 + position.getY(i) * 0.2)
    const armor = new THREE.SkinnedMesh(source)
    const parts = new Map([['top_plate', [armor]]])
    showModularOutfit([], parts, {
      ...DEFAULT_MODULAR_OUTFIT,
      top: 'plate',
      gloves: 'barbarian',
    })
    expect(area(armor.geometry)).toBeCloseTo(area(source))
    expect(armor.geometry.attributes.position.array).toEqual(position.array)
  })

  it.each([false, true])(
    'cuts a straight boundary with continuous UVs and bone weights (nonindexed: %s)',
    (nonindexed) => {
      const source = nonindexed ? cloth().toNonIndexed() : cloth()
      const positions = source.attributes.position.array.slice()
      const trimmed = clipSkinnedGeometry(source, (p) => p.y - 0.5)
      expect(area(trimmed)).toBeCloseTo(1)
      const { position, uv, skinIndex, skinWeight } = trimmed.attributes
      let boundary = 0
      for (let i = 0; i < position.count; i++) {
        expect(position.getY(i)).toBeGreaterThanOrEqual(0.5)
        expect(uv.getX(i)).toBeCloseTo((position.getX(i) + 1) / 2)
        expect(uv.getY(i)).toBeCloseTo(position.getY(i))
        if (position.getY(i) !== 0.5) continue
        boundary++
        const weights = new Map<number, number>()
        for (let j = 0; j < 4; j++) {
          const joint = skinIndex.getComponent(i, j)
          weights.set(
            joint,
            (weights.get(joint) ?? 0) + skinWeight.getComponent(i, j)
          )
        }
        expect(weights.get(0)).toBeCloseTo(0.5)
        expect(weights.get(1)).toBeCloseTo(0.5)
      }
      expect(boundary).toBeGreaterThanOrEqual(3)
      expect(source.attributes.position.array).toEqual(positions)
    }
  )

  it('retains material groups and omits degenerate faces on an existing edge', () => {
    const source = cloth()
    source.addGroup(0, 3, 2)
    source.addGroup(3, 3, 4)
    const clipped = clipSkinnedGeometry(source, (p) => p.y - 0.5)
    expect(clipped.groups.map((group) => group.materialIndex)).toEqual([2, 4])
    expect(clipped.groups.reduce((sum, group) => sum + group.count, 0)).toBe(
      clipped.index!.count
    )
    expect(area(clipSkinnedGeometry(source, (p) => p.y))).toBeCloseTo(2)
    expect(clipSkinnedGeometry(source, (p) => p.y - 1).index!.count).toBe(0)
  })

  it('restores full clothes, isolates shared characters, and disposes cached cuts with the source', () => {
    const source = cloth()
    const first = new THREE.SkinnedMesh(source)
    const second = new THREE.SkinnedMesh(source)
    trimModularClothing(first, 'greaves')
    const cut = first.geometry
    expect(area(cut)).toBeCloseTo(1.08)
    expect(second.geometry).toBe(source)
    trimModularClothing(second, 'greaves')
    expect(second.geometry).toBe(cut)
    trimModularClothing(first)
    expect(first.geometry).toBe(source)
    expect(second.geometry).toBe(cut)
    trimModularClothing(first, 'greaves', true)
    expect(area(first.geometry) + area(cut)).toBeCloseTo(2)
    trimModularClothing(first, 'greaves')
    expect(first.geometry).toBe(cut)
    const dispose = vi.fn()
    cut.addEventListener('dispose', dispose)
    source.dispose()
    expect(dispose).toHaveBeenCalledOnce()
  })

  it.each(['cloth', 'plate'] as const)(
    'exposes only the lower leg with tall boots and restores %s pants when changing equipment',
    (style) => {
      const mesh = (region: string) => {
        const result = new THREE.SkinnedMesh(cloth())
        result.userData.region = region
        return result
      }
      const legs = mesh('legs'),
        ankles = mesh('ankles'),
        feet = mesh('feet')
      const clothPants = mesh('main'),
        platePants = mesh('pants_plate'),
        cuffs = mesh('cuffs'),
        tucked = mesh('tucked_cuffs')
      const pants = style === 'cloth' ? clothPants : platePants
      const outfit = { ...DEFAULT_MODULAR_OUTFIT, pants: style }
      const originals = [legs.geometry, ankles.geometry, pants.geometry]
      const body = [legs, ankles, feet]
      const parts = new Map([
        ['pants_cloth', [clothPants, cuffs, tucked]],
        ['pants_plate', [platePants]],
      ])
      showModularOutfit(body, parts, {
        ...outfit,
        boots: 'barbarian',
      })
      expect(body.every((part) => part.visible)).toBe(true)
      expect(cuffs.visible).toBe(false)
      expect(tucked.visible).toBe(false)
      expect(area(legs.geometry) + area(pants.geometry)).toBeCloseTo(2)
      for (const boots of ['none', 'leather', 'plate'] as const) {
        showModularOutfit(body, parts, { ...outfit, boots })
        expect([legs.geometry, ankles.geometry, pants.geometry]).toEqual(
          originals
        )
        expect(legs.visible).toBe(false)
        expect(ankles.visible).toBe(false)
        expect(cuffs.visible).toBe(style === 'cloth' && boots === 'none')
        expect(tucked.visible).toBe(style === 'cloth' && boots !== 'none')
      }
      showModularOutfit(body, parts, {
        ...DEFAULT_MODULAR_OUTFIT,
        pants: 'none',
        boots: 'barbarian',
      })
      expect(legs.visible).toBe(true)
      expect(legs.geometry).toBe(originals[0])
    }
  )
})
