import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler, hash } from './lib/headless-three.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'
await ensureFemalePreview('animations')

const directory = process.argv[2] ?? 'assets/items/female_knight_boots/reference_v2/fitted_v1'
const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const partPath = `${directory}/boots_female_knight.glb`
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { femaleOutfitMask } = await server.ssrLoadModule('/src/lib/utils/female-outfit-mask.ts')
  const { femaleBootGround } = await server.ssrLoadModule('/src/lib/utils/female-boot-ground.ts')
  const { femaleSeatOffset } = await server.ssrLoadModule('/src/previews/female-seat.ts')
  const base = await load(basePath)
  const prepared = JSON.parse(readFileSync('assets/modular_human_female_01/preview/animations.json'))
  for (const data of prepared.clips) {
    const clip = THREE.AnimationClip.parse(data)
    const index = base.animations.findIndex(entry => entry.name === clip.name)
    if (index < 0) base.animations.push(clip)
    else base.animations[index] = clip
  }
  const asset = await load(partPath)
  const boots = bindModularPart(base.scene, asset.scene)
  let body
  base.scene.traverse(node => {
    if (node.isSkinnedMesh && base.parser.associations.get(node)?.primitives === 0) body = node
  })
  assert.ok(body)
  const skeleton = body.skeleton
  base.scene.updateMatrixWorld(true)
  const parts = boots.map(mesh => {
    for (let i = 0; i < 65; i++) {
      assert.equal(mesh.skeleton.bones[i], skeleton.bones[i])
      assert.deepEqual(mesh.skeleton.boneInverses[i].elements, skeleton.boneInverses[i].elements)
    }
    const { position, skinIndex, skinWeight } = mesh.geometry.attributes
    const rest = []
    for (let i = 0; i < position.count; i++) {
      const point = mesh.getVertexPosition(i, new THREE.Vector3())
      assert.ok(point.distanceTo(new THREE.Vector3().fromBufferAttribute(position, i)) < 1.8e-6)
      rest.push(point)
      let sum = 0
      for (let k = 0; k < 4; k++) {
        const w = skinWeight.getComponent(i, k)
        sum += w
        if (!w) continue
        assert.ok(w >= 0 && w <= 1)
        const name = mesh.skeleton.bones[skinIndex.getComponent(i, k)].name
        assert.ok(name.startsWith(point.x < 0 ? 'Right' : 'Left'), name)
        assert.ok(/(UpLeg|Leg|Foot|ToeBase)$/.test(name), name)
      }
      assert.ok(Math.abs(sum - 1) < 1e-6)
    }
    const index = mesh.geometry.index
    const rigidEdges = []
    const flexibleEdges = []
    for (let i = 0; i < index.count; i += 3) {
      for (let k = 0; k < 3; k++) {
        const a = index.getX(i + k), b = index.getX(i + (k + 1) % 3)
        const length = rest[a].distanceTo(rest[b])
        if (length < .0036) continue
        const rigid = skinWeight.getX(a) > .99999 && skinWeight.getX(b) > .99999 && skinIndex.getX(a) === skinIndex.getX(b)
        ;(rigid ? rigidEdges : flexibleEdges).push([a, b, length])
      }
    }
    return { mesh, rest, rigidEdges, flexibleEdges }
  })
  const originalGeometry = body.geometry
  const mask = femaleOutfitMask(base)
  const coverage = JSON.parse(readFileSync(`${directory}/body-occlusion.json`))
  mask.register('boots', coverage)
  mask.setEquipped('boots', true)
  let splitTriangles = 0
  for (const { point, normal, triangle_indices } of coverage.clip_planes) {
    const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(new THREE.Vector3(...normal), new THREE.Vector3(...point))
    for (const triangle of triangle_indices) {
      const kept = [0, 1, 2].filter(k => {
        const id = originalGeometry.index.getX(3 * triangle + k)
        return plane.distanceToPoint(new THREE.Vector3().fromBufferAttribute(originalGeometry.attributes.position, id)) >= 0
      }).length
      assert.ok(kept === 1 || kept === 2)
      splitTriangles += kept - 1
    }
  }
  assert.equal(body.geometry.index.count, originalGeometry.index.count + (splitTriangles - coverage.hide_triangle_indices.length) * 3)
  mask.setEquipped('boots', false)
  assert.equal(body.geometry, originalGeometry)
  const ground = femaleBootGround(base.scene, body, boots, asset.scene.userData.heel_pose)
  const footBones = ['LeftFoot', 'RightFoot', 'LeftToeBase', 'RightToeBase'].map(name => base.scene.getObjectByName(name))
  const bareRotations = footBones.map(bone => bone.quaternion.clone())
  const heelVertices = []
  for (let i = 0; i < body.geometry.attributes.position.count; i++) {
    const p = new THREE.Vector3().fromBufferAttribute(body.geometry.attributes.position, i)
    if (p.y < .072 && p.z < -.0216) heelVertices.push(i)
  }
  const meanHeel = () => heelVertices.reduce((sum, index) => sum + body.getVertexPosition(index, new THREE.Vector3()).y, 0) / heelVertices.length
  const bareHeel = meanHeel()
  const lift = ground.apply()
  const forefootSeams = parts.map(({ mesh }) => {
    const vertices = new Map()
    const pairs = []
    const points = Array.from({ length: mesh.geometry.attributes.position.count }, (_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
    const shoe = new THREE.Box3().setFromPoints(points.filter(point => point.y < .099))
    const toe = THREE.MathUtils.lerp(shoe.min.z, shoe.max.z, .88)
    for (const [i, point] of points.entries()) {
      if (point.y >= .099 || point.z <= toe) continue
      const key = point.toArray().map(value => Math.round(value * 1e6)).join(',')
      const other = vertices.get(key)
      if (other === undefined) vertices.set(key, i)
      else pairs.push([other, i])
    }
    return pairs
  })
  const forefootSeamCount = forefootSeams.reduce((sum, pairs) => sum + pairs.length, 0)
  assert.ok(forefootSeamCount > 0, 'Forefoot seam samples are missing')
  const transversePairs = parts.map(({ mesh }) => {
    const rows = new Map()
    for (let i = 0; i < mesh.geometry.attributes.position.count; i++) {
      const point = mesh.getVertexPosition(i, new THREE.Vector3())
      if (point.y > -.027) continue
      const key = [point.y, point.z].map(value => Math.round(value * 1e6)).join(',')
      const row = rows.get(key) ?? []
      row.push({ index: i, point })
      rows.set(key, row)
    }
    return [...rows.values()].flatMap(row => {
      row.sort((a, b) => a.point.x - b.point.x)
      const a = row[0], b = row.at(-1)
      const width = b.point.x - a.point.x
      return width > .0072 ? [[a.index, b.index, width]] : []
    })
  })
  const transversePairCount = transversePairs.reduce((sum, pairs) => sum + pairs.length, 0)
  assert.ok(transversePairCount >= 4, 'Sole width samples are missing')
  const solePlanes = [-1, 1].map(side => {
    const support = parts.flatMap(({ mesh }) => Array.from({ length: mesh.geometry.attributes.position.count }, (_, index) => ({
      mesh, index, point: mesh.getVertexPosition(index, new THREE.Vector3()),
    })).filter(({ point }) => Math.sign(point.x) === side && point.y < -.054468))
    const rear = support.reduce((a, b) => a.point.z < b.point.z ? a : b)
    const toe = Math.max(...support.map(({ point }) => point.z))
    const front = support.filter(({ point }) => point.z > THREE.MathUtils.lerp(rear.point.z, toe, .78))
    const left = front.reduce((a, b) => a.point.x < b.point.x ? a : b)
    const right = front.reduce((a, b) => a.point.x > b.point.x ? a : b)
    assert.ok(left.point.distanceTo(right.point) > .036)
    assert.ok(rear.point.distanceTo(left.point) > .09)
    return [rear, left, right]
  })
  const standing = new Set(['idle1', 'idle_armed', 'combat_idle', 'torch_idle1', 'torch_combat_idle', 'torch_sword_idle1'])
  const heelElevation = meanHeel() + lift - bareHeel
  const rotationAngles = footBones.map((bone, i) => THREE.MathUtils.radToDeg(bone.quaternion.angleTo(bareRotations[i])))
  assert.ok(rotationAngles.every(angle => Math.abs(angle - 30) < .001))
  assert.ok(heelElevation > .054, `Heel elevation ${heelElevation}`)
  for (let i = 0; i < 30; i++) ground.apply()
  ground.restore()
  for (let i = 0; i < footBones.length; i++) assert.ok(footBones[i].quaternion.angleTo(bareRotations[i]) < 1e-7)
  const clips = []
  const animationOffsets = []
  const sample = clipSampler(base.scene, { samples: 61, restore: skeleton.bones })
  for (const clip of base.animations) {
    const lifts = []
    let maxRigidStretch = 0, maxFlexibleStretch = 0, maxMotion = 0, minY = Infinity, maxLift = 0, maxForefootSeamGap = 0, maxTransverseSlope = 0, maxSoleTilt = 0
    for (const { time } of sample(clip)) {
      const heelLift = ground.apply(clip.name === 'sit_idle' ? femaleSeatOffset : undefined, clip.name)
      const lift = clip.name === 'sit_idle' ? femaleSeatOffset : heelLift
      lifts.push({ time, lift,
        hips: base.scene.getObjectByName('Hips').position.toArray(),
        rotations: Object.fromEntries(['Left', 'Right'].flatMap(side => ['UpLeg', 'Leg', 'Foot', 'ToeBase'].map(suffix => {
          const name = side + suffix
          return [name, base.scene.getObjectByName(name).quaternion.toArray()]
        }))) })
      maxLift = Math.max(maxLift, lift)
      for (const plane of solePlanes) {
        const [a, b, c] = plane.map(({ mesh, index }) => mesh.getVertexPosition(index, new THREE.Vector3()))
        const normal = b.sub(a).cross(c.sub(a)).normalize()
        maxSoleTilt = Math.max(maxSoleTilt, Math.hypot(normal.x, normal.z))
      }
      for (const [partIndex, { mesh, rest, rigidEdges, flexibleEdges }] of parts.entries()) {
        const posed = rest.map((_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
        for (const [a, b] of forefootSeams[partIndex])
          maxForefootSeamGap = Math.max(maxForefootSeamGap, posed[a].distanceTo(posed[b]))
        for (const [a, b, width] of transversePairs[partIndex])
          maxTransverseSlope = Math.max(maxTransverseSlope, Math.abs(posed[a].y - posed[b].y) / width)
        for (let i = 0; i < posed.length; i++) {
          assert.ok(posed[i].toArray().every(Number.isFinite))
          maxMotion = Math.max(maxMotion, posed[i].distanceTo(rest[i]))
          minY = Math.min(minY, posed[i].y + lift)
        }
        for (const [a, b, length] of rigidEdges)
          maxRigidStretch = Math.max(maxRigidStretch, Math.abs(posed[a].distanceTo(posed[b]) / length - 1))
        for (const [a, b, length] of flexibleEdges)
          maxFlexibleStretch = Math.max(maxFlexibleStretch, posed[a].distanceTo(posed[b]) / length)
      }
      if (time === 0) {
        const corrected = footBones.map(bone => bone.quaternion.clone())
        ground.apply(clip.name === 'sit_idle' ? femaleSeatOffset : undefined, clip.name)
        for (let i = 0; i < footBones.length; i++)
          assert.ok(footBones[i].quaternion.toArray().every((value, j) => Math.abs(value - corrected[i].toArray()[j]) < 1e-9), `${clip.name} posture accumulated`)
      }
      ground.restore()
    }
    assert.ok(maxRigidStretch < .001, `${clip.name} rigid stretch ${maxRigidStretch}`)
    assert.ok(maxMotion < 3.6)
    assert.ok(maxFlexibleStretch < 3.5, `${clip.name} flexible stretch ${maxFlexibleStretch}`)
    assert.ok(maxForefootSeamGap < 1.8e-5, `${clip.name} forefoot seam gap ${maxForefootSeamGap}`)
    if (standing.has(clip.name)) {
      assert.ok(maxTransverseSlope < .0002, `${clip.name} sole transverse slope ${maxTransverseSlope}`)
      assert.ok(maxSoleTilt < .001, `${clip.name} sole plane tilt ${maxSoleTilt}`)
    }
    clips.push({ name: clip.name, samples: 61, maximum_rigid_edge_stretch: maxRigidStretch,
      maximum_forefoot_seam_gap: maxForefootSeamGap,
      maximum_sole_transverse_slope: maxTransverseSlope,
      maximum_sole_plane_tilt: maxSoleTilt,
      sole_leveling_expected: standing.has(clip.name),
      maximum_flexible_edge_ratio: maxFlexibleStretch, maximum_motion: maxMotion,
      minimum_height_after_ground_correction: minY, maximum_ground_lift: maxLift })
    animationOffsets.push({ name: clip.name, lifts })
  }
  mask.dispose()
  const report = { date: new Date().toISOString().slice(0, 10), source: { path: partPath, sha256: hash(partPath) },
    body: { path: basePath, sha256: hash(basePath) }, exact_65_bone_bind_compatibility: true,
    four_normalized_influences: true, no_opposite_leg_weights: true, skin_mask_restores: true,
    forefoot_seam_pairs: forefootSeamCount,
    sole_transverse_pairs: transversePairCount,
    heel_posture: { rotation_angles_degrees: rotationAngles, root_lift: lift, heel_elevation: heelElevation, no_accumulation_and_bare_pose_restored: true },
    triangles: boots.reduce((sum, mesh) => sum + mesh.geometry.index.count / 3, 0), clips,
    scope: 'Actual bindModularPart and animation evaluation; rendered pose review required for visual intersections.' }
  writeFileSync(`${directory}/runtime-validation.json`, JSON.stringify(report, null, 2) + '\n')
  writeFileSync(`${directory}/heel-animation-offsets.json`, JSON.stringify(animationOffsets, null, 2) + '\n')
  console.log(JSON.stringify(report, null, 2))
} finally { await server.close() }
