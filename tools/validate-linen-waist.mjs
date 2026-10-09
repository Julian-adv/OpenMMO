import assert from 'node:assert/strict'
import { writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, loadClips, clipSampler, root } from './lib/headless-three.mjs'

const { server, load, sources } = await headlessThree()
try {
  const { skinnedParts, bindModularPart, showModularOutfit, modularAnimationClips, region } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const body = (await load('assets/modular_human_male_01/fitted/base.glb')).scene
  const skin = skinnedParts(body)
  const parts = new Map()
  for (const name of ['top_linen', 'pants_cloth', 'pants_plate'])
    parts.set(name, bindModularPart(body, (await load(`assets/modular_human_male_01/fitted/${name}.glb`)).scene))
  const torso = skin.filter(mesh => region(mesh) === 'torso')
  const shirt = parts.get('top_linen').find(mesh => region(mesh) === 'torso')
  const originalShirt = shirt.geometry
  const originalSkin = torso.map(mesh => mesh.geometry)
  const outfit = { hair: 'none', top: 'linen', pants: 'none', gloves: 'none', boots: 'none', helmet: 'none' }
  showModularOutfit(skin, parts, { ...outfit, top: 'none' })
  const underwearSkin = torso.map(mesh => mesh.geometry)
  showModularOutfit(skin, parts, outfit)
  assert.ok(torso.every(mesh => mesh.visible))
  for (const attr of ['uv', 'skinIndex', 'skinWeight'])
    assert.deepEqual(shirt.geometry.attributes[attr].array, originalShirt.attributes[attr].array)
  assert.deepEqual(shirt.geometry.index.array, originalShirt.index.array)
  const positions = shirt.geometry.attributes.position
  const sourcePosition = originalShirt.attributes.position
  for (let i = 0; i < positions.count; i++) {
    assert.equal(positions.getY(i), sourcePosition.getY(i))
    if (positions.getY(i) >= 1.2)
      for (let j = 0; j < 3; j++) {
        assert.equal(positions.getComponent(i, j), sourcePosition.getComponent(i, j))
        assert.equal(shirt.geometry.attributes.normal.getComponent(i, j), originalShirt.attributes.normal.getComponent(i, j))
      }
  }
  assert.ok(Array.from(shirt.geometry.attributes.normal.array).every(Number.isFinite))
  let degenerate = 0
  const a = new THREE.Vector3(), b = new THREE.Vector3(), c = new THREE.Vector3()
  const indices = shirt.geometry.index
  for (let i = 0; i < indices.count; i += 3) {
    a.fromBufferAttribute(positions, indices.getX(i))
    b.fromBufferAttribute(positions, indices.getX(i + 1))
    c.fromBufferAttribute(positions, indices.getX(i + 2))
    if (b.sub(a).cross(c.sub(a)).length() < 1e-12) degenerate++
  }
  assert.equal(degenerate, 0)
  const hem = Array.from({ length: positions.count }, (_, i) => i).filter(i => positions.getY(i) < 1.069)
  const caps = torso.flatMap(mesh => {
    const p = mesh.geometry.attributes.position
    return Array.from({ length: p.count }, (_, i) => i).filter(i => Math.abs(p.getY(i) - 1.09) < 1e-6).map(index => ({ mesh, index }))
  })
  assert.ok(hem.length && caps.length)
  const hips = shirt.skeleton.bones.find(bone => bone.name === 'Hips')
  const inverse = new THREE.Matrix4(), point = new THREE.Vector3()
  const sampler = clipSampler(body, { samples: 13, restore: shirt.skeleton.bones })
  const clips = await loadClips(body, load, modularAnimationClips)
  const motion = []
  for (const clip of clips) {
    let overlap = Infinity
    for (const { time } of sampler(clip)) {
      shirt.skeleton.update()
      inverse.copy(hips.matrixWorld).invert()
      let hemTop = -Infinity, capBottom = Infinity
      for (let i = 0; i < positions.count; i++) {
        shirt.getVertexPosition(i, point).applyMatrix4(shirt.matrixWorld).applyMatrix4(inverse)
        assert.ok(point.toArray().every(Number.isFinite), `${clip.name} at ${time}`)
      }
      for (const index of hem) {
        shirt.getVertexPosition(index, point).applyMatrix4(shirt.matrixWorld).applyMatrix4(inverse)
        hemTop = Math.max(hemTop, point.y)
      }
      for (const { mesh, index } of caps) {
        mesh.getVertexPosition(index, point).applyMatrix4(mesh.matrixWorld).applyMatrix4(inverse)
        capBottom = Math.min(capBottom, point.y)
      }
      overlap = Math.min(overlap, capBottom - hemTop)
    }
    assert.ok(overlap >= 0, `${clip.name}: waist vertical gap ${overlap}`)
    motion.push({ clip: clip.name, minimum_vertical_overlap_m: overlap, finite_vertices: true })
  }
  const opened = shirt.geometry
  for (const pants of ['cloth', 'plate']) {
    showModularOutfit(skin, parts, { ...outfit, pants })
    assert.equal(shirt.geometry, originalShirt)
    torso.forEach((mesh, i) => { assert.equal(mesh.geometry, originalSkin[i]); assert.equal(mesh.visible, false) })
    showModularOutfit(skin, parts, outfit)
    assert.equal(shirt.geometry, opened)
  }
  showModularOutfit(skin, parts, { ...outfit, top: 'none' })
  torso.forEach((mesh, i) => { assert.equal(mesh.geometry, underwearSkin[i]); assert.equal(mesh.visible, true) })
  const report = {
    date: '2026-10-08', sources, samples_per_clip: 13, motion,
    preservation: ['GLB source files unchanged', 'shirt indices, UVs and skin attributes', 'shirt Y coordinates', 'shirt shape and custom normals above Y=1.2m', 'sleeves and collar'],
    degenerate_faces: degenerate,
    checks: ['Cloth and plate trousers restore the tucked shirt and body mask', 'Shirt removal restores the complete torso with the underwear seat shape', 'Repeated selection reuses the same geometry'],
    scope: 'Conservative vertical overlap of lower shirt vertices and clipped waist edge in the moving Hips frame; finite shirt vertices. This is not full triangle collision validation.'
  }
  writeFileSync(new URL('doc/assets/modular-linen-underwear-waist-validation-v2.json', root), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(motion))
} finally { await server.close() }
