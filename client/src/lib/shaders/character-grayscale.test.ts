import { Group, Mesh, MeshPhysicalMaterial, SkinnedMesh, Texture } from 'three'
import { MeshBasicNodeMaterial, type NodeMaterial } from 'three/webgpu'
import { vec3, vec4 } from 'three/tsl'
import { describe, expect, it, vi } from 'vitest'
import { CharacterGrayscale } from './character-grayscale'

describe('character preview grayscale', () => {
  it('isolates shared body and equipment materials and restores them on disposal', () => {
    const source = new MeshPhysicalMaterial({
      color: 0xff5533,
      map: new Texture(),
      transparent: true,
      opacity: 0.7,
      roughness: 0.4,
    })
    const root = new Group()
    const body = new SkinnedMesh(undefined, source)
    const equipment = new Mesh(undefined, [source, source])
    const otherCharacter = new Mesh(undefined, source)
    root.add(body, equipment)
    const effect = new CharacterGrayscale()
    effect.setEnabled(true)
    effect.sync(root)

    const converted = body.material as MeshPhysicalMaterial & NodeMaterial
    expect(converted).not.toBe(source)
    expect(converted.map).toBe(source.map)
    expect(converted.opacity).toBe(0.7)
    expect(converted.roughness).toBe(0.4)
    expect(converted.transparent).toBe(true)
    expect(converted.outputNode).not.toBeNull()
    expect(equipment.material).toEqual([converted, converted])
    expect(otherCharacter.material).toBe(source)
    expect(source.color.getHex()).toBe(0xff5533)

    const released = vi.fn()
    const sourceReleased = vi.fn()
    converted.addEventListener('dispose', released)
    source.addEventListener('dispose', sourceReleased)
    effect.dispose()
    expect(body.material).toBe(source)
    expect(equipment.material).toEqual([source, source])
    expect(released).toHaveBeenCalledOnce()
    expect(sourceReleased).not.toHaveBeenCalled()
  })

  it('handles delayed equipment and cape texture changes while releasing detached materials', () => {
    const root = new Group()
    const effect = new CharacterGrayscale()
    effect.setEnabled(true)
    effect.sync(root)
    const source = new MeshBasicNodeMaterial()
    source.colorNode = vec3(1, 0, 0)
    source.outputNode = vec4(0.2, 0.4, 0.8, 0.5)
    const originalOutput = source.outputNode
    const cape = new Mesh(undefined, source)
    root.add(cape)
    effect.sync(root)
    const converted = cape.material as NodeMaterial
    expect(converted.outputNode).not.toBe(originalOutput)
    expect(source.outputNode).toBe(originalOutput)
    const newPrint = vec3(0, 1, 0)
    source.colorNode = newPrint
    source.needsUpdate = true
    effect.sync(root)
    expect(cape.material).toBe(converted)
    expect(converted.colorNode).toBe(newPrint)
    expect(source.outputNode).toBe(originalOutput)

    const released = vi.fn()
    converted.addEventListener('dispose', released)
    root.remove(cape)
    effect.sync(root)
    expect(cape.material).toBe(source)
    expect(released).toHaveBeenCalledOnce()
    effect.dispose()
    expect(released).toHaveBeenCalledOnce()
  })

  it('lets armor replace restored materials and keeps cancellation free of shader rebuilds', () => {
    const skin = new MeshPhysicalMaterial({ color: 0xffbb88 })
    const armor = new MeshPhysicalMaterial({ color: 0x1133aa })
    const mesh = new Mesh(undefined, skin)
    const effect = new CharacterGrayscale()
    effect.sync(mesh)
    expect(mesh.material).toBe(skin)
    effect.setEnabled(true)
    effect.sync(mesh)
    effect.restore(mesh)
    expect(mesh.material).toBe(skin)
    mesh.material = armor
    effect.sync(mesh)
    const converted = mesh.material as MeshPhysicalMaterial & NodeMaterial
    const output = converted.outputNode
    const version = converted.version
    effect.setEnabled(false)
    effect.sync(mesh)
    expect(mesh.material).toBe(converted)
    expect(converted.outputNode).toBe(output)
    expect(converted.version).toBe(version)
    effect.dispose()
    expect(mesh.material).toBe(armor)
  })
})
