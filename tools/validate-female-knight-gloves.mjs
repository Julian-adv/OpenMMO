import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler, hash } from './lib/headless-three.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'
await ensureFemalePreview('animations')

const directory = 'assets/items/female_knight_gloves/tripo_v1/fitted_v3'
const paired = process.argv.includes('--pair')
const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const partPath = `${directory}/${paired ? 'gloves_female_knight' : 'glove_right'}.glb`
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { femaleOutfitMask } = await server.ssrLoadModule('/src/lib/utils/female-outfit-mask.ts')
  const base = await load(basePath)
  const prepared = JSON.parse(readFileSync('assets/modular_human_female_01/preview/animations.json'))
  for (const data of prepared.clips) {
    const clip = THREE.AnimationClip.parse(data)
    const index = base.animations.findIndex(entry => entry.name === clip.name)
    if (index < 0) base.animations.push(clip)
    else base.animations[index] = clip
  }
  let body
  base.scene.traverse(node => {
    if (node.isSkinnedMesh && base.parser.associations.get(node)?.primitives === 0) body = node
  })
  const skeleton = body.skeleton
  const meshes = bindModularPart(base.scene, (await load(partPath)).scene)
  assert.equal(meshes.length, paired ? 4 : 2)
  base.scene.updateMatrixWorld(true)
  const parts = meshes.map(mesh => {
    assert.equal(mesh.skeleton.bones.length, 65)
    for (let i = 0; i < 65; i++) {
      assert.equal(mesh.skeleton.bones[i], skeleton.bones[i])
      assert.deepEqual(mesh.skeleton.boneInverses[i].elements, skeleton.boneInverses[i].elements)
    }
    const { position, skinWeight, skinIndex, normal, tangent } = mesh.geometry.attributes
    assert.ok([...normal.array, ...(tangent?.array ?? [])].every(Number.isFinite))
    const rest = []
    for (let i = 0; i < position.count; i++) {
      const p = mesh.getVertexPosition(i, new THREE.Vector3())
      assert.ok(p.distanceTo(new THREE.Vector3().fromBufferAttribute(position, i)) < 1e-6)
      const side = p.x < 0 ? 'Right' : 'Left'
      let sum = 0
      for (let k = 0; k < 4; k++) {
        const weight = skinWeight.getComponent(i, k)
        sum += weight
        if (weight) {
          const name = skeleton.bones[skinIndex.getComponent(i, k)].name
          assert.ok(name.startsWith(side), name)
          assert.ok(/Arm|Hand/.test(name), name)
        }
      }
      assert.ok(Math.abs(sum-1) < 1e-6)
      rest.push(p)
    }
    const edges = [], rigid = []
    const index = mesh.geometry.index
    for (let i = 0; i < index.count; i += 3) {
      for (let k = 0; k < 3; k++) {
        const a = index.getX(i+k), b = index.getX(i+(k+1)%3)
        const length = rest[a].distanceTo(rest[b])
        if (length < .0015) continue
        edges.push([a,b,length])
        if (skinWeight.getX(a) > .99999 && skinWeight.getX(b) > .99999 && skinIndex.getX(a) === skinIndex.getX(b)) rigid.push([a,b,length])
      }
    }
    return {mesh, rest, edges, rigid}
  })
  const mask = femaleOutfitMask(base)
  const bodyGeometry = body.geometry
  const coverage = JSON.parse(readFileSync(`${directory}/body-occlusion.json`))
  mask.register('gloves', coverage)
  mask.setEquipped('gloves', true)
  assert.equal(body.geometry.index.count, bodyGeometry.index.count-coverage.hide_triangle_indices.length*3)
  const top = bindModularPart(base.scene,(await load('assets/items/female_knight_top/tripo_v1/fitted_v4/top_female_knight.glb')).scene)[0]
  const topGeometry = top.geometry
  const sleeves = JSON.parse(readFileSync(`${directory}/top-occlusion.json`))
  mask.register('sleeves', sleeves, top)
  mask.setEquipped('sleeves', true)
  assert.equal(top.geometry.index.count,topGeometry.index.count-sleeves.hide_triangle_indices.length*3)
  mask.setEquipped('gloves', false)
  assert.equal(body.geometry, bodyGeometry)
  assert.notEqual(top.geometry,topGeometry)
  mask.setEquipped('sleeves',false)
  assert.equal(top.geometry,topGeometry)
  assert.throws(()=>mask.register('bad',{...sleeves,primitive_triangles:sleeves.primitive_triangles+1},top))
  mask.dispose()
  const sample = clipSampler(base.scene, {samples:33, restore:skeleton.bones})
  const clips = [], poses = []
  for (const clip of base.animations) {
    let maximumEdgeRatio = 0, maximumRigidChange = 0, worstEdge
    for (const {time, sample: index} of sample(clip)) {
      for (const {mesh, rest, edges, rigid} of parts) {
        const positions = rest.map((_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
        assert.ok(positions.every(p => p.toArray().every(Number.isFinite)))
        for (const [a,b,length] of edges) {
          const ratio = positions[a].distanceTo(positions[b])/length
          if (ratio > maximumEdgeRatio) {
            maximumEdgeRatio = ratio
            worstEdge = {mesh:mesh.name, a,b,length,time,rest:[rest[a].toArray(),rest[b].toArray()]}
          }
        }
        for (const [a,b,length] of rigid) maximumRigidChange = Math.max(maximumRigidChange, Math.abs(positions[a].distanceTo(positions[b])/length-1))
      }
      if (index % 4 === 0) poses.push({clip:clip.name,time,matrices:skeleton.bones.map((bone,i) => new THREE.Matrix4().multiplyMatrices(bone.matrixWorld,skeleton.boneInverses[i]).toArray())})
    }
    clips.push({name:clip.name, samples:33, maximum_edge_ratio:maximumEdgeRatio, maximum_rigid_change:maximumRigidChange,worst_edge:worstEdge})
    assert.ok(maximumRigidChange < 1e-4)
    assert.ok(maximumEdgeRatio < 3,`${clip.name}: unexpected glove stretch ${maximumEdgeRatio}`)
  }
  const report = {base_sha256:hash(basePath),glove_sha256:hash(partPath),bones:65,exact_bind_compatibility:true,normalized_weights:true,no_opposite_side_influence:true,clips}
  writeFileSync(`${directory}/animation-validation.json`,JSON.stringify(report,null,2)+'\n')
  writeFileSync(`${directory}/validation-poses.json`,JSON.stringify(poses)+'\n')
  console.log(JSON.stringify(report,null,2))
} finally {
  await server.close()
}
