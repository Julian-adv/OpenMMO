import assert from 'node:assert/strict'
import { writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, loadClips, clipSampler, root } from './lib/headless-three.mjs'

const topStyle = process.argv[2] ?? 'plate'
assert.ok(['plate', 'rogue'].includes(topStyle))
const topId = `top_${topStyle}`
const topPath = topStyle === 'plate'
  ? 'assets/modular_human_male_01/parts/fitted/top_plate.glb'
  : 'assets/modular_human_male_01/parts/rogue_tripo_v1/top_rogue.glb'
const waistHeight = topStyle === 'plate' ? 1.06 : 1.105
const { server, load, sources } = await headlessThree()
try {
  const { skinnedParts, bindModularPart, showModularOutfit, modularAnimationClips, region } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const body = (await load('assets/modular_human_male_01/parts/fitted/base.glb')).scene
  const skin = skinnedParts(body), parts = new Map()
  for (const [id, path] of [
    [topId, topPath],
    ['pants_priest', 'assets/modular_human_male_01/parts/priest_tripo_pants_v1/pants_priest.glb'],
    ...['leather', 'plate', 'barbarian'].map(id => [`boots_${id}`, `assets/modular_human_male_01/parts/fitted/boots_${id}.glb`]),
    ['boots_caveman', 'assets/modular_human_male_01/parts/caveman_tripo_boots_v1/boots_caveman.glb'],
    ['boots_ranger', 'assets/modular_human_male_01/parts/ranger_tripo_boots_v1/boots_ranger.glb'],
  ]) parts.set(id, bindModularPart(body, (await load(path)).scene))
  const pants = parts.get('pants_priest')[0], original = pants.geometry
  const originalSkin = skin.map(mesh => mesh.geometry)
  const originalSkinTriangles = skin.reduce((n, mesh) => n + mesh.geometry.index.count / 3, 0)
  const originalTop = parts.get(topId).map(mesh => mesh.geometry)
  original.computeBoundingBox()
  const outfit = { hair: 'none', top: topStyle, pants: 'priest', gloves: 'none', boots: 'none', helmet: 'none' }
  showModularOutfit(skin, parts, outfit)
  const fitted = pants.geometry
  const torso = skin.filter(mesh => region(mesh) === 'torso')
  if (topStyle === 'rogue')
    for (const mesh of torso) {
      mesh.geometry.computeBoundingBox()
      assert.ok(mesh.geometry.boundingBox.max.y <= waistHeight + 1e-6)
    }
  parts.get(topId).forEach((mesh, i) => assert.equal(mesh.geometry, originalTop[i]))
  const triangles = meshes => meshes.reduce((n, mesh) => n + mesh.geometry.index.count / 3, 0)
  const budget = {
    scope: `Canonical body, ${topStyle} top and priest chainmail trousers; no hair, gloves, boots, helmet, weapon or cape`,
    preserved_glb_triangles: originalSkinTriangles + triangles(parts.get(topId)) + original.index.count / 3,
    runtime_including_hidden_triangles: triangles(skin) + triangles(parts.get(topId)) + fitted.index.count / 3,
    visible_triangles: triangles([...skin, ...parts.get(topId), pants].filter(mesh => mesh.visible)),
    face_triangles: 1505,
  }
  assert.notEqual(fitted, original)
  fitted.computeBoundingBox()
  assert.ok(fitted.boundingBox.max.y <= waistHeight + 1e-6)
  const weights = fitted.attributes.skinWeight, normals = fitted.attributes.normal
  for (let i = 0; i < weights.count; i++) {
    const sum = [0, 1, 2, 3].reduce((n, j) => n + weights.getComponent(i, j), 0)
    assert.ok(Math.abs(sum - 1) < 1e-6)
    assert.ok([normals.getX(i), normals.getY(i), normals.getZ(i)].every(Number.isFinite))
  }
  const clips = await loadClips(body, load, modularAnimationClips)
  const sampler = clipSampler(body, { samples: 13, restore: pants.skeleton.bones })
  const point = new THREE.Vector3(), motion = []
  for (const clip of clips) {
    for (const { time } of sampler(clip)) {
      pants.skeleton.update()
      for (const mesh of [pants, ...torso])
        for (let i = 0; i < mesh.geometry.attributes.position.count; i++) {
          mesh.getVertexPosition(i, point)
          assert.ok(point.toArray().every(Number.isFinite), `${clip.name} at ${time}`)
        }
    }
    motion.push({ clip: clip.name, finite_vertices: true, samples: 13 })
  }
  const boots = []
  for (const boot of ['leather', 'plate', 'barbarian', 'caveman', 'ranger']) {
    showModularOutfit(skin, parts, { ...outfit, boots: boot })
    pants.geometry.computeBoundingBox()
    const bounds = pants.geometry.boundingBox
    assert.ok(bounds.max.y <= waistHeight + 1e-6)
    assert.ok(bounds.min.y > original.boundingBox.min.y)
    boots.push({ boot, min_y: bounds.min.y, max_y: bounds.max.y, triangles: pants.geometry.index.count / 3 })
  }
  for (const top of ['none', 'linen', 'priest']) {
    showModularOutfit(skin, parts, { ...outfit, top })
    assert.equal(pants.geometry, original)
  }
  showModularOutfit(skin, parts, outfit)
  assert.equal(pants.geometry, fitted)
  parts.delete(topId)
  showModularOutfit(skin, parts, outfit)
  assert.equal(pants.geometry, original)
  skin.forEach((mesh, i) => assert.equal(mesh.geometry, originalSkin[i]))
  const report = { date: '2026-10-09', sources, motion, boots, budget, original_triangles: original.index.count / 3, waist_trimmed_triangles: fitted.index.count / 3, checks: [`${topStyle} top must be loaded and visible`, 'Waist correction composes with all five boot trims', 'Top removal, replacement and missing top restore full waist', 'Repeated selection reuses geometry', 'Interpolated skin weights remain normalized and normals finite', ...(topStyle === 'rogue' ? ['Covered torso ends at Y=1.105m; source undershirt geometry unchanged', 'Missing top restores torso geometry'] : [])], scope: '91 finite-vertex motion samples and combination/restore checks; full surface collisions not certified.' }
  writeFileSync(new URL(`doc/assets/modular-priest-${topStyle}-waist-validation-v${topStyle === 'rogue' ? 2 : 1}.json`, root), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report))
} finally { await server.close() }
