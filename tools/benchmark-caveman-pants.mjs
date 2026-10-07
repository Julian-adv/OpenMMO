import assert from 'node:assert/strict'
import { writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree } from './lib/headless-three.mjs'

const { server, sources, load } = await headlessThree()
try {
  const { bindModularPart, modularAnimationClips } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { updatePeltPhysics, resetPeltPhysics, disposePeltPhysics } = await server.ssrLoadModule('/src/lib/effects/pelt-rig.ts')
  const results = []
  const baseline = process.argv[2]
  for (const [name, path] of [
    ...(baseline ? [['previous_caveman', baseline]] : []),
    ['barbarian', 'assets/modular_human_male_01/parts/fitted/pants_barbarian.glb'],
    ['rebuilt_caveman', 'assets/modular_human_male_01/parts/caveman_tripo_pants_v1/pants_caveman.glb'],
  ]) {
    const body = (await load('assets/modular_human_male_01/parts/fitted/base.glb')).scene
    const meshes = bindModularPart(body, (await load(path)).scene)
    if (name === 'rebuilt_caveman') {
      assert.ok(meshes.every(mesh => !mesh.userData.pelt_physics?.bend))
      assert.equal(meshes.filter(mesh => mesh.userData.pelt_physics?.cloth).length, 2)
    }
    const mixer = new THREE.AnimationMixer(body)
    const timings = []
    for (const [pack, clipName] of [
      ['assets/modular_human_male_01/rigged_hand_tuned/animations.glb', 'walk'],
      ['assets/modular_human_male_01/rigged_hand_tuned/animations.glb', 'run'],
      ['client/public/models/characters/modular_male/animations/social.glb', 'sit_idle'],
    ]) {
      mixer.stopAllAction()
      const clip = modularAnimationClips(body, await load(pack), 'corrected').find(clip => clip.name === clipName)
      assert.ok(clip, clipName)
      mixer.clipAction(clip).reset().play()
      mixer.update(0)
      body.updateMatrixWorld(true)
      updatePeltPhysics(body, 0)
      resetPeltPhysics(body)
      const samples = []
      for (let frame = 0; frame < 132; frame++) {
        mixer.update(1 / 60)
        body.updateMatrixWorld(true)
        const start = performance.now()
        updatePeltPhysics(body, 1 / 60)
        const elapsed = performance.now() - start
        if (frame >= 12) samples.push(elapsed)
      }
      samples.sort((a, b) => a - b)
      timings.push({ clip: clipName, frames: samples.length,
        mean_ms: samples.reduce((a, b) => a + b) / samples.length,
        median_ms: samples[Math.floor(samples.length / 2)],
        p95_ms: samples[Math.floor(samples.length * .95)],
      })
    }
    results.push({ name, path, triangles: meshes.reduce((sum, mesh) => sum + mesh.geometry.index.count / 3, 0), timings })
    console.log(JSON.stringify(results.at(-1)))
    disposePeltPhysics(body)
    mixer.uncacheRoot(body)
  }
  const report = { date: '2026-10-04', runtime: process.version,
    method: 'Same process, canonical rig and animations at 60 Hz; 12 warmup and 120 measured frames per clip. Times cover updatePeltPhysics only, excluding animation, rendering and matrix updates. These are CPU physics costs, not game FPS.',
    sources, results }
  writeFileSync('/tmp/caveman-pants-performance.json', JSON.stringify(report, null, 2) + '\n')
} finally {
  await server.close()
}
