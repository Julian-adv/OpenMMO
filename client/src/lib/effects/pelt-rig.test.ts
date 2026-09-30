import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import {
  createPeltRig,
  disposePeltPhysics,
  updatePeltPhysics,
  type PeltPhysics,
} from './pelt-rig'

const config: PeltPhysics = {
  kind: 'plate',
  bone: 'Hips',
  pivot: [0, 1.055, 0.2],
  outward: [0, 0, 1],
  length: 0.365,
  colliders: [],
}

function fixture() {
  const root = new THREE.Group()
  const hip = new THREE.Bone()
  hip.name = 'Hips'
  hip.position.y = 1
  root.add(hip)
  const geometry = new THREE.PlaneGeometry(0.2, 0.365, 2, 5)
  geometry.translate(0, 0.8725, 0.2)
  const count = geometry.getAttribute('position').count
  geometry.setAttribute(
    'skinIndex',
    new THREE.Uint16BufferAttribute(new Uint16Array(count * 4), 4)
  )
  const weights = new Float32Array(count * 4)
  for (let i = 0; i < count; i++) weights[i * 4] = 1
  geometry.setAttribute(
    'skinWeight',
    new THREE.Float32BufferAttribute(weights, 4)
  )
  const mesh = new THREE.SkinnedMesh(geometry, new THREE.MeshStandardMaterial())
  root.add(mesh)
  root.updateMatrixWorld(true)
  mesh.bind(new THREE.Skeleton([hip]))
  mesh.userData.pelt_physics = config
  return { root, hip, mesh }
}

function softFixture(kind: 'strap' | 'fur' = 'strap') {
  const result = fixture()
  const geometry = result.mesh.geometry
  const count = geometry.getAttribute('position').count
  for (const [name, attr] of Object.entries(geometry.attributes)) {
    const values = Array.from(attr.array)
    geometry.setAttribute(
      name,
      new THREE.Float32BufferAttribute([...values, ...values], attr.itemSize)
    )
  }
  const pos = geometry.getAttribute('position') as THREE.BufferAttribute
  const normal = geometry.getAttribute('normal') as THREE.BufferAttribute
  for (let i = count; i < count * 2; i++) {
    pos.setZ(i, pos.getZ(i) - 0.003)
    normal.setZ(i, -1)
  }
  const front = Array.from(geometry.index!.array),
    back: number[] = []
  for (let i = 0; i < front.length; i += 3)
    back.push(front[i + 2] + count, front[i + 1] + count, front[i] + count)
  geometry.setIndex([...front, ...back])
  const settings: PeltPhysics = {
    ...config,
    kind,
    cloth: { columns: 3, rows: 6, pinned_rows: 2 },
  }
  result.mesh.userData.pelt_physics = settings
  return { ...result, settings }
}

function worldVertex(mesh: THREE.SkinnedMesh, i: number) {
  mesh.updateWorldMatrix(true, false)
  return mesh
    .getVertexPosition(i, new THREE.Vector3())
    .applyMatrix4(mesh.matrixWorld)
}

describe('pelt physics', () => {
  it('pins the hinge and lets a rigid plate trail a moving wearer, then settle', () => {
    const { root, mesh } = fixture()
    const rig = createPeltRig(mesh, config)
    rig.reset()
    root.position.z = -0.08
    rig.update(1 / 60)
    expect(worldVertex(mesh, 0).z).toBeCloseTo(0.12, 5)
    const last = mesh.geometry.getAttribute('position').count - 1
    expect(worldVertex(mesh, last).z).toBeGreaterThan(0.15)
    for (let i = 0; i < 300; i++) rig.update(1 / 60)
    expect(worldVertex(mesh, last).z).toBeCloseTo(0.12, 3)
  })

  it('uses fixed steps consistently at 30 and 60 fps', () => {
    const a = fixture(),
      b = fixture()
    const ra = createPeltRig(a.mesh, config),
      rb = createPeltRig(b.mesh, config)
    ra.reset()
    rb.reset()
    a.root.position.z = b.root.position.z = -0.04
    for (let i = 0; i < 30; i++) ra.update(1 / 30)
    for (let i = 0; i < 60; i++) rb.update(1 / 60)
    expect(Array.from(a.mesh.geometry.getAttribute('position').array)).toEqual(
      Array.from(b.mesh.geometry.getAttribute('position').array)
    )
  })

  it('rotates the whole plate outside a collider that follows an animated, transformed body', () => {
    const { root, hip, mesh } = fixture()
    const collider = {
      bone: 'Hips',
      center: [0, 0.85, 0.2] as [number, number, number],
      radii: [0.1, 0.1, 0.1] as [number, number, number],
    }
    const rig = createPeltRig(mesh, { ...config, colliders: [collider] })
    root.position.set(3, 2, -1)
    root.rotation.y = 0.7
    rig.reset()
    hip.position.y += 0.03
    rig.update(1 / 60)
    root.updateMatrixWorld(true)
    const center = new THREE.Vector3(0, -0.15, 0.2).applyMatrix4(
      hip.matrixWorld
    )
    for (let i = 3; i < mesh.geometry.getAttribute('position').count; i++)
      expect(worldVertex(mesh, i).distanceTo(center)).toBeGreaterThanOrEqual(
        0.0999
      )
  })

  it('resets after teleport, hidden equipment and long frames without changing cached geometry', () => {
    const { root, mesh } = fixture()
    const source = mesh.geometry
    const positions = Array.from(source.getAttribute('position').array)
    updatePeltPhysics(root, 1 / 60)
    const owned = mesh.geometry
    expect(owned).not.toBe(source)
    root.position.set(10, 5, -4)
    updatePeltPhysics(root, 1 / 60)
    mesh.visible = false
    updatePeltPhysics(root, 1 / 60)
    mesh.visible = true
    updatePeltPhysics(root, 5)
    expect(
      Array.from(owned.getAttribute('position').array).every(Number.isFinite)
    ).toBe(true)
    expect(
      worldVertex(mesh, 0).distanceTo(new THREE.Vector3(9.9, 6.055, -3.8))
    ).toBeLessThan(1e-5)
    expect(Array.from(source.getAttribute('position').array)).toEqual(positions)
    let disposed = 0
    owned.addEventListener('dispose', () => disposed++)
    disposePeltPhysics(root)
    disposePeltPhysics(root)
    expect(disposed).toBe(1)
    expect(mesh.geometry).toBe(source)
  })

  it('keeps simulations independent when characters share a cached geometry', () => {
    const a = fixture(),
      b = fixture()
    b.mesh.geometry = a.mesh.geometry
    updatePeltPhysics(a.root, 1 / 60)
    updatePeltPhysics(b.root, 1 / 60)
    const before = Array.from(b.mesh.geometry.getAttribute('position').array)
    a.root.position.z = -0.08
    updatePeltPhysics(a.root, 1 / 60)
    expect(Array.from(b.mesh.geometry.getAttribute('position').array)).toEqual(
      before
    )
    expect(a.mesh.geometry).not.toBe(b.mesh.geometry)
  })

  it('keeps a triangle spanning a collider outside even when its corners start outside', () => {
    const { mesh } = fixture()
    mesh.geometry.setAttribute(
      'position',
      new THREE.Float32BufferAttribute(
        [-0.14, 0.98, 0.2, 0.14, 0.98, 0.2, 0, 0.68, 0.2],
        3
      )
    )
    mesh.geometry.setIndex([0, 1, 2])
    const collider = {
      bone: 'Hips',
      center: [0, 0.88, 0.2] as [number, number, number],
      radii: [0.08, 0.08, 0.08] as [number, number, number],
    }
    const rig = createPeltRig(mesh, { ...config, colliders: [collider] })
    rig.reset()
    const center = new THREE.Vector3()
    for (let i = 0; i < 3; i++) center.add(worldVertex(mesh, i))
    center.divideScalar(3)
    expect(
      center.distanceTo(new THREE.Vector3(...collider.center))
    ).toBeGreaterThanOrEqual(0.0799)
  })
  it('preserves plate distances, normals and a motionless hinge through repeated collisions', () => {
    const { root, hip, mesh } = fixture()
    const rig = createPeltRig(mesh, {
      ...config,
      colliders: [
        { bone: 'Hips', center: [0, 0.85, 0.2], radii: [0.1, 0.1, 0.1] },
      ],
    })
    rig.reset()
    const pinned = worldVertex(mesh, 0)
    const distance = worldVertex(mesh, 0).distanceTo(worldVertex(mesh, 17))
    for (let i = 0; i < 180; i++) {
      root.position.z = Math.sin(i / 12) * 0.05
      hip.rotation.x = Math.sin(i / 18) * 0.2
      rig.update(1 / 60)
      expect(
        worldVertex(mesh, 0).distanceTo(worldVertex(mesh, 17))
      ).toBeCloseTo(distance, 5)
    }
    root.position.z = hip.rotation.x = 0
    for (let i = 0; i < 300; i++) rig.update(1 / 60)
    expect(worldVertex(mesh, 0).distanceTo(pinned)).toBeLessThan(1e-6)
    const settled = Array.from(mesh.geometry.getAttribute('position').array)
    for (let i = 0; i < 120; i++) rig.update(1 / 60)
    expect(Array.from(mesh.geometry.getAttribute('position').array)).toEqual(
      settled
    )
  })

  it('lets a leather strap swing independently of the stiffer pelt', () => {
    const a = fixture(),
      b = fixture()
    const plate = createPeltRig(a.mesh, config)
    const strap = createPeltRig(b.mesh, { ...config, kind: 'strap' })
    plate.reset()
    strap.reset()
    a.root.position.z = b.root.position.z = -0.08
    for (let i = 0; i < 10; i++) {
      plate.update(1 / 60)
      strap.update(1 / 60)
    }
    expect(worldVertex(b.mesh, 17).z).toBeGreaterThan(
      worldVertex(a.mesh, 17).z + 0.005
    )
    expect(
      worldVertex(a.mesh, 0).distanceTo(worldVertex(b.mesh, 0))
    ).toBeLessThan(1e-6)
  })
})

describe('flexible belt attachments', () => {
  it.each(['strap', 'fur'] as const)(
    'keeps both top rows of %s fixed while the lower rows bend',
    (kind) => {
      const { root, hip, mesh, settings } = softFixture(kind)
      const source = mesh.geometry.clone()
      const rig = createPeltRig(mesh, settings)
      rig.reset()
      root.position.z = -0.06
      hip.rotation.y = 0.15
      for (let i = 0; i < 12; i++) rig.update(1 / 60)
      const positions = mesh.geometry.getAttribute('position')
      const original = source.getAttribute('position')
      for (const start of [0, 18])
        for (let i = start; i < start + 6; i++) {
          expect([
            positions.getX(i),
            positions.getY(i),
            positions.getZ(i),
          ]).toEqual([original.getX(i), original.getY(i), original.getZ(i)])
        }
      const top = new THREE.Vector3().fromBufferAttribute(positions, 4)
      const middle = new THREE.Vector3().fromBufferAttribute(positions, 10)
      const hem = new THREE.Vector3().fromBufferAttribute(positions, 16)
      const straight = new THREE.Line3(top, hem).closestPointToPoint(
        middle,
        true,
        new THREE.Vector3()
      )
      expect(middle.distanceTo(straight)).toBeGreaterThan(0.003)
      for (let i = 6; i < 18; i++) {
        const edge = new THREE.Vector3()
          .fromBufferAttribute(positions, i)
          .distanceTo(new THREE.Vector3().fromBufferAttribute(positions, i - 3))
        expect(edge / 0.073).toBeLessThanOrEqual(1.0401)
        expect(edge / 0.073).toBeGreaterThan(0.9)
      }
    }
  )

  it('gives the same flexible result at 30 and 60 fps', () => {
    const a = softFixture(),
      b = softFixture()
    const ra = createPeltRig(a.mesh, a.settings),
      rb = createPeltRig(b.mesh, b.settings)
    ra.reset()
    rb.reset()
    a.root.position.z = b.root.position.z = -0.04
    for (let i = 0; i < 30; i++) ra.update(1 / 30)
    for (let i = 0; i < 60; i++) rb.update(1 / 60)
    expect(Array.from(a.mesh.geometry.getAttribute('position').array)).toEqual(
      Array.from(b.mesh.geometry.getAttribute('position').array)
    )
  })

  it('settles against a body contact without moving the attachment or its cached geometry', () => {
    const { root, mesh, settings } = softFixture('fur')
    settings.colliders = [
      { bone: 'Hips', center: [0, 0.82, 0.2], radii: [0.11, 0.09, 0.08] },
    ]
    const source = mesh.geometry
    const original = Array.from(source.getAttribute('position').array)
    updatePeltPhysics(root, 1 / 60)
    for (let i = 0; i < 600; i++) updatePeltPhysics(root, 1 / 60)
    const positions = mesh.geometry.getAttribute('position')
    const before = Array.from(positions.array)
    for (let i = 0; i < 120; i++) updatePeltPhysics(root, 1 / 60)
    const movement = Array.from(positions.array).map((v, i) =>
      Math.abs(v - before[i])
    )
    expect(Math.max(...movement)).toBeLessThan(0.0001)
    for (let i = 6; i < 18; i++) {
      const point = worldVertex(mesh, i)
        .sub(new THREE.Vector3(0, 0.82, 0.2))
        .divide(new THREE.Vector3(0.11, 0.09, 0.08))
      expect(point.lengthSq()).toBeGreaterThanOrEqual(1)
    }
    expect(Array.from(source.getAttribute('position').array)).toEqual(original)
    root.position.x = 5
    updatePeltPhysics(root, 1)
    expect(Array.from(positions.array).every(Number.isFinite)).toBe(true)
    disposePeltPhysics(root)
    expect(mesh.geometry).toBe(source)
  })
})
