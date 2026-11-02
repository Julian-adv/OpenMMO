import assert from 'node:assert/strict'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { NodeIO, VertexLayout } from '@gltf-transform/core'
import { ALL_EXTENSIONS } from '@gltf-transform/extensions'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler } from './lib/headless-three.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'

const root = new URL('../', import.meta.url)
const settingsPath = 'doc/assets/valkyrie-bracer-preview-fit.json'
const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const directory = 'assets/items/valkyrie_gloves/fitted_v3'
const snapshot = JSON.parse(await readFile(new URL(settingsPath, root), 'utf8'))
const sha = bytes => createHash('sha256').update(bytes).digest('hex')
const input = await readFile(new URL(snapshot.part_path, root))
assert.equal(sha(input), snapshot.part_sha256, 'The saved fitting source changed')
assert.equal(sha(await readFile(new URL('client/src/previews/female-bracer-mask.ts', root))), snapshot.mask_sha256, 'The saved masking rules changed')
assert.equal(snapshot.hide_skin, true, 'This bake expects the approved skin mask')
await ensureFemalePreview('base')
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { femaleOutfitMask } = await server.ssrLoadModule('/src/lib/utils/female-outfit-mask.ts')
  const { applyFemaleBracerFit } = await server.ssrLoadModule('/src/previews/female-bracer-fit.ts')
  const { femaleBracerMask } = await server.ssrLoadModule('/src/previews/female-bracer-mask.ts')
  const base = await load(basePath)
  let body
  base.scene.traverse(node => {
    const association = base.parser.associations.get(node)
    if (node.isSkinnedMesh && association?.meshes === 0 && association.primitives === 0) body = node
  })
  assert.ok(body)
  const originalBody = body.geometry
  const preview = await load('assets/modular_human_female_01/preview/base.glb')
  let previewBody
  preview.scene.traverse(node => {
    const association = preview.parser.associations.get(node)
    if (node.isSkinnedMesh && association?.meshes === 0 && association.primitives === 0) previewBody = node
  })
  assert.ok(previewBody)
  assert.deepEqual(previewBody.geometry.index.array, originalBody.index.array)
  for (let i = 0; i < originalBody.attributes.position.count; i++)
    for (let j = 0; j < 3; j++)
      assert.equal(previewBody.geometry.attributes.position.getComponent(i, j), originalBody.attributes.position.getComponent(i, j))
  const meshes = bindModularPart(base.scene, (await load(snapshot.part_path)).scene)
  const skeleton = meshes[0].skeleton
  const jointIndex = name => {
    const i = skeleton.bones.findIndex(bone => bone.name === name)
    assert.ok(i >= 0, name)
    return i
  }
  const bind = name => new THREE.Vector3().setFromMatrixPosition(skeleton.boneInverses[jointIndex(name)].clone().invert())
  const arm = bind('LeftHand').sub(bind('LeftForeArm')).normalize()
  const outward = new THREE.Vector3(1, 0, 0).addScaledVector(arm, -arm.x).normalize()
  const axes = [outward, new THREE.Vector3().crossVectors(arm, outward).normalize(), arm]
  const parts = meshes.map(mesh => ({
    mesh,
    positions: new Float32Array(mesh.geometry.attributes.position.array),
    normals: new Float32Array(mesh.geometry.attributes.normal.array),
    tangents: mesh.geometry.attributes.tangent && new Float32Array(mesh.geometry.attributes.tangent.array),
    center: new THREE.Vector3().fromArray(mesh.userData.fit_center),
    side: mesh.userData.fit_center[0] < 0 ? -1 : 1,
  }))
  applyFemaleBracerFit(parts, axes, snapshot.settings)
  const sides = parts.map(part => part.side < 0 ? 'Right' : 'Left')
  const segments = sides.map(side => [bind(side + 'ForeArm'), bind(side + 'Hand')])
  const bones = sides.map(side => ['Arm', 'ForeArm', 'Hand'].map(name => jointIndex(side + name)))
  const mask = femaleBracerMask(originalBody, meshes, segments, bones)
  const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).setVertexLayout(VertexLayout.SEPARATE)
  const document = await io.readBinary(input)
  for (const node of document.getRoot().listNodes()) {
    const mesh = node.getMesh()
    if (!mesh) continue
    const expected = meshes.find(part => part.name === node.getName())
    assert.ok(expected, node.getName())
    assert.equal(mesh.listPrimitives().length, 1)
    const primitive = mesh.listPrimitives()[0]
    for (const [semantic, attribute] of [['POSITION', 'position'], ['NORMAL', 'normal'], ['TANGENT', 'tangent']]) {
      if (!expected.geometry.attributes[attribute]) continue
      primitive.getAttribute(semantic).setArray(new Float32Array(expected.geometry.attributes[attribute].array))
    }
    node.setExtras({ ...node.getExtras(), fit_baked: true })
  }
  document.getRoot().getDefaultScene().setExtras({
    ...document.getRoot().getDefaultScene().getExtras(),
    bracer_fit_baked: { settings: snapshot.settings, hide_skin: snapshot.hide_skin, source_sha256: snapshot.part_sha256 },
  })
  const bytes = await io.writeBinary(document)
  const restored = await io.readBinary(bytes)
  assert.deepEqual(restored.getRoot().listTextures().map(t => sha(t.getImage())), document.getRoot().listTextures().map(t => sha(t.getImage())))
  const output = `${directory}/bracers_female_valkyrie.glb`
  await mkdir(new URL(directory + '/', root), { recursive: true })
  await writeFile(new URL(output, root), bytes)
  const baked = bindModularPart(base.scene, (await load(output)).scene)
  assert.equal(baked.length, 2)
  for (const mesh of baked) {
    const expected = meshes.find(part => part.name === mesh.name)
    for (const attribute of ['position', 'normal', 'tangent', 'skinIndex', 'skinWeight', 'uv']) {
      if (!expected.geometry.attributes[attribute]) continue
      assert.deepEqual(mesh.geometry.attributes[attribute].array, expected.geometry.attributes[attribute].array, `${mesh.name}: ${attribute}`)
    }
    assert.deepEqual(mesh.geometry.index.array, expected.geometry.index.array)
    assert.equal(mesh.skeleton.bones.length, 65)
  }
  assert.deepEqual(femaleBracerMask(originalBody, baked, segments, bones), mask)
  const masking = femaleOutfitMask(base)
  masking.register('baked-bracers', mask)
  masking.setEquipped('baked-bracers', true)
  assert.equal(body.geometry.index.count, originalBody.index.count - mask.hide_triangle_indices.length * 3)
  masking.setEquipped('baked-bracers', false)
  assert.equal(body.geometry, originalBody)
  masking.dispose()
  const samples = clipSampler(base.scene, { samples: 9, restore: skeleton.bones })
  const clips = base.animations.filter(clip => ['idle1', 'walk', 'run', 'jump', 'combat_idle', 'slash1', 'sit_idle', 'dying'].includes(clip.name))
  assert.ok(clips.length >= 6)
  let maximumDifference = 0
  for (const clip of clips) {
    for (const _pose of samples(clip)) {
      for (const mesh of baked) {
        const expected = meshes.find(part => part.name === mesh.name)
        for (let i = 0; i < mesh.geometry.attributes.position.count; i++) {
          const a = mesh.getVertexPosition(i, new THREE.Vector3())
          const b = expected.getVertexPosition(i, new THREE.Vector3())
          assert.ok(a.toArray().every(Number.isFinite))
          maximumDifference = Math.max(maximumDifference, a.distanceTo(b))
        }
      }
    }
  }
  assert.ok(maximumDifference < 1e-6)
  const json = value => JSON.stringify(value, null, 2) + '\n'
  await writeFile(new URL(`${directory}/body-occlusion.json`, root), json({ ...mask, body_sha256: sha(await readFile(new URL(basePath, root))) }))
  await writeFile(new URL(`${directory}/saved-fit.json`, root), json(snapshot))
  const report = {
    source_settings: settingsPath, source_settings_sha256: sha(await readFile(new URL(settingsPath, root))),
    part_sha256: sha(bytes), body_sha256: sha(await readFile(new URL(basePath, root))),
    source_sha256: snapshot.part_sha256,
    triangles: baked.reduce((sum, mesh) => sum + mesh.geometry.index.count / 3, 0),
    hidden_body_triangles: mask.hide_triangle_indices.length,
    attributes_identical_to_preview: true, textures_preserved: true, bones: 65,
    animations: clips.map(clip => clip.name), samples_per_animation: 9, maximum_position_difference_m: maximumDifference,
    mask_equivalent: true, preview_body_topology_identical: true, unequip_restores_body: true,
  }
  await writeFile(new URL(`${directory}/baking.json`, root), json(report))
  console.log(json(report))
} finally { await server.close() }
