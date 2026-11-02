import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler, hash } from './lib/headless-three.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'

await ensureFemalePreview('animations')
const bodyPath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const directory = 'assets/items/valkyrie_starter/tripo_boots_v1/fitted_v1'
const partPath = `${directory}/boots_female_valkyrie.glb`
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { scene: body, animations: clips } = await load(bodyPath)
  for (const data of JSON.parse(readFileSync('assets/modular_human_female_01/preview/animations.json')).clips) {
    const clip = THREE.AnimationClip.parse(data)
    const index = clips.findIndex(entry => entry.name === clip.name)
    if (index < 0) clips.push(clip)
    else clips[index] = clip
  }
  let bodyMesh
  body.traverse(node => { if (node.isSkinnedMesh && !bodyMesh) bodyMesh = node })
  const { scene: asset } = await load(partPath)
  assert.ok(asset.userData.heel_pose?.local_quaternion_offsets)
  const parts = bindModularPart(body, asset)
  assert.equal(parts.length, 2)
  body.updateMatrixWorld(true)
  let maxRestError = 0
  let triangles = 0
  for (const mesh of parts) {
    assert.equal(mesh.skeleton, bodyMesh.skeleton)
    const side = mesh.name.endsWith('_right') ? 'Right' : 'Left'
    const { position, skinIndex, skinWeight } = mesh.geometry.attributes
    triangles += mesh.geometry.index.count / 3
    for (let i = 0; i < position.count; i++) {
      let total = 0
      for (let j = 0; j < 4; j++) {
        const weight = skinWeight.getComponent(i, j)
        total += weight
        if (weight) assert.ok(['Hips', side+'UpLeg', side+'Leg', side+'Foot', side+'ToeBase'].includes(mesh.skeleton.bones[skinIndex.getComponent(i, j)].name))
      }
      assert.ok(Math.abs(total - 1) < 1e-5)
      const original = new THREE.Vector3().fromBufferAttribute(position, i)
      maxRestError = Math.max(maxRestError, original.distanceTo(mesh.getVertexPosition(i, new THREE.Vector3())))
    }
  }
  assert.ok(maxRestError < 1e-5)
  const sample = clipSampler(body, { samples: 13, restore: bodyMesh.skeleton.bones })
  const results = []
  for (const name of ['idle1', 'walk', 'run']) {
    const clip = clips.find(c => c.name === name)
    assert.ok(clip, name)
    for (const _ of sample(clip)) for (const mesh of parts) {
      for (let i = 0; i < mesh.geometry.attributes.position.count; i++) {
        assert.ok(mesh.getVertexPosition(i, new THREE.Vector3()).toArray().every(Number.isFinite))
      }
    }
    results.push({ name, samples: 13 })
  }
  const report = { body_sha256: hash(bodyPath), boots_sha256: hash(partPath), triangles, shared_skeleton: true,
    normalized_weights: true, maximum_rest_error_m: maxRestError, clips: results,
    scope: 'Numeric binding and finite skinning checks; collisions are not measured.' }
  writeFileSync(`${directory}/rig-validation.json`, JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report))
} finally { await server.close() }
