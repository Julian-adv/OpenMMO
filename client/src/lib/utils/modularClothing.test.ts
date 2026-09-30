import * as THREE from 'three'
import { describe, expect, it, vi } from 'vitest'
import { clipSkinnedGeometry, trimModularClothing } from './modularClothing'
import { DEFAULT_MODULAR_OUTFIT, showModularOutfit } from './modularCharacter'

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

describe('modular clothing cuts', () => {
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
