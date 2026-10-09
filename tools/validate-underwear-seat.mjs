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
  for (const name of ['top_linen', 'pants_cloth', 'pants_plate', 'pants_barbarian'])
    parts.set(name, bindModularPart(body, (await load(`assets/modular_human_male_01/fitted/${name}.glb`)).scene))
  const originals = skin.map(mesh => mesh.geometry)
  const outfit = { hair: 'none', top: 'none', pants: 'none', gloves: 'none', boots: 'none', helmet: 'none' }
  showModularOutfit(skin, parts, outfit)
  const reshaped = skin.map(mesh => mesh.geometry)
  const seams = new Map()
  let modified = 0, preservedTriangles = 0
  skin.forEach((mesh, m) => {
    const original = originals[m], changed = mesh.geometry
    if (!['torso', 'legs'].includes(region(mesh))) assert.equal(changed, original)
    for (const name of ['uv', 'skinIndex', 'skinWeight'])
      assert.deepEqual(changed.attributes[name].array, original.attributes[name].array)
    assert.deepEqual(changed.index.array, original.index.array)
    preservedTriangles += changed.index.count / 3
    const a = original.attributes.position, b = changed.attributes.position
    for (let i = 0; i < a.count; i++) {
      assert.equal(b.getX(i), a.getX(i)); assert.equal(b.getY(i), a.getY(i))
      if (a.getZ(i) >= -.005 || a.getY(i) <= .76 || a.getY(i) >= 1.2) assert.equal(b.getZ(i), a.getZ(i))
      if (a.getZ(i) !== b.getZ(i)) modified++
      if (['torso', 'legs'].includes(region(mesh))) {
        const key = [a.getX(i), a.getY(i), a.getZ(i)].map(v => Math.round(v * 1e6)).join(',')
        const group = seams.get(key) ?? []
        group.push(new THREE.Vector3().fromBufferAttribute(b, i)); seams.set(key, group)
      }
    }
  })
  assert.ok(modified > 0)
  let seamSeparation = 0
  for (const points of seams.values())
    for (const point of points) seamSeparation = Math.max(seamSeparation, point.distanceTo(points[0]))
  assert.ok(seamSeparation < 2e-6)
  const clips = await loadClips(body, load, modularAnimationClips)
  const sampler = clipSampler(body, { samples: 13, restore: skin[0].skeleton.bones })
  const point = new THREE.Vector3(), motion = []
  for (const clip of clips) {
    for (const { time } of sampler(clip)) {
      skin[0].skeleton.update()
      for (const mesh of skin.filter(mesh => ['torso', 'legs'].includes(region(mesh))))
        for (let i = 0; i < mesh.geometry.attributes.position.count; i++) {
          mesh.getVertexPosition(i, point)
          assert.ok(point.toArray().every(Number.isFinite), `${clip.name} at ${time}`)
        }
    }
    motion.push({ clip: clip.name, finite_vertices: true, samples: 13 })
  }
  for (const pants of ['cloth', 'plate', 'barbarian']) {
    showModularOutfit(skin, parts, { ...outfit, pants })
    skin.forEach((mesh, i) => assert.equal(mesh.geometry, originals[i]))
    showModularOutfit(skin, parts, outfit)
    skin.forEach((mesh, i) => assert.equal(mesh.geometry, reshaped[i]))
  }
  showModularOutfit(skin, parts, { ...outfit, top: 'linen' })
  skin.forEach((mesh, i) => {
    if (region(mesh) === 'legs') assert.equal(mesh.geometry, reshaped[i])
    if (region(mesh) === 'torso') {
      mesh.geometry.computeBoundingBox()
      assert.ok(mesh.geometry.boundingBox.max.y <= 1.090001)
    }
  })
  showModularOutfit(skin, parts, outfit)
  skin.forEach((mesh, i) => assert.equal(mesh.geometry, reshaped[i]))
  const report = { date: '2026-10-08', sources, motion, modified_vertices: modified, maximum_rest_seam_separation_m: seamSeparation, preserved_body_triangles: preservedTriangles, checks: ['UV, indices, skin attributes and X/Y coordinates preserved', 'Front, lower legs, upper torso and other body regions preserved', 'Cloth, plate and barbarian restore source body geometry', 'Underwear reselection reuses the same geometry', 'Linen waist clipping composes with seat shaping'], scope: '91 finite-vertex animation samples and rest seam continuity; full surface collisions not certified.' }
  writeFileSync(new URL('doc/assets/modular-underwear-seat-validation-v1.json', root), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report))
} finally { await server.close() }
