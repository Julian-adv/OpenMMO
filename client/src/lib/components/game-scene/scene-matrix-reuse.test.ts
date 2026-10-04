import { describe, expect, it, vi } from 'vitest'
import { Bone, Group, PerspectiveCamera, Scene, Vector3 } from 'three'
import type { WebGPURenderer } from 'three/webgpu'
import { installSceneMatrixReuse } from './scene-matrix-reuse'

function fixture() {
  const scene = new Scene()
  const camera = new PerspectiveCamera()
  const group = new Group()
  const bone = new Bone()
  scene.add(group)
  group.add(bone)
  let depth = 0
  const positions: number[] = []
  const renderer = {
    render(target: Scene, view: PerspectiveCamera) {
      if (target.matrixWorldAutoUpdate) target.updateMatrixWorld()
      positions.push(new Vector3().setFromMatrixPosition(bone.matrixWorld).x)
      if (target === scene && depth === 0) {
        depth++
        try {
          for (let i = 0; i < 6; i++) this.render(target, view)
        } finally {
          depth--
        }
      }
    },
  }
  installSceneMatrixReuse(renderer as unknown as WebGPURenderer, scene)
  return { scene, camera, group, bone, renderer, positions }
}

describe('scene matrices shared by shadow passes', () => {
  it('uses the current pose on all six faces and updates again on the next render', () => {
    const { scene, camera, group, bone, renderer, positions } = fixture()
    const update = vi.spyOn(scene, 'updateMatrixWorld')
    group.position.x = 4
    bone.position.x = 2
    renderer.render(scene, camera)
    expect(positions).toEqual(Array(7).fill(6))
    expect(update).toHaveBeenCalledOnce()
    group.position.x = 10
    bone.position.x = 3
    renderer.render(scene, camera)
    expect(positions.slice(7)).toEqual(Array(7).fill(13))
    expect(update).toHaveBeenCalledTimes(2)
    expect(scene.matrixWorldAutoUpdate).toBe(true)
  })

  it('preserves manual matrix control and updates independent bake scenes', () => {
    const { scene, camera, group, renderer } = fixture()
    scene.matrixWorldAutoUpdate = false
    group.position.x = 9
    renderer.render(scene, camera)
    expect(group.matrixWorld.elements[12]).toBe(0)
    expect(scene.matrixWorldAutoUpdate).toBe(false)
    const bake = new Scene()
    const mesh = new Group()
    mesh.position.x = 7
    bake.add(mesh)
    renderer.render(bake, camera)
    expect(mesh.matrixWorld.elements[12]).toBe(7)
    expect(bake.matrixWorldAutoUpdate).toBe(true)
  })

  it('restores automatic updates after a failed shadow render and removes the hook', () => {
    const scene = new Scene()
    const original = vi.fn((target: Scene) => {
      expect(target.matrixWorldAutoUpdate).toBe(false)
      throw new Error('render failed')
    })
    const renderer = { render: original }
    const restore = installSceneMatrixReuse(
      renderer as unknown as WebGPURenderer,
      scene
    )
    expect(() => renderer.render(scene)).toThrow('render failed')
    expect(scene.matrixWorldAutoUpdate).toBe(true)
    restore()
    expect(renderer.render).toBe(original)
  })
})
