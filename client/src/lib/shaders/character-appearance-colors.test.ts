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
