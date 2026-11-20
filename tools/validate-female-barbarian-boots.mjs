import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler, hash } from './lib/headless-three.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'
await ensureFemalePreview('animations')

const directory =
  process.argv[2] ?? 'assets/items/female_barbarian_boots/tripo_v1/fitted_v3'
const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const partPath = `${directory}/boots_female_barbarian.glb`
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule(
    '/src/lib/utils/modularCharacter.ts'
  )
  const { femaleOutfitMask } = await server.ssrLoadModule(
    '/src/lib/utils/female-outfit-mask.ts'
  )
  const { femaleBootGround } = await server.ssrLoadModule(
    '/src/lib/utils/female-boot-ground.ts'
  )
  const { femaleSeatOffset } = await server.ssrLoadModule(
    '/src/previews/female-seat.ts'
  )
  const base = await load(basePath)
  const prepared = JSON.parse(
    readFileSync('assets/modular_human_female_01/preview/animations.json')
  )
  for (const data of prepared.clips) {
    const clip = THREE.AnimationClip.parse(data)
    const index = base.animations.findIndex((entry) => entry.name === clip.name)
    if (index < 0) base.animations.push(clip)
    else base.animations[index] = clip
  }
  const asset = await load(partPath)
  const boots = bindModularPart(base.scene, asset.scene)
  assert.equal(
    asset.scene.userData.heel_pose.fixed_toe_quaternions,
    undefined,
    'Sandal must not override toe animation'
  )
  let body
  base.scene.traverse((node) => {
    if (
      node.isSkinnedMesh &&
      base.parser.associations.get(node)?.primitives === 0
    )
      body = node
  })
  assert.ok(body)
  const skeleton = body.skeleton
  base.scene.updateMatrixWorld(true)
  const parts = boots.map((mesh) => {
    for (let i = 0; i < 65; i++) {
      assert.equal(mesh.skeleton.bones[i], skeleton.bones[i])
      assert.deepEqual(
        mesh.skeleton.boneInverses[i].elements,
        skeleton.boneInverses[i].elements
      )
    }
    const { position, skinIndex, skinWeight } = mesh.geometry.attributes
    const rest = []
    for (let i = 0; i < position.count; i++) {
      const point = mesh.getVertexPosition(i, new THREE.Vector3())
      assert.ok(
        point.distanceTo(new THREE.Vector3().fromBufferAttribute(position, i)) <
          1.8e-6
      )
      rest.push(point)
      let sum = 0
      for (let k = 0; k < 4; k++) {
        const w = skinWeight.getComponent(i, k)
        sum += w
        if (!w) continue
        assert.ok(w >= 0 && w <= 1)
        const name = skeleton.bones[skinIndex.getComponent(i, k)].name
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
        const a = index.getX(i + k),
          b = index.getX(i + ((k + 1) % 3))
        const length = rest[a].distanceTo(rest[b])
        if (length < 0.0036) continue
        const rigid =
          skinWeight.getX(a) > 0.99999 &&
          skinWeight.getX(b) > 0.99999 &&
          skinIndex.getX(a) === skinIndex.getX(b)
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
  assert.equal(
    body.geometry.index.count,
    originalGeometry.index.count - coverage.hide_triangle_indices.length * 3
  )
  mask.setEquipped('boots', false)
  assert.equal(body.geometry, originalGeometry)
  const ground = femaleBootGround(
    base.scene,
    body,
    boots,
    asset.scene.userData.heel_pose
  )
  const footBones = [
    'LeftFoot',
    'RightFoot',
    'LeftToeBase',
    'RightToeBase',
  ].map((name) => base.scene.getObjectByName(name))
  const bareRotations = footBones.map((bone) => bone.quaternion.clone())
  const heelVertices = []
  for (let i = 0; i < body.geometry.attributes.position.count; i++) {
    const p = new THREE.Vector3().fromBufferAttribute(
      body.geometry.attributes.position,
      i
    )
    if (p.y < 0.072 && p.z < -0.0216) heelVertices.push(i)
  }
  const meanHeel = () =>
    heelVertices.reduce(
      (sum, index) =>
        sum + body.getVertexPosition(index, new THREE.Vector3()).y,
      0
    ) / heelVertices.length
  const bareHeel = meanHeel()
  const lift = ground.apply()
  const referenceLift = JSON.parse(
    readFileSync('assets/modular_human_female_01/rig_v1/heel-ground.json')
  ).posed_floor_offset
  assert.ok(
    Math.abs(lift - referenceLift) < 1.8e-6,
    'Sole edge lowered the shared heel ground'
  )
  for (const part of parts) {
    for (const edge of part.flexibleEdges)
      edge[2] = part.mesh
        .getVertexPosition(edge[0], new THREE.Vector3())
        .distanceTo(part.mesh.getVertexPosition(edge[1], new THREE.Vector3()))
    part.flexibleEdges = part.flexibleEdges.filter((edge) => edge[2] > 0.0036)
  }
  const solePlanes = [-1, 1].map((side) => {
    const support = parts.flatMap(({ mesh }) =>
      Array.from(
        { length: mesh.geometry.attributes.position.count },
        (_, index) => ({
          mesh,
          index,
          point: mesh.getVertexPosition(index, new THREE.Vector3()),
        })
      ).filter(({ mesh, index, point }) => {
        const { skinIndex, skinWeight } = mesh.geometry.attributes
        return (
          Math.sign(point.x) === side &&
          point.y < -0.054468 &&
          skinWeight.getX(index) > 0.99999 &&
          skeleton.bones[skinIndex.getX(index)].name.endsWith('Foot')
        )
      })
    )
    const rear = support.reduce((a, b) => (a.point.z < b.point.z ? a : b))
    const front = support.filter(({ point }) => point.z > 0.027)
    const left = front.reduce((a, b) => (a.point.x < b.point.x ? a : b))
    const right = front.reduce((a, b) => (a.point.x > b.point.x ? a : b))
    assert.ok(
      left.point.distanceTo(right.point) > 0.036,
      `Sole support side ${side}: ${front.length} vertices, width ${left.point.distanceTo(right.point)}`
    )
    assert.ok(rear.point.distanceTo(left.point) > 0.09)
    return [rear, left, right]
  })
  const standing = new Set([
    'idle1',
    'idle_armed',
    'combat_idle',
    'torch_idle1',
    'torch_combat_idle',
    'torch_sword_idle1',
  ])
  const heelElevation = meanHeel() + lift - bareHeel
  const rotationAngles = footBones.map((bone, i) =>
    THREE.MathUtils.radToDeg(bone.quaternion.angleTo(bareRotations[i]))
  )
  assert.ok(rotationAngles.every((angle) => Math.abs(angle - 30) < 0.001))
  assert.ok(heelElevation > 0.054, `Heel elevation ${heelElevation}`)
  for (let i = 0; i < 30; i++) ground.apply()
  ground.restore()
  for (let i = 0; i < footBones.length; i++)
    assert.ok(footBones[i].quaternion.angleTo(bareRotations[i]) < 1e-7)
  const clips = []
  const animationOffsets = []
  const sample = clipSampler(base.scene, {
    samples: 61,
    restore: skeleton.bones,
  })
  for (const clip of base.animations) {
    const lifts = []
    let maxRigidStretch = 0,
      maxFlexibleStretch = 0,
      maxMotion = 0,
      minY = Infinity,
      maxLift = 0,
      maxSoleTilt = 0
    for (const { time } of sample(clip)) {
      const animatedRotations = footBones.map((bone) => bone.quaternion.clone())
      const heelLift = ground.apply(
        clip.name === 'sit_idle' ? femaleSeatOffset : undefined,
        clip.name
      )
      const lift = clip.name === 'sit_idle' ? femaleSeatOffset : heelLift
      for (const [i, bone] of footBones.entries()) {
        if (!bone.name.endsWith('ToeBase')) continue
        const expected = animatedRotations[i]
          .clone()
          .multiply(
            new THREE.Quaternion().fromArray(
              asset.scene.userData.heel_pose.local_quaternion_offsets[bone.name]
            )
          )
        assert.ok(
          bone.quaternion
            .toArray()
            .every(
              (value, k) => Math.abs(value - expected.toArray()[k]) < 1e-9
            ),
          `${clip.name} changed the existing toe animation`
        )
      }
      lifts.push({
        time,
        lift,
        hips: base.scene.getObjectByName('Hips').position.toArray(),
        rotations: Object.fromEntries(
          ['Left', 'Right'].flatMap((side) =>
            ['UpLeg', 'Leg', 'Foot', 'ToeBase'].map((suffix) => {
              const name = side + suffix
              return [
                name,
                base.scene.getObjectByName(name).quaternion.toArray(),
              ]
            })
          )
        ),
      })
      maxLift = Math.max(maxLift, lift)
      for (const plane of solePlanes) {
        const [a, b, c] = plane.map(({ mesh, index }) =>
          mesh.getVertexPosition(index, new THREE.Vector3())
        )
        const normal = b.sub(a).cross(c.sub(a)).normalize()
        maxSoleTilt = Math.max(maxSoleTilt, Math.hypot(normal.x, normal.z))
      }
      for (const [
        partIndex,
        { mesh, rest, rigidEdges, flexibleEdges },
      ] of parts.entries()) {
        const posed = rest.map((_, i) =>
          mesh.getVertexPosition(i, new THREE.Vector3())
        )
        for (let i = 0; i < posed.length; i++) {
          assert.ok(posed[i].toArray().every(Number.isFinite))
          maxMotion = Math.max(maxMotion, posed[i].distanceTo(rest[i]))
          minY = Math.min(minY, posed[i].y + lift)
        }
        for (const [a, b, length] of rigidEdges)
          maxRigidStretch = Math.max(
            maxRigidStretch,
            Math.abs(posed[a].distanceTo(posed[b]) / length - 1)
          )
        for (const [a, b, length] of flexibleEdges)
          maxFlexibleStretch = Math.max(
            maxFlexibleStretch,
            posed[a].distanceTo(posed[b]) / length
          )
      }
      if (time === 0) {
        const corrected = footBones.map((bone) => bone.quaternion.clone())
        ground.apply(
          clip.name === 'sit_idle' ? femaleSeatOffset : undefined,
          clip.name
        )
        for (let i = 0; i < footBones.length; i++)
          assert.ok(
            footBones[i].quaternion
              .toArray()
              .every(
                (value, j) => Math.abs(value - corrected[i].toArray()[j]) < 1e-9
              ),
            `${clip.name} posture accumulated`
          )
      }
      ground.restore()
      for (const [i, bone] of footBones.entries())
        assert.ok(
          bone.quaternion
            .toArray()
            .every(
              (value, k) =>
                Math.abs(value - animatedRotations[i].toArray()[k]) < 1e-9
            ),
          `${clip.name} unequip did not restore animated toes`
        )
    }
    assert.ok(
      maxRigidStretch < 0.001,
      `${clip.name} rigid stretch ${maxRigidStretch}`
    )
    assert.ok(maxMotion < 3.6)
    assert.ok(
      maxFlexibleStretch < 3.5,
      `${clip.name} flexible stretch ${maxFlexibleStretch}`
    )
    if (standing.has(clip.name))
      assert.ok(
        maxSoleTilt < 0.02,
        `${clip.name} sole plane tilt ${maxSoleTilt}`
      )
    clips.push({
      name: clip.name,
      samples: 61,
      maximum_rigid_edge_stretch: maxRigidStretch,
      maximum_sole_plane_tilt: maxSoleTilt,
      sole_leveling_expected: standing.has(clip.name),
      maximum_flexible_edge_ratio: maxFlexibleStretch,
      maximum_motion: maxMotion,
      minimum_height_after_ground_correction: minY,
      maximum_ground_lift: maxLift,
    })
    animationOffsets.push({ name: clip.name, lifts })
  }
  mask.dispose()
  const report = {
    date: '2026-10-15',
    source: { path: partPath, sha256: hash(partPath) },
    body: { path: basePath, sha256: hash(basePath) },
    exact_65_bone_bind_compatibility: true,
    four_normalized_influences: true,
    flexible_edge_reference:
      'Equipped heel pose; edges longer than 3.6 mm',
    sole_plane_reference:
      'Heel tip and rigid Foot-weighted ball contact; flexing toe tip excluded',
    original_toe_animation_preserved: true,
    no_opposite_leg_weights: true,
    skin_mask_restores: true,
    heel_posture: {
      rotation_angles_degrees: rotationAngles,
      root_lift: lift,
      heel_elevation: heelElevation,
      no_accumulation_and_bare_pose_restored: true,
    },
    triangles: boots.reduce(
      (sum, mesh) => sum + mesh.geometry.index.count / 3,
      0
    ),
    clips,
    scope:
      'Actual bindModularPart and animation evaluation; rendered pose review required for visual intersections.',
  }
  writeFileSync(
    `${directory}/runtime-validation.json`,
    JSON.stringify(report, null, 2) + '\n'
  )
  writeFileSync(
    `${directory}/heel-animation-offsets.json`,
    JSON.stringify(animationOffsets, null, 2) + '\n'
  )
  console.log(JSON.stringify(report, null, 2))
} finally {
  await server.close()
}
