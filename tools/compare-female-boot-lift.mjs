// Ground lift per clip for female heel boots: node tools/compare-female-boot-lift.mjs <boots.glb>...
import { readFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler } from './lib/headless-three.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'
await ensureFemalePreview('animations')
const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const { server, load } = await headlessThree()
const wanted = ['idle1', 'walk', 'run']
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { femaleBootGround } = await server.ssrLoadModule('/src/lib/utils/female-boot-ground.ts')
  for (const path of process.argv.slice(2)) {
    const base = await load(basePath)
    for (const data of JSON.parse(readFileSync('assets/modular_human_female_01/preview/animations.json')).clips) {
      const clip = THREE.AnimationClip.parse(data)
      const i = base.animations.findIndex(e => e.name === clip.name)
      if (i < 0) base.animations.push(clip); else base.animations[i] = clip
    }
    const asset = await load(path)
    const boots = bindModularPart(base.scene, asset.scene)
    let body
    base.scene.traverse(n => { if (n.isSkinnedMesh && base.parser.associations.get(n)?.primitives === 0) body = n })
    base.scene.updateMatrixWorld(true)
    const ground = femaleBootGround(base.scene, body, boots, asset.scene.userData.heel_pose)
    const sample = clipSampler(base.scene, { samples: 61, restore: body.skeleton.bones })
    const out = {}
    for (const clip of base.animations.filter(c => wanted.includes(c.name))) {
      let min = Infinity, max = 0
      for (const _ of sample(clip)) { const l = ground.apply(undefined, clip.name); min = Math.min(min, l); max = Math.max(max, l); ground.restore() }
      out[clip.name] = [min.toFixed(4), max.toFixed(4)]
    }
    console.log(path.split('/').slice(-1)[0], JSON.stringify(out))
  }
} finally { await server.close() }
