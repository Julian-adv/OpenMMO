import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler } from './lib/headless-three.mjs'

const directory = 'assets/modular_human_female_01/faces/tripo_custom_v1/'
const fitting = JSON.parse(readFileSync(new URL('../' + directory + 'fitting.json', import.meta.url), 'utf8'))
assert.equal(fitting.neck_seams, 1)
assert.equal(fitting.neck_bridge_triangles, 0)
assert.equal(fitting.new_head_scale_and_translation_only, true)
assert.ok(fitting.maximum_head_affine_error_m < 1e-12)
if (fitting.original_head_alpha_only) {
  assert.equal(fitting.geometry_rig_and_animations_unchanged, true)
  assert.equal(fitting.original_lash_rgb_preserved, true)
  assert.equal(fitting.eyelashes.positions_normals_weights_and_triangles_preserved, true)
} else if (fitting.neck_fit_only) {
  assert.equal(fitting.source_head_material_preserved, true)
  assert.equal(fitting.eye_and_eyelash_edits_applied, false)
} else if (fitting.reference_eye_rebuild) {
  assert.equal(fitting.reference_eye_rebuild.original_head_positions_preserved_outside_eye_region, true)
  assert.equal(fitting.reference_eye_rebuild.body_and_neck_geometry_preserved, true)
  assert.equal(fitting.eyelashes.alpha_mode, 'BLEND')
} else {
  assert.ok(fitting.eyelashes.upper_root_attachment.maximum_adjustment_m < .009)
  assert.equal(fitting.eyelashes.upper_root_attachment.original_face_positions_preserved, true)
  assert.equal(fitting.eyelashes.upper_root_attachment.other_lash_positions_preserved, true)
  assert.ok(fitting.eyelashes.lower_root_attachment.maximum_adjustment_m < .009)
  assert.equal(fitting.eyelashes.lower_root_attachment.original_face_positions_preserved, true)
  assert.equal(fitting.eyelashes.lower_root_attachment.other_lash_positions_preserved, true)
}
const { server, load, sources } = await headlessThree()
try {
  const { scene: body, animations } = await load(directory + 'base_custom.glb')
  const meshes = []
  body.traverse(object => { if (object.isSkinnedMesh) meshes.push(object) })
  assert.equal(meshes.length, fitting.neck_fit_only ? 2 : 3)
  assert.ok(!body.getObjectByName('face_neck_bridge'))
  const bones = meshes[0].skeleton.bones
  assert.equal(bones.length, 65)
  const head = bones.find(bone => bone.name === 'Head')
  const boundary = new Map()
  const faceMeshes = meshes.filter(mesh => ['face_female_custom', 'eyelashes_female_custom'].includes(mesh.name))
  assert.equal(faceMeshes.length, fitting.neck_fit_only ? 1 : 2)
  const lashMesh = meshes.find(mesh => mesh.name === 'eyelashes_female_custom')
  if (!fitting.neck_fit_only)
    assert.equal(lashMesh.geometry.index.count / 3, fitting.eyelashes.triangles)
  const source = await load(directory + 'source.glb')
  let sourceMesh
  source.scene.traverse(object => { if (object.isMesh) sourceMesh = object })
  const triangleKey = (mesh, triangle, vScale = 1) => {
    const { index, attributes: { uv } } = mesh.geometry
    return [0, 1, 2].map(corner => {
      const i = index.getX(triangle * 3 + corner)
      return `${Math.round(uv.getX(i) * 1e6)},${Math.round(uv.getY(i) * vScale * 1e6)}`
    }).sort().join('|')
  }
  const triangleKeys = (mesh, vScale = 1) => new Set(Array.from({ length: mesh.geometry.index.count / 3 }, (_, i) => triangleKey(mesh, i, vScale)))
  const lashTriangles = lashMesh ? triangleKeys(lashMesh) : new Set()
  const opaqueTriangles = triangleKeys(meshes.find(mesh => mesh.name === 'face_female_custom'), fitting.reference_eye_rebuild ? 2576 / 2048 : 1)
  const rearSideFaces = [10013, 10014, 10021, 10023, 10071, 10072, 29166, 29168, 29179, 29213]
  const upperBackFaces = [1451, 10093, 10094, 10095, 10096, 10119, 10120, 10871, 10872, 10873, 10874,
    29169, 29170, 29171, 29172, 29173, 29174, 29237, 29238, 29239, 29240, 29259, 29260,
    29441, 29442, 29561, 29562, 29605, 29606, 29607, 29608, 29643, 29644, 29671, 29672, 29703, 29704]
  for (const triangle of new Set([...rearSideFaces, ...upperBackFaces, ...(fitting.protected_eye_source_faces ?? [])])) {
    const key = triangleKey(sourceMesh, triangle)
    if (fitting.neck_fit_only || fitting.protected_eye_source_faces?.includes(triangle)) {
      assert.ok(opaqueTriangles.has(key), `Original face ${triangle} must be restored`)
      assert.ok(!lashTriangles.has(key), `Original eye face ${triangle} must not use eyelash alpha`)
      continue
    }
    if (!fitting.reference_eye_rebuild)
      assert.ok(lashTriangles.has(key), `Eyelash face ${triangle} must use the alpha material`)
    assert.ok(!opaqueTriangles.has(key), `Eyelash face ${triangle} must not remain opaque`)
  }
  const rigid = []
  body.updateMatrixWorld(true)
  const inverseRest = bones.map(bone => bone.matrixWorld.clone().invert())
  for (const mesh of meshes) {
    assert.deepEqual(mesh.skeleton.bones.map(bone => bone.name), bones.map(bone => bone.name))
    const { position, normal, skinWeight: weights, skinIndex: joints } = mesh.geometry.attributes
    for (let i = 0; i < position.count; i++) {
      const rest = new THREE.Vector3().fromBufferAttribute(position, i)
      assert.ok(mesh.getVertexPosition(i, new THREE.Vector3()).distanceTo(rest) < 3e-6)
      assert.ok(Math.abs(weights.getX(i) + weights.getY(i) + weights.getZ(i) + weights.getW(i) - 1) < 2e-6)
      assert.ok([normal.getX(i), normal.getY(i), normal.getZ(i)].every(Number.isFinite))
      if (rest.y > 1.54 && rest.y < 1.64) {
        const key = rest.toArray().map(value => Math.round(value * 1e6)).join(',')
        if (!boundary.has(key)) boundary.set(key, [])
        boundary.get(key).push({ mesh, i })
      }
      if (faceMeshes.includes(mesh) && rest.y > 1.611) {
        assert.equal(bones[joints.getX(i)].name, 'Head')
        assert.ok(weights.getX(i) > .999999)
        rigid.push({ mesh, i, local: rest.applyMatrix4(head.matrixWorld.clone().invert()) })
      }
    }
  }
  const seams = [...boundary.values()].filter(group => new Set(group.map(ref => ref.mesh)).size > 1)
  assert.ok(seams.length > 80)
  for (const refs of seams)
    for (const ref of refs)
      assert.ok(Math.abs(ref.mesh.geometry.attributes.position.getY(ref.i) - fitting.neck_cut_y_m) < 1e-6)
  const sampler = clipSampler(body, { samples: 25, restore: bones })
  const clips = [], snapshots = []
  for (const clip of animations) {
    let seamError = 0, rigidError = 0
    for (const { sample, time } of sampler(clip)) {
      for (const mesh of meshes) {
        const count = mesh.geometry.attributes.position.count
        for (let i = 0; i < count; i++)
          assert.ok(mesh.getVertexPosition(i, new THREE.Vector3()).toArray().every(Number.isFinite))
      }
      for (const refs of seams) {
        const first = refs[0].mesh.getVertexPosition(refs[0].i, new THREE.Vector3())
        for (const ref of refs.slice(1))
          seamError = Math.max(seamError, first.distanceTo(ref.mesh.getVertexPosition(ref.i, new THREE.Vector3())))
      }
      for (const { mesh, i, local } of rigid)
        rigidError = Math.max(rigidError, mesh.getVertexPosition(i, new THREE.Vector3()).distanceTo(local.clone().applyMatrix4(head.matrixWorld)))
      if (sample === 10)
        snapshots.push({ clip: clip.name, time, matrices: Object.fromEntries(bones.map((bone, i) => [bone.name, bone.matrixWorld.clone().multiply(inverseRest[i]).toArray()])) })
    }
    assert.ok(seamError < 3e-6, `${clip.name}: neck gap ${seamError}`)
    assert.ok(rigidError < 3e-6, `${clip.name}: face detached ${rigidError}`)
    clips.push({ name: clip.name, samples: 25, maximum_seam_error_m: seamError, maximum_rigid_head_error_m: rigidError })
  }
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const original = await load('assets/modular_human_female_01/rig_v1/female-base.glb')
  const standalone = await load(directory + 'face_custom.glb')
  assert.equal(bindModularPart(original.scene, standalone.scene).length, fitting.neck_fit_only ? 1 : 2)
  const report = { date: '2026-11-07', sources, bones: bones.length, seam_vertex_groups: seams.length,
    rigid_face_vertices: rigid.length, standalone_binding: true, neck_seams: 1,
    reference_eye_rebuild: Boolean(fitting.reference_eye_rebuild),
    neck_fit_only: Boolean(fitting.neck_fit_only),
    source_lash_faces_absent_from_opaque_head: fitting.neck_fit_only ? [] : [...rearSideFaces, ...upperBackFaces].filter(triangle => !fitting.protected_eye_source_faces?.includes(triangle)),
    original_eye_and_eyelash_faces_restored: Boolean(fitting.neck_fit_only || fitting.original_head_alpha_only),
    original_head_alpha_only: Boolean(fitting.original_head_alpha_only),
    protected_eye_source_faces: fitting.protected_eye_source_faces ?? [],
    upper_lash_roots_fitted: !fitting.neck_fit_only && !fitting.original_head_alpha_only,
    lower_lash_roots_fitted: !fitting.neck_fit_only && !fitting.original_head_alpha_only, head_affine_scope: fitting.head_affine_scope,
    new_head_scale_and_translation_only: true, maximum_head_affine_error_m: fitting.maximum_head_affine_error_m, clips,
    scope: 'Rest binding, normalized weights, finite deformation, rigid head tracking and neck seams. Hair and equipment intersections require visual review.' }
  writeFileSync(new URL('../doc/assets/modular-female-custom-face-animation-v1.json', import.meta.url), JSON.stringify(report, null, 2) + '\n')
  writeFileSync(new URL('../' + directory + 'animation-snapshots.json', import.meta.url), JSON.stringify(snapshots) + '\n')
  console.log(JSON.stringify(report))
} finally {
  await server.close()
}
