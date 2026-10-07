import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { fileURLToPath } from 'node:url'
import * as THREE from '../../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from '../../client/node_modules/three/examples/jsm/libs/meshopt_decoder.module.js'
import { createServer } from '../../client/node_modules/vite/dist/node/index.js'

export const root = new URL('../../', import.meta.url)

export const hash = (path) => createHash('sha256').update(readFileSync(new URL(path, root))).digest('hex')

export const CANONICAL_CLIPS = [
  ['assets/modular_human_male_01/rigged_hand_tuned/animations.glb', ['idle1', 'walk', 'run', 'jump', 'combat_idle', 'slash1']],
  ['client/public/models/characters/modular_male/animations/social.glb', ['sit_idle']],
]
export const CANONICAL_CLIPS_WITH_DYING = [[CANONICAL_CLIPS[0][0], [...CANONICAL_CLIPS[0][1], 'dying']], CANONICAL_CLIPS[1]]

// Vite SSR loads the client's TypeScript; materials are stubbed because Node has no image decoder.
export async function headlessThree({ material = () => new THREE.MeshBasicMaterial(), meshopt = false } = {}) {
  const server = await createServer({
    root: fileURLToPath(new URL('client/', root)), configFile: false,
    optimizeDeps: { noDiscovery: true, include: [] },
    server: { middlewareMode: true, watch: null }, appType: 'custom',
  })
  globalThis.self = globalThis
  const loader = new GLTFLoader().register(() => ({
    name: 'headless-materials', loadMaterial: async () => material(),
  }))
  if (meshopt) loader.setMeshoptDecoder(MeshoptDecoder)
  const sources = []
  async function load(path) {
    const data = readFileSync(new URL(path, root))
    sources.push({ path, sha256: createHash('sha256').update(data).digest('hex') })
    return loader.parseAsync(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), '')
  }
  return { server, sources, load }
}

export async function loadClips(body, load, modularAnimationClips, packs = CANONICAL_CLIPS) {
  const clips = []
  for (const [path, names] of packs) {
    const animations = modularAnimationClips(body, await load(path), 'corrected')
    for (const name of names) {
      const clip = animations.find(c => c.name === name)
      assert.ok(clip, name)
      clips.push(clip)
    }
  }
  return clips
}

// `restore` resets bones after each clip so the next clip doesn't inherit bones it leaves unanimated.
export function clipSampler(body, { samples = 25, restore } = {}) {
  const mixer = new THREE.AnimationMixer(body)
  const rest = restore?.map(bone => ({ bone, p: bone.position.clone(), q: bone.quaternion.clone(), s: bone.scale.clone() }))
  return function* (clip) {
    mixer.stopAllAction()
    const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1)
    action.clampWhenFinished = true
    action.play()
    for (let sample = 0; sample < samples; sample++) {
      const time = clip.duration * sample / (samples - 1)
      mixer.setTime(time)
      body.updateMatrixWorld(true)
      yield { sample, time }
    }
    if (!rest) return
    action.stop()
    mixer.uncacheClip(clip)
    for (const { bone, p, q, s } of rest) { bone.position.copy(p); bone.quaternion.copy(q); bone.scale.copy(s) }
    body.updateMatrixWorld(true)
  }
}
