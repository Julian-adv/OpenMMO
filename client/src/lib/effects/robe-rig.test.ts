import * as THREE from 'three'
import { describe, expect, it, vi } from 'vitest'
import { createRobeRig, robeBaseSkeleton, type RobePhysics } from './robe-rig'

const config: RobePhysics = {
  waist_height_m: 1.08,
  hem_height_m: 0.64,
  center_z_m: -0.035,
  waist_radii_m: [0.18, 0.14],
  vertices: [2, 3, 4, 5],
}

function fixture(shared?: THREE.BufferGeometry) {
  const root = new THREE.Group()
  const hip = new THREE.Bone()
  hip.name = 'Hips'
  hip.position.y = 1.08
  root.add(hip)
  const bones = [hip]
  const upperLegs = ['Left', 'Right'].map((side, i) => {
    const upper = new THREE.Bone()
    upper.name = side + 'UpLeg'
    upper.position.set(i === 0 ? 0.1 : -0.1, -0.06, 0)
    hip.add(upper)
    const knee = new THREE.Bone()
    knee.name = side + 'Leg'
    knee.position.y = -0.46
    upper.add(knee)
    bones.push(upper, knee)
    return upper
  })
  const geometry = shared ?? new THREE.BufferGeometry()
  if (!shared) {
    geometry.setAttribute(
      'position',
      new THREE.Float32BufferAttribute(
        [
          -0.1, 1.3, 0.15, 0.1, 1.3, 0.15, -0.1, 1.08, 0.15, 0.1, 1.08, 0.15,
          -0.1, 0.64, 0.2, 0.1, 0.64, 0.2,
        ],
        3
      )
    )
    geometry.setAttribute(
      'skinIndex',
      new THREE.Uint16BufferAttribute(new Uint16Array(24), 4)
    )
    const weights = new Float32Array(24)
    for (let i = 0; i < 6; i++) weights[i * 4] = 1
    geometry.setAttribute(
      'skinWeight',
      new THREE.Float32BufferAttribute(weights, 4)
    )
    geometry.setIndex([0, 2, 1, 1, 2, 3, 2, 4, 3, 3, 4, 5])
  }
  const mesh = new THREE.SkinnedMesh(geometry, new THREE.MeshBasicMaterial())
  root.add(mesh)
  root.updateMatrixWorld(true)
  mesh.bind(new THREE.Skeleton(bones))
  const skeleton = mesh.skeleton
  const rig = createRobeRig(mesh, config)
  const frame = (dt = 1 / 30) => {
    root.updateMatrixWorld(true)
    rig.update(dt)
  }
  const vertex = (i: number) =>
    mesh
      .getVertexPosition(i, new THREE.Vector3())
      .applyMatrix4(mesh.matrixWorld)
  frame()
  return { root, mesh, hip, upperLegs, geometry, skeleton, rig, frame, vertex }
}

describe('lightweight robe motion', () => {
  it('releases render bindings when the skeleton size changes without disposing shared assets', () => {
    const f = fixture()
    f.rig.dispose()
    const releaseBindings = vi.fn()
    const disposeMaterial = vi.fn()
    const disposeGeometry = vi.fn()
    f.mesh.addEventListener('dispose', releaseBindings)
    f.mesh.material.addEventListener('dispose', disposeMaterial)
    f.geometry.addEventListener('dispose', disposeGeometry)
    const originalBytes = f.mesh.skeleton.boneMatrices!.byteLength

    const rig = createRobeRig(f.mesh, config)
    expect(f.mesh.skeleton.boneMatrices!.byteLength).toBe(originalBytes + 8 * 64)
    expect(releaseBindings).toHaveBeenCalledTimes(1)

    rig.update(1 / 30)
    expect(releaseBindings).toHaveBeenCalledTimes(1)

    rig.dispose()
    expect(f.mesh.skeleton.boneMatrices!.byteLength).toBe(originalBytes)
    expect(releaseBindings).toHaveBeenCalledTimes(2)
    expect(disposeMaterial).not.toHaveBeenCalled()
    expect(disposeGeometry).not.toHaveBeenCalled()
    expect(f.mesh.parent).toBe(f.root)
  })

  it('lifts the hem ahead of a raised thigh while preserving the waist and upper body', () => {
    const f = fixture()
    const before = [f.vertex(0), f.vertex(2), f.vertex(3), f.vertex(5)]
    f.upperLegs[0].rotation.x = -1
    f.frame()
    for (const i of [0, 2, 3])
      expect(f.vertex(i).distanceTo(before[i === 0 ? 0 : i - 1])).toBeLessThan(
        1e-6
      )
    expect(f.vertex(5).z).toBeGreaterThan(before[3].z + 0.15)
    expect(f.vertex(5).y).toBeGreaterThan(before[3].y + 0.05)
    expect(f.mesh.skeleton.bones).toHaveLength(f.skeleton.bones.length + 8)
    expect(robeBaseSkeleton(f.mesh)).toBe(f.skeleton)
    f.rig.dispose()
  })

  it('shares static geometry while keeping each character motion independent', () => {
    const a = fixture(),
      b = fixture(a.geometry)
    expect(a.mesh.geometry).toBe(b.mesh.geometry)
    const dispose = vi.fn()
    b.mesh.geometry.addEventListener('dispose', dispose)
    a.upperLegs[0].rotation.x = -1
    a.frame()
    b.frame()
    expect(a.vertex(5).distanceTo(b.vertex(5))).toBeGreaterThan(0.1)
    expect(Array.from(b.mesh.geometry.attributes.position.array)).toEqual(
      Array.from(b.geometry.attributes.position.array)
    )
    a.rig.dispose()
    expect(dispose).not.toHaveBeenCalled()
    expect(a.mesh.geometry).toBe(a.geometry)
    expect(a.mesh.skeleton).toBe(a.skeleton)
    b.rig.dispose()
    b.rig.dispose()
    expect(dispose).toHaveBeenCalledTimes(1)
  })

  it('skips spring updates between 30 Hz ticks and settles after leg motion', () => {
    const f = fixture()
    f.upperLegs[0].rotation.x = -1
    f.frame(1 / 60)
    expect(f.vertex(5).z).toBeCloseTo(0.2, 5)
    f.frame(1 / 60)
    expect(f.vertex(5).z).toBeGreaterThan(0.35)
    f.upperLegs[0].rotation.x = 0
    f.frame()
    expect(f.vertex(5).z).toBeGreaterThan(0.3)
    for (let i = 0; i < 180; i++) f.frame()
    expect(f.vertex(5).z).toBeCloseTo(0.2, 3)
    f.rig.dispose()
  })

  it('resets hidden and teleported characters without accumulated motion', () => {
    const f = fixture()
    f.mesh.visible = false
    f.upperLegs[0].rotation.x = -1
    f.frame(5)
    f.mesh.visible = true
    f.frame()
    expect(f.vertex(5).z).toBeGreaterThan(0.35)
    f.upperLegs[0].rotation.x = 0
    f.root.position.x = 100
    f.frame()
    expect(f.vertex(5).z).toBeCloseTo(0.2, 5)
    expect(f.mesh.frustumCulled).toBe(true)
    f.rig.dispose()
  })
})
