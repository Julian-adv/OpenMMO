import { Color, Group, MeshStandardMaterial, SkinnedMesh, Texture } from 'three'
import { StandardNodeLibrary, type Node, type NodeMaterial } from 'three/webgpu'
import { uniform, vec4 } from 'three/tsl'
import { describe, expect, it, vi } from 'vitest'
import {
  applyAppearanceColors,
  disposeAppearanceColors,
} from './character-appearance-colors'

const library = new StandardNodeLibrary()

function colorsIn(node: Node): string[] {
  const values: string[] = []
  node.traverse((child) => {
    if ('value' in child && child.value instanceof Color)
      values.push(child.value.getHexString())
  })
  return values
}

describe('character appearance colors', () => {
  it('restores original eyes independently of hair dye and reuses the iris material', () => {
    const source = new MeshStandardMaterial({ map: new Texture() })
    const root = new Group()
    const head = new SkinnedMesh(undefined, source)
    head.userData.region = 'head'
    root.add(head)
    const appearance = {
      face: 'default',
      hair: 'crop',
      hair_color: '#ff0000',
      eye_color: '#0000ff',
    } as const
    applyAppearanceColors(root, [head], appearance)
    const material = head.material as unknown as NodeMaterial
    const version = material.version
    const weights: number[] = []
    applyAppearanceColors(root, [head], {
      ...appearance,
      eye_color_original: true,
    })
    material.colorNode!.traverse((node) => {
      if (
        'isUniformNode' in node &&
        'value' in node &&
        typeof node.value === 'number'
      )
        weights.push(node.value)
    })
    expect(weights).toContain(0)
    expect(head.material).toBe(material)
    expect(material.version).toBe(version)
    applyAppearanceColors(root, [head], {
      ...appearance,
      eye_color_original: false,
    })
    const dyed: number[] = []
    material.colorNode!.traverse((node) => {
      if (
        'isUniformNode' in node &&
        'value' in node &&
        typeof node.value === 'number'
      )
        dyed.push(node.value)
    })
    expect(dyed).toContain(1)
    disposeAppearanceColors(root)
    expect(head.material).toBe(source)
  })

  it('restores original male and female hair without replacing materials or affecting another character', () => {
    const source = new MeshStandardMaterial({ map: new Texture() })
    const root = new Group()
    const male = new SkinnedMesh(undefined, source)
    male.userData.part_id = 'hair_crop'
    const female = new SkinnedMesh(undefined, source.clone())
    female.userData.female_part = 'hair'
    const other = new SkinnedMesh(undefined, source)
    root.add(male, female)
    const appearance = {
      face: 'default',
      hair: 'crop',
      hair_color: '#ff0000',
    } as const
    applyAppearanceColors(root, [male, female], appearance)
    const material = male.material as unknown as NodeMaterial
    const version = material.version
    const tintWeights = () => {
      const weights: number[] = []
      material.colorNode!.traverse((node) => {
        if (
          'isUniformNode' in node &&
          'value' in node &&
          typeof node.value === 'number'
        )
          weights.push(node.value)
      })
      return weights
    }
    expect(tintWeights()).toContain(1)
    applyAppearanceColors(root, [male, female], {
      ...appearance,
      hair_color_original: true,
    })
    expect(male.material).toBe(material)
    expect(material.version).toBe(version)
    expect(tintWeights()).toContain(0)
    expect(other.material).toBe(source)
    applyAppearanceColors(root, [male, female], {
      ...appearance,
      hair_color: '#00ff00',
      hair_color_original: false,
    })
    expect(tintWeights()).toContain(1)
    expect(colorsIn(material.colorNode!)).toContain('00ff00')
    disposeAppearanceColors(root)
    expect(male.material).toBe(source)
  })

  it('tints ranger hair and irises', () => {
    const skin = new MeshStandardMaterial({ map: new Texture() })
    const hair = new MeshStandardMaterial({ map: new Texture() })
    const root = new Group()
    const head = new SkinnedMesh(undefined, skin)
    head.userData.region = 'head'
    const rangerHair = new SkinnedMesh(undefined, hair)
    rangerHair.userData.part_id = 'hair_ranger'
    root.add(head, rangerHair)
    applyAppearanceColors(root, [head, rangerHair], {
      face: 'ranger',
      hair: 'ranger',
      hair_color: '#ff0000',
      eye_color: '#0000ff',
    })
    const tintedSkin = head.material as unknown as NodeMaterial
    const tintedHair = rangerHair.material as unknown as NodeMaterial
    expect(colorsIn(tintedSkin.colorNode!)).toContain('0000ff')
    expect(colorsIn(tintedHair.colorNode!)).toContain('ff0000')
    disposeAppearanceColors(root)
    expect(head.material).toBe(skin)
    expect(rangerHair.material).toBe(hair)
  })

  it('tints female hair and irises on top of the body node colors', () => {
    const body = new MeshStandardMaterial({ map: new Texture() })
    const nodeBody = library.fromMaterial(body)
    nodeBody.colorNode = vec4(uniform(new Color('#00ff00')), 1)
    const hair = new MeshStandardMaterial({ map: new Texture() })
    const root = new Group()
    const skin = new SkinnedMesh(undefined, nodeBody)
    skin.userData.female_body = true
    const ponytail = new SkinnedMesh(undefined, hair)
    ponytail.userData.female_part = 'ranger_hair'
    root.add(skin, ponytail)
    applyAppearanceColors(root, [skin, ponytail], {
      face: 'default',
      hair: 'ponytail',
      hair_color: '#ff0000',
      eye_color: '#0000ff',
    })
    const tintedSkin = skin.material as unknown as NodeMaterial
    expect(colorsIn(tintedSkin.colorNode!)).toEqual(
      expect.arrayContaining(['0000ff', '00ff00'])
    )
    expect(
      colorsIn((ponytail.material as unknown as NodeMaterial).colorNode!)
    ).toContain('ff0000')
    disposeAppearanceColors(root)
    expect(skin.material).toBe(nodeBody)
    expect(ponytail.material).toBe(hair)
  })

  it('isolates per-character colors, reuses uniforms and restores shared materials', () => {
    const skin = new MeshStandardMaterial({ map: new Texture() })
    const hair = new MeshStandardMaterial({ map: new Texture() })
    const root = new Group()
    const head = new SkinnedMesh(undefined, skin)
    head.userData.region = 'head'
    const crop = new SkinnedMesh(undefined, hair)
    crop.userData.part_id = 'hair_crop'
    const torso = new SkinnedMesh(undefined, skin)
    root.add(head, crop, torso)
    const other = new SkinnedMesh(undefined, hair)
    const bone = hair.clone()
    bone.userData.appearance_color_fixed = true
    const wavy = new SkinnedMesh(undefined, [hair, bone])
    wavy.userData.part_id = 'hair_wavy_bone'
    root.add(wavy)
    applyAppearanceColors(root, [head, crop, torso, wavy], {
      face: 'default',
      hair: 'crop',
      hair_color: '#ff0000',
      eye_color: '#0000ff',
    })
    const tintedHair = crop.material as unknown as NodeMaterial
    const tintedSkin = head.material as unknown as NodeMaterial
    const version = tintedHair.version
    expect(tintedHair).not.toBe(hair)
    expect(tintedSkin).not.toBe(skin)
    expect(torso.material).toBe(skin)
    expect(other.material).toBe(hair)
    expect(wavy.material[1]).toBe(bone)
    expect(colorsIn(tintedHair.colorNode!)).toContain('ff0000')
    expect(colorsIn(tintedSkin.colorNode!)).toContain('0000ff')
    applyAppearanceColors(root, [head, crop, torso, wavy], {
      face: 'default',
      hair: 'crop',
      hair_color: '#00ff00',
      eye_color: '#78518d',
    })
    expect(crop.material).toBe(tintedHair)
    expect(head.material).toBe(tintedSkin)
    expect(tintedHair.version).toBe(version)
    expect(colorsIn(tintedHair.colorNode!)).toContain('00ff00')
    const released = vi.fn()
    tintedHair.addEventListener('dispose', released)
    disposeAppearanceColors(root)
    expect(crop.material).toBe(hair)
    expect(head.material).toBe(skin)
    expect(released).toHaveBeenCalledOnce()
  })
})
