import { Color, Group, MeshStandardMaterial, SkinnedMesh, Texture } from 'three'
import { StandardNodeLibrary, type Node, type NodeMaterial } from 'three/webgpu'
import { uniform, vec4 } from 'three/tsl'
import { describe, expect, it } from 'vitest'
import {
  applyAppearanceColors,
  disposeAppearanceColors,
  type Tints,
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

/** The colors a mesh's shared tinted material reads for it when drawn. */
const tints = (mesh: SkinnedMesh) => mesh.userData.appearanceTints as Tints

describe('character appearance colors', () => {
  it('restores original eyes independently of hair dye and keeps the iris material', () => {
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
    applyAppearanceColors(root, [head], {
      ...appearance,
      eye_color_original: true,
    })
    expect(tints(head).eyeTint).toBe(0)
    expect(head.material).toBe(material)
    expect(material.version).toBe(version)
    applyAppearanceColors(root, [head], {
      ...appearance,
      eye_color_original: false,
    })
    expect(tints(head).eyeTint).toBe(1)
    disposeAppearanceColors(root)
    expect(head.material).toBe(source)
    expect(head.userData.appearanceTints).toBeUndefined()
  })

  it('restores original male and female hair without affecting another character', () => {
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
    expect(tints(male).hairTint).toBe(1)
    applyAppearanceColors(root, [male, female], {
      ...appearance,
      hair_color_original: true,
    })
    expect(male.material).toBe(material)
    expect(material.version).toBe(version)
    expect(tints(male).hairTint).toBe(0)
    expect(tints(female).hairTint).toBe(0)
    expect(other.material).toBe(source)
    applyAppearanceColors(root, [male, female], {
      ...appearance,
      hair_color: '#00ff00',
      hair_color_original: false,
    })
    expect(tints(male).hairTint).toBe(1)
    expect(tints(male).hair.getHexString()).toBe('00ff00')
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
    expect(head.material).not.toBe(skin)
    expect(rangerHair.material).not.toBe(hair)
    expect(tints(head).eyes.getHexString()).toBe('0000ff')
    expect(tints(rangerHair).hair.getHexString()).toBe('ff0000')
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
    expect(colorsIn(tintedSkin.colorNode!)).toContain('00ff00')
    expect(tints(skin).eyes.getHexString()).toBe('0000ff')
    expect(tints(ponytail).hair.getHexString()).toBe('ff0000')
    disposeAppearanceColors(root)
    expect(skin.material).toBe(nodeBody)
    expect(ponytail.material).toBe(hair)
  })

  it('shares one tinted material between characters and keeps their colors apart', () => {
    const skin = new MeshStandardMaterial({ map: new Texture() })
    const hair = new MeshStandardMaterial({ map: new Texture() })
    const character = () => {
      const root = new Group()
      const head = new SkinnedMesh(undefined, skin)
      head.userData.region = 'head'
      const crop = new SkinnedMesh(undefined, hair)
      crop.userData.part_id = 'hair_crop'
      const torso = new SkinnedMesh(undefined, skin)
      root.add(head, crop, torso)
      return { root, head, crop, torso }
    }
    const a = character()
    const b = character()
    const bone = hair.clone()
    bone.userData.appearance_color_fixed = true
    const wavy = new SkinnedMesh(undefined, [hair, bone])
    wavy.userData.part_id = 'hair_wavy_bone'
    a.root.add(wavy)
    applyAppearanceColors(a.root, [a.head, a.crop, a.torso, wavy], {
      face: 'default',
      hair: 'crop',
      hair_color: '#ff0000',
      eye_color: '#0000ff',
    })
    applyAppearanceColors(b.root, [b.head, b.crop, b.torso], {
      face: 'default',
      hair: 'crop',
      hair_color: '#00ff00',
      eye_color: '#78518d',
    })
    expect(a.crop.material).not.toBe(hair)
    expect(b.crop.material).toBe(a.crop.material)
    expect(b.head.material).toBe(a.head.material)
    expect(a.torso.material).toBe(skin)
    expect(wavy.material[1]).toBe(bone)
    expect(tints(a.crop).hair.getHexString()).toBe('ff0000')
    expect(tints(b.crop).hair.getHexString()).toBe('00ff00')
    disposeAppearanceColors(a.root)
    expect(a.crop.material).toBe(hair)
    expect(a.head.material).toBe(skin)
    expect(b.crop.material).not.toBe(hair)
  })
})
