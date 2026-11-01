import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler, hash } from './lib/headless-three.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'

await ensureFemalePreview('animations')
const directory = process.argv[2] ?? 'assets/items/female_caveman_boots/tripo_v1/fitted_v4'
const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const partPath = `${directory}/boots_female_caveman.glb`
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { femaleOutfitMask } = await server.ssrLoadModule('/src/lib/utils/female-outfit-mask.ts')
  const { femaleBootGround, standingMotions } = await server.ssrLoadModule('/src/lib/utils/female-boot-ground.ts')
  const { femaleSeatOffset } = await server.ssrLoadModule('/src/previews/female-seat.ts')
  const { updateModularPoseCorrectives } = await server.ssrLoadModule('/src/lib/utils/modularPoseCorrectives.ts')
  const base = await load(basePath)
  for (const data of JSON.parse(readFileSync('assets/modular_human_female_01/preview/animations.json')).clips) {
    const clip = THREE.AnimationClip.parse(data)
    const i = base.animations.findIndex(entry => entry.name === clip.name)
    if (i < 0) base.animations.push(clip)
    else base.animations[i] = clip
  }
  const asset = await load(partPath)
  const boots = bindModularPart(base.scene, asset.scene)
  assert.equal(boots.length, 2)
  let body
  base.scene.traverse(node => {
    const association = base.parser.associations.get(node)
    if (node.isSkinnedMesh && association?.meshes === 0 && association.primitives === 0) body = node
  })
  const skeleton = body.skeleton
  base.scene.updateMatrixWorld(true)
  const parts = boots.map(mesh => {
    assert.equal(mesh.skeleton.bones.length, 65)
    for (let i = 0; i < 65; i++) {
      assert.equal(mesh.skeleton.bones[i], skeleton.bones[i])
      assert.deepEqual(mesh.skeleton.boneInverses[i].elements, skeleton.boneInverses[i].elements)
    }
    const { position, skinIndex, skinWeight } = mesh.geometry.attributes
    const rest = []
    for (let i = 0; i < position.count; i++) {
      const p = mesh.getVertexPosition(i, new THREE.Vector3())
      assert.ok(p.distanceTo(new THREE.Vector3().fromBufferAttribute(position, i)) < 2e-6)
      rest.push(p)
      let sum = 0
      for (let k = 0; k < 4; k++) {
        const w = skinWeight.getComponent(i, k)
        assert.ok(Number.isFinite(w) && w >= 0)
        sum += w
        if (!w) continue
        const name = skeleton.bones[skinIndex.getComponent(i, k)].name
        assert.ok(name === 'Hips' || name.startsWith(p.x < 0 ? 'Right' : 'Left'), name)
        assert.ok(name === 'Hips' || /(UpLeg|Leg|Foot)$/.test(name), name)
      }
      assert.ok(Math.abs(sum-1) < 1e-6)
    }
    const edges = [], rigidEdges = []
    const index = mesh.geometry.index
    for (let i = 0; i < index.count; i += 3) for (let k = 0; k < 3; k++) {
      const a = index.getX(i+k), b = index.getX(i+(k+1)%3), length = rest[a].distanceTo(rest[b])
      if (length < 1e-6) continue
      const rigid = rest[a].y < .324 && rest[b].y < .324 && skinWeight.getX(a) > .99999 && skinWeight.getX(b) > .99999 && skinIndex.getX(a) === skinIndex.getX(b)
      ;(rigid ? rigidEdges : edges).push([a,b,length])
    }
    return { mesh, rest, edges, rigidEdges }
  })
  const geometry = body.geometry
  const mask = femaleOutfitMask(base)
  const coverage = JSON.parse(readFileSync(`${directory}/body-occlusion.json`))
  mask.register('caveman-boots', coverage)
  mask.setEquipped('caveman-boots', true)
  assert.equal(body.geometry.index.count, geometry.index.count-coverage.hide_triangle_indices.length*3)
  mask.setEquipped('caveman-boots', false)
  assert.equal(body.geometry, geometry)
  const ground = femaleBootGround(base.scene, body, boots, asset.scene.userData.heel_pose)
  ground.pose('walk')
  for (const part of parts) {
    const equipped = part.rest.map((_,i) => part.mesh.getVertexPosition(i,new THREE.Vector3()))
    part.edges = part.edges.map(([a,b]) => [a,b,equipped[a].distanceTo(equipped[b])]).filter(([, , length]) => length >= .004)
    part.rigidEdges = part.rigidEdges.filter(([, , length]) => length >= .004)
  }
  ground.restore()
  base.scene.updateMatrixWorld(true)
  const handBones = ['LeftHand','RightHand'].map(name=>base.scene.getObjectByName(name))
  const armBones = ['Left','Right'].flatMap(side=>['Arm','ForeArm','Hand'].map(name=>base.scene.getObjectByName(side+name)))
  const clips = [], snapshots = []
  const sampler = clipSampler(base.scene, { samples: 61, restore: skeleton.bones })
  const footSamples = Array.from({length:body.geometry.attributes.position.count},(_,i)=>i)
    .filter(i=>body.geometry.attributes.position.getY(i)<.144)
  for (const clip of base.animations) {
    updateModularPoseCorrectives(base.scene,clip.name)
    let rigidStretch = 0, flexibleRatio = 0, minimumHeight = Infinity, maximumLift = 0, minimumGroundAlignment = Infinity, maximumHandLiftError = 0, worstFlexibleEdge
    for (const { time, sample } of sampler(clip)) {
      const originalRotations = skeleton.bones.map(bone=>bone.quaternion.toArray())
      const originalHands = handBones.map(bone=>bone.getWorldPosition(new THREE.Vector3()))
      const originalArms = armBones.map(bone=>bone.quaternion.toArray())
      const barefootFloor = Math.min(...footSamples.map(i=>body.getVertexPosition(i,new THREE.Vector3()).y))
      const lift = ground.apply(clip.name === 'sit_idle' ? femaleSeatOffset : undefined, clip.name)
      const elevation = clip.name === 'sit_idle' ? femaleSeatOffset : lift
      if (clip.name === 'sit_idle') {
        for (let i=0;i<handBones.length;i++) {
          const expected = originalHands[i].clone().add(new THREE.Vector3(0,asset.scene.userData.heel_pose.seated_hand_lift_m,0))
          maximumHandLiftError = Math.max(maximumHandLiftError,handBones[i].getWorldPosition(new THREE.Vector3()).distanceTo(expected))
        }
      } else {
        for (let i=0;i<armBones.length;i++) assert.deepEqual(armBones[i].quaternion.toArray(),originalArms[i])
      }
      maximumLift = Math.max(maximumLift, elevation)
      let frameFloor = Infinity
      for (const {mesh, rest, edges, rigidEdges} of parts) {
        const posed = rest.map((_,i) => mesh.getVertexPosition(i, new THREE.Vector3()))
        for (const p of posed) {
          assert.ok(p.toArray().every(Number.isFinite))
          minimumHeight = Math.min(minimumHeight, p.y+elevation)
          frameFloor = Math.min(frameFloor, p.y+elevation)
        }
        for (const [a,b,length] of edges) {
          const ratio = posed[a].distanceTo(posed[b])/length
          if (ratio > flexibleRatio) {
            flexibleRatio = ratio
            worstFlexibleEdge = {mesh:mesh.name,a,b,time,length,rest_a:rest[a].toArray(),rest_b:rest[b].toArray()}
          }
        }
        for (const [a,b,length] of rigidEdges) rigidStretch = Math.max(rigidStretch, Math.abs(posed[a].distanceTo(posed[b])/length-1))
      }
      minimumGroundAlignment = Math.min(minimumGroundAlignment,frameFloor-(clip.name === 'sit_idle' ? 0 : Math.min(0,barefootFloor)))
      snapshots.push({ clip: clip.name, phase: sample/60, time, lift: elevation,
        matrices: skeleton.bones.map((bone,i) => new THREE.Matrix4().multiplyMatrices(bone.matrixWorld,skeleton.boneInverses[i]).elements) })
      if (sample === 0) {
        const before = skeleton.bones.map(bone => bone.quaternion.clone())
        ground.apply(clip.name === 'sit_idle' ? femaleSeatOffset : undefined, clip.name)
        for (let i = 0; i < before.length; i++)
          assert.ok(before[i].toArray().every((value,k) => Math.abs(value-skeleton.bones[i].quaternion.toArray()[k]) < 1e-9))
      }
      ground.restore()
      for (let i=0;i<originalRotations.length;i++) assert.deepEqual(skeleton.bones[i].quaternion.toArray(),originalRotations[i])
    }
    clips.push({name:clip.name,samples:61,maximum_rigid_edge_stretch:rigidStretch,
      maximum_flexible_edge_ratio:flexibleRatio,minimum_height_m:minimumHeight,maximum_root_lift_m:maximumLift,
      minimum_ground_difference_from_original_motion_m:minimumGroundAlignment,
      maximum_seated_hand_lift_error_m:maximumHandLiftError,
      sole_leveling_expected:standingMotions.has(clip.name),worst_flexible_edge:worstFlexibleEdge})
  }
  const report = {date:'2026-11-01',source:{path:partPath,sha256:hash(partPath)},body:{path:basePath,sha256:hash(basePath)},
    triangles:boots.reduce((sum,mesh)=>sum+mesh.geometry.index.count/3,0),exact_65_bone_bind:true,
    normalized_four_weights:true,no_opposite_leg_influences:true,body_mask_restores:true,heel_pose_does_not_accumulate:true,
    flexible_edge_reference:'Heel-equipped authoring pose before animation; inverse binding can compress the unequipped bind geometry.',clips}
  writeFileSync(`${directory}/runtime-validation.json`,JSON.stringify(report,null,2)+'\n')
  writeFileSync(`${directory}/animation-samples.json`,JSON.stringify(snapshots))
  console.log(JSON.stringify(report,null,2))
  assert.ok(clips.every(clip=>clip.maximum_rigid_edge_stretch < .001),'Rigid sole or heel deformed')
  assert.ok(clips.every(clip=>clip.maximum_flexible_edge_ratio < 3.5),'Excessive leather stretch')
  assert.ok(clips.every(clip=>clip.minimum_ground_difference_from_original_motion_m > -.00001),'Boot lowered original motion ground contact')
  assert.ok(clips.every(clip=>clip.maximum_seated_hand_lift_error_m < .00001),'Seated hands missed their clearance target')
  mask.dispose()
} finally { await server.close() }
