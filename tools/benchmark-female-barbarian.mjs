import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, root } from './lib/headless-three.mjs'

const option = (name) => process.argv.find(arg => arg.startsWith(`--${name}=`))?.slice(name.length + 3)
const baseline = option('baseline')
const count = Number(option('count') ?? 1)
const legacy = process.argv.includes('--legacy')
assert.ok(Number.isInteger(count) && count > 0 && count <= 100, 'count must be between 1 and 100')
const overrides = new Map()
if (baseline)
  for (const path of ['client/src/lib/effects/pelt-bend.ts', 'client/src/lib/effects/pelt-rig.ts', 'client/src/lib/utils/femaleCharacter.ts']) {
    overrides.set(fileURLToPath(new URL(path, root)), execFileSync('git', ['show', `${baseline}:${path}`], {
      cwd: fileURLToPath(root), encoding: 'utf8',
    }))
  }
const { server, load, sources } = await headlessThree({ plugins: [{
  name: 'pelt-benchmark-baseline', enforce: 'pre', load: id => overrides.get(id),
}] })
try {
  const { assembleFemaleCharacter, applyFemaleArmor, FEMALE_PARTS, femaleBootsClipName } =
    await server.ssrLoadModule('/src/lib/utils/femaleCharacter.ts')
  const { createCharacterModelRoot } = await server.ssrLoadModule('/src/lib/utils/characterAnimationUtils.ts')
  const { createSecondaryMotionScheduler } = await server.ssrLoadModule('/src/lib/effects/secondary-motion-scheduler.ts')
  const { updatePeltPhysics, disposePeltPhysics } = await server.ssrLoadModule('/src/lib/effects/pelt-rig.ts')
  const directory = 'client/public/models/characters/modular_female'
  const base = await load(`${directory}/base.glb`)
  const source = assembleFemaleCharacter(base, await Promise.all(FEMALE_PARTS.map(name => load(`${directory}/${name}.glb`))))
  const characters = Array.from({ length: count }, (_, index) => {
    const { modelRoot: body } = createCharacterModelRoot(source)
    if (legacy) body.traverse(node => { delete node.userData.robe_physics })
    body.position.set(index % 5, 0, Math.floor(index / 5))
    applyFemaleArmor(body, {
    head: 'worn_barbarian_helmet', chest: 'worn_barbarian_armor', pants: 'worn_barbarian_pants',
    boots: 'worn_barbarian_boots', hands: 'worn_barbarian_bracers',
    })
    return { body, mixer: new THREE.AnimationMixer(body), scheduler: createSecondaryMotionScheduler(index) }
  })
  const timings = []
  for (const name of ['idle1', 'walk', 'run', 'combat_idle', 'slash1', 'sit_idle']) {
    const raw = base.animations.find(clip => clip.name === name)
    const companion = base.animations.find(clip => clip.name === femaleBootsClipName(name, 'boots'))
    assert.ok(raw, name)
    assert.ok(companion, `${name}: heeled boots`)
    const replacements = new Map(companion.tracks.map(track => [track.name, track]))
    const clip = new THREE.AnimationClip(name, raw.duration, raw.tracks.map(track => replacements.get(track.name) ?? track))
    for (const { body, mixer } of characters) {
      mixer.stopAllAction()
      mixer.clipAction(clip).reset().play()
      mixer.update(0)
      updatePeltPhysics(body, 0, null, 1 / 60, true)
    }
    const samples = []
    const frameSamples = []
    for (let frame = 0; frame < 150; frame++) {
      const frameStart = performance.now()
      let physicsMs = 0
      for (const [index, { body, mixer, scheduler }] of characters.entries()) {
        mixer.update(1 / 60)
        const physics = scheduler.next(1 / 60, true, index === 0)
        if (!physics) continue
        const start = performance.now()
        updatePeltPhysics(body, physics.deltaTime, null, physics.step, physics.reset)
        physicsMs += performance.now() - start
      }
      if (frame >= 30) {
        samples.push(physicsMs)
        frameSamples.push(performance.now() - frameStart)
      }
    }
    samples.sort((a, b) => a - b)
    timings.push({ clip: name, frames: samples.length,
      mean_ms: samples.reduce((a, b) => a + b) / samples.length,
      median_ms: samples[Math.floor(samples.length / 2)],
      p95_ms: samples[Math.floor(samples.length * .95)],
      animation_and_physics_mean_ms: frameSamples.reduce((a, b) => a + b) / frameSamples.length,
    })
  }
  const report = { baseline: baseline ?? null, legacy, characters: count, runtime: process.version,
    method: 'Game-scale female barbarians with baked boots tracks; one local at 60 Hz, remaining characters on the game remote scheduler at 30 Hz. 30 warmup and 120 measured frames per clip. mean/median/p95 cover aggregate CPU physics including world matrices; animation_and_physics_mean_ms also includes mixers and scheduling. Rendering is excluded; these are CPU costs, not game FPS.',
    sources, timings }
  const output = option('output')
  if (output) writeFileSync(output, JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report))
  for (const { body, mixer } of characters) {
    disposePeltPhysics(body)
    mixer.uncacheRoot(body)
  }
} finally {
  await server.close()
}
