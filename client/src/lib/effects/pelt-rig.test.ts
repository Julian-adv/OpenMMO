import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import {
  createPeltRig,
  disposePeltPhysics,
  resetPeltPhysics,
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
  it.each(['strap', 'fur'] as const)(
    'keeps %s cloth pinned and unstretched at a 30 Hz physics step',
    (kind) => {
      const { root, hip, mesh, settings } = softFixture(kind)
      const rig = createPeltRig(mesh, settings)
      rig.reset()
      for (let i = 0; i < 120; i++) {
        root.position.z = i / 30
        root.rotation.y = Math.sin(i / 30) * 0.6
        hip.position.y = 1 + Math.sin(i) * 0.03
        rig.update(
          1 / 30,
          { windDirX: 1, windDirZ: 0, windStrength: 1 },
          1 / 30
        )
        const positions = mesh.geometry.getAttribute('position')
        expect(Array.from(positions.array).every(Number.isFinite)).toBe(true)
        const rest = root.worldToLocal(worldVertex(mesh, 1))
        expect(rest.y).toBeCloseTo(1.055 + Math.sin(i) * 0.03, 5)
        for (let row = 2; row < 6; row++)
          for (let col = 0; col < 3; col++) {
            const index = row * 3 + col
            const length = worldVertex(mesh, index).distanceTo(
              worldVertex(mesh, index - 3)
            )
            expect(length).toBeLessThanOrEqual(0.073 * 1.041)
          }
      }
      rig.dispose()
    }
  )

  it.each(['plate', 'fur'] as const)(
    'matches standalone %s physics with shared matrices through animation and resets',
    (kind) => {
      const make = () => (kind === 'fur' ? softFixture('fur') : fixture())
      const a = make()
      const b = make()
      const settings: PeltPhysics = {
        ...(a.mesh.userData.pelt_physics as PeltPhysics),
        colliders: [
          { bone: 'Hips', center: [0, 0.82, 0.2], radii: [0.11, 0.09, 0.08] },
        ],
      }
      a.mesh.userData.pelt_physics = settings
      const standalone = createPeltRig(b.mesh, settings)
      for (let i = 0; i < 60; i++) {
        for (const { root, hip } of [a, b]) {
          root.position.set(3 + Math.sin(i * 0.1) * 0.1, 2, -1)
          root.rotation.y = 0.7 + i * 0.01
          root.scale.setScalar(1.2)
          hip.position.y = 1 + Math.sin(i * 0.2) * 0.03
          hip.rotation.x = Math.sin(i * 0.1) * 0.2
        }
        if (i === 30) {
          resetPeltPhysics(a.root)
          standalone.reset()
        }
        const dt = i === 45 ? 1 : 1 / 60
        updatePeltPhysics(a.root, dt)
        standalone.update(dt)
        expect(
          Array.from(a.mesh.geometry.getAttribute('position').array)
        ).toEqual(Array.from(b.mesh.geometry.getAttribute('position').array))
      }
      disposePeltPhysics(a.root)
      standalone.dispose()
    }
  )

  it('keeps a heavier side pelt closer to its wearer after a sudden movement', () => {
    const a = fixture(),
      b = fixture(),
      c = fixture()
    const normal = createPeltRig(a.mesh, config)
    const heavy = createPeltRig(b.mesh, {
      ...config,
      motion: {
        damping: 26,
        stiffness: 40,
        gravity: 14,
        inertia: 0.2,
        max_angle: 0.65,
      },
    })
    const middle = createPeltRig(c.mesh, {
      ...config,
      motion: {
        damping: 19,
        stiffness: 52,
        gravity: 10,
        inertia: 0.6,
        max_angle: 0.95,
      },
    })
    normal.reset()
    heavy.reset()
    middle.reset()
    a.root.position.z = b.root.position.z = c.root.position.z = -0.08
    let normalSwing = 0,
      heavySwing = 0,
      middleSwing = 0
    for (let i = 0; i < 60; i++) {
      normal.update(1 / 60)
      heavy.update(1 / 60)
      middle.update(1 / 60)
      normalSwing = Math.max(normalSwing, worldVertex(a.mesh, 17).z - 0.12)
      heavySwing = Math.max(heavySwing, worldVertex(b.mesh, 17).z - 0.12)
      middleSwing = Math.max(middleSwing, worldVertex(c.mesh, 17).z - 0.12)
    }
    expect(normalSwing).toBeGreaterThan(0.04)
    expect(heavySwing).toBeLessThan(normalSwing * 0.4)
    expect(middleSwing).toBeGreaterThan(heavySwing * 1.5)
    expect(middleSwing).toBeLessThan(normalSwing * 0.85)
    expect(worldVertex(b.mesh, 17).z).toBeCloseTo(0.12, 3)
  })

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
  it('keeps side clearance and lifts further when the thigh swings forward', () => {
    const { root, hip, mesh } = fixture()
    mesh.geometry.rotateY(Math.PI / 2)
    const thigh = new THREE.Bone()
    thigh.name = 'LeftUpLeg'
    root.add(thigh)
    root.updateMatrixWorld(true)
    mesh.bind(new THREE.Skeleton([hip, thigh]))
    const rig = createPeltRig(mesh, {
      ...config,
      pivot: [0.2, 1.055, 0],
      outward: [1, 0, 0],
      bend: { droop: 0, contact: true },
      motion: {
        damping: 19,
        stiffness: 52,
        gravity: 10,
        inertia: 0.6,
        min_angle: 0.18,
        flex_clearance: 0.35,
      },
      colliders: [
        { bone: 'LeftUpLeg', center: [0, 0.8, 0], radii: [0.01, 0.01, 0.01] },
      ],
    })
    rig.reset()
    const pinned = worldVertex(mesh, 1)
    const standing = worldVertex(mesh, 16)
    expect(standing.x).toBeGreaterThan(0.25)
    thigh.rotation.x = 0.7
    rig.update(1 / 60)
    expect(worldVertex(mesh, 16).x).toBeGreaterThan(standing.x + 0.04)
    expect(worldVertex(mesh, 1).distanceTo(pinned)).toBeLessThan(1e-6)
    rig.dispose()
  })

  it('folds both sides around the longitudinal centerline while keeping the waist and lengths', () => {
    const { mesh } = fixture()
    const source = mesh.geometry
    const rig = createPeltRig(mesh, {
      ...config,
      bend: {
        droop: 0,
        crease: { width: 0.025, angle: 0.25, damping: 14, stiffness: 36 },
      },
    })
    rig.reset()
    const position = mesh.geometry.getAttribute('position')
    const rest = source.getAttribute('position')
    const center = new THREE.Vector3().fromBufferAttribute(position, 16)
    for (const i of [15, 17]) {
      const edge = new THREE.Vector3().fromBufferAttribute(position, i)
      expect(center.z - edge.z).toBeGreaterThan(0.008)
    }
    for (const i of [0, 1, 2, 16])
      expect(
        new THREE.Vector3()
          .fromBufferAttribute(position, i)
          .distanceTo(new THREE.Vector3().fromBufferAttribute(rest, i))
      ).toBeLessThan(0.001)
    const index = mesh.geometry.index!
    for (let i = 0; i < index.count; i += 3)
      for (let j = 0; j < 3; j++) {
        const a = index.getX(i + j),
          b = index.getX(i + ((j + 1) % 3))
        const length = new THREE.Vector3()
          .fromBufferAttribute(rest, a)
          .distanceTo(new THREE.Vector3().fromBufferAttribute(rest, b))
        const posed = new THREE.Vector3()
          .fromBufferAttribute(position, a)
          .distanceTo(new THREE.Vector3().fromBufferAttribute(position, b))
        expect(posed).toBeLessThanOrEqual(length * 1.021)
      }
    rig.dispose()
    expect(mesh.geometry).toBe(source)
  })

  it('lets the lower half fold at the middle and lag behind the waist without elongating', () => {
    const { root, mesh } = fixture()
    const source = mesh.geometry
    const original = Array.from(source.getAttribute('position').array)
    const rig = createPeltRig(mesh, {
      ...config,
      motion: {
        damping: 19,
        stiffness: 52,
        gravity: 10,
        inertia: 0.6,
        max_angle: 0.95,
      },
      bend: {
        droop: 0.6,
        joint: {
          at: 0.5,
          width: 0.14,
          damping: 12,
          stiffness: 28,
          inertia: 0.65,
        },
      },
    })
    rig.reset()
    root.position.z = -0.08
    let fold = 0
    for (let frame = 0; frame < 90; frame++) {
      rig.update(1 / 60)
      const p = mesh.geometry.getAttribute('position')
      const upper = new THREE.Vector3()
        .fromBufferAttribute(p, 7)
        .sub(new THREE.Vector3().fromBufferAttribute(p, 4))
      const lower = new THREE.Vector3()
        .fromBufferAttribute(p, 16)
        .sub(new THREE.Vector3().fromBufferAttribute(p, 13))
      fold = Math.max(
        fold,
        Math.abs(Math.atan2(upper.z, -upper.y) - Math.atan2(lower.z, -lower.y))
      )
      for (let row = 1; row < 6; row++) {
        const a = new THREE.Vector3().fromBufferAttribute(p, row * 3 + 1)
        const b = new THREE.Vector3().fromBufferAttribute(p, (row - 1) * 3 + 1)
        expect(a.distanceTo(b)).toBeLessThan(0.073 * 1.021)
      }
      expect(
        worldVertex(mesh, 1).distanceTo(new THREE.Vector3(0, 1.055, 0.12))
      ).toBeLessThan(1e-6)
    }
    expect(fold).toBeGreaterThan(0.15)
    expect(Array.from(source.getAttribute('position').array)).toEqual(original)
    rig.dispose()
    expect(mesh.geometry).toBe(source)
  })

  it('updates local thigh contact even when the hip hinge angle stays still', () => {
    const { root, hip, mesh } = fixture()
    const thigh = new THREE.Bone()
    thigh.name = 'LeftUpLeg'
    root.add(thigh)
    root.updateMatrixWorld(true)
    mesh.bind(new THREE.Skeleton([hip, thigh]))
    const source = mesh.geometry
    const rig = createPeltRig(mesh, {
      ...config,
      bend: { droop: 0.6, contact: true },
      colliders: [
        {
          bone: 'LeftUpLeg',
          center: [0, 0.82, 0.16],
          radii: [0.11, 0.12, 0.09],
        },
      ],
    })
    rig.reset()
    const before = Array.from(mesh.geometry.getAttribute('position').array)
    thigh.position.z = 0.05
    rig.update(1 / 60)
    const after = Array.from(mesh.geometry.getAttribute('position').array)
    expect(
      Math.max(...after.map((v, i) => Math.abs(v - before[i])))
    ).toBeGreaterThan(0.001)
    expect(after.every(Number.isFinite)).toBe(true)
    rig.dispose()
    expect(mesh.geometry).toBe(source)
  })

  it('curves a separate pelt toward its hem without stretching it into the legs', () => {
    const { root, mesh } = fixture()
    const source = mesh.geometry
    const original = Array.from(source.getAttribute('position').array)
    const rig = createPeltRig(mesh, { ...config, bend: { droop: 0.6 } })
    rig.reset()
    root.position.z = -0.08
    let curve = 0
    for (let frame = 0; frame < 30; frame++) {
      rig.update(1 / 60)
      const position = mesh.geometry.getAttribute('position')
      const top = new THREE.Vector3().fromBufferAttribute(position, 1)
      const middle = new THREE.Vector3().fromBufferAttribute(position, 10)
      const bottom = new THREE.Vector3().fromBufferAttribute(position, 16)
      const straight = new THREE.Line3(top, bottom).closestPointToPoint(
        middle,
        true,
        new THREE.Vector3()
      )
      curve = Math.max(curve, middle.distanceTo(straight))
      for (let row = 1; row < 6; row++) {
        const a = new THREE.Vector3().fromBufferAttribute(position, row * 3 + 1)
        const b = new THREE.Vector3().fromBufferAttribute(
          position,
          (row - 1) * 3 + 1
        )
        expect(a.distanceTo(b)).toBeLessThan(0.073 * 1.015)
      }
      expect(
        worldVertex(mesh, 1).distanceTo(new THREE.Vector3(0, 1.055, 0.12))
      ).toBeLessThan(1e-6)
    }
    expect(curve).toBeGreaterThan(0.005)
    expect(Array.from(source.getAttribute('position').array)).toEqual(original)
    rig.dispose()
    expect(mesh.geometry).toBe(source)
  })

  it.each(['strap', 'fur'] as const)(
    'keeps both top rows of %s fixed while the lower rows bend',
    (kind) => {
      const { root, hip, mesh, settings } = softFixture(kind)
      const source = mesh.geometry.clone()
      const rig = createPeltRig(mesh, settings)
      rig.reset()
      root.position.z = -0.06
      hip.rotation.y = 0.15
      const positions = mesh.geometry.getAttribute('position')
      let bend = 0
      for (let i = 0; i < 12; i++) {
        rig.update(1 / 60)
        const top = new THREE.Vector3().fromBufferAttribute(positions, 4)
        const middle = new THREE.Vector3().fromBufferAttribute(positions, 10)
        const hem = new THREE.Vector3().fromBufferAttribute(positions, 16)
        const straight = new THREE.Line3(top, hem).closestPointToPoint(
          middle,
          true,
          new THREE.Vector3()
        )
        bend = Math.max(bend, middle.distanceTo(straight))
      }
      const original = source.getAttribute('position')
      for (const start of [0, 18])
        for (let i = start; i < start + 6; i++) {
          expect([
            positions.getX(i),
            positions.getY(i),
            positions.getZ(i),
          ]).toEqual([original.getX(i), original.getY(i), original.getZ(i)])
        }
      expect(bend).toBeGreaterThan(0.003)
      for (let i = 6; i < 18; i++) {
        const edge = new THREE.Vector3()
          .fromBufferAttribute(positions, i)
          .distanceTo(new THREE.Vector3().fromBufferAttribute(positions, i - 3))
        expect(edge / 0.073).toBeLessThanOrEqual(1.0401)
        expect(edge / 0.073).toBeGreaterThan(0.9)
      }
    }
  )

  it.each([30, 120, 144])(
    'gives the same flexible result at %i and 60 fps',
    (fps) => {
      const a = softFixture(),
        b = softFixture()
      const ra = createPeltRig(a.mesh, a.settings),
        rb = createPeltRig(b.mesh, b.settings)
      ra.reset()
      rb.reset()
      a.root.position.z = b.root.position.z = -0.04
      for (let i = 0; i < fps; i++) ra.update(1 / fps)
      for (let i = 0; i < 60; i++) rb.update(1 / 60)
      expect(
        Array.from(a.mesh.geometry.getAttribute('position').array)
      ).toEqual(Array.from(b.mesh.geometry.getAttribute('position').array))
    }
  )

  it('skips geometry uploads and normal updates between physics steps', () => {
    const { mesh, settings } = softFixture('fur')
    const rig = createPeltRig(mesh, settings)
    rig.reset()
    const positions = mesh.geometry.getAttribute(
      'position'
    ) as THREE.BufferAttribute
    const normals = mesh.geometry.getAttribute(
      'normal'
    ) as THREE.BufferAttribute
    const positionVersion = positions.version
    const normalVersion = normals.version
    rig.update(1 / 120)
    expect(positions.version).toBe(positionVersion)
    expect(normals.version).toBe(normalVersion)
    rig.update(1 / 120)
    expect(positions.version).toBeGreaterThan(positionVersion)
    expect(normals.version).toBeGreaterThan(normalVersion)
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
    let before = Array.from(positions.array)
    for (let i = 0; i < 120; i++) {
      updatePeltPhysics(root, 1 / 60)
      const current = Array.from(positions.array)
      const movement = current.map((v, j) => Math.abs(v - before[j]))
      expect(Math.max(...movement)).toBeLessThan(0.0001)
      before = current
    }
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
