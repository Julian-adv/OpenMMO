import { test, expect, vi } from 'vitest'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'

vi.mock('../src/lib/utils/gltfCache', () => ({ loadGLB: () => { throw new Error('unexpected load') } }))
import { retargetAnimationsForCharacterModel } from '../src/lib/utils/characterAnimationUtils'

const root = new URL('../../', import.meta.url)
const load = async (path: string) => {
  const data = readFileSync(new URL(path, root))
  const loader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder)
    .register(() => ({ name: 'geometry-only', loadMaterial: async () => new THREE.MeshBasicMaterial() }))
  return loader.parseAsync(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), '')
}

const findMesh = (scene: THREE.Object3D): THREE.SkinnedMesh => {
  let result: THREE.SkinnedMesh | undefined
  scene.traverse(o => { if (o instanceof THREE.SkinnedMesh && (!result || o.skeleton.bones.length > result.skeleton.bones.length)) result = o })
  if (!result) throw new Error('missing skin')
  return result
}

test('bake retargeted thief king motion', async () => {
  const target = await load('assets/monsters/thief_king/rig_v1/thief_king.glb')
  const locomotion = await load('client/public/models/animations/locomotion.glb')
  const combat = await load('client/public/models/animations/combat_melee.glb')
  const requested = ['idle1', 'walk', 'run', 'slash1', 'combat_idle', 'dying']
  const clips: THREE.AnimationClip[] = []
  for (const pack of [locomotion, combat]) {
    clips.push(...await retargetAnimationsForCharacterModel(target.scene, pack.scene, pack.animations.filter(c => requested.includes(c.name))))
  }
  expect(clips.map(c => c.name).sort()).toEqual([...requested].sort())
  const mesh = findMesh(target.scene), geometry = mesh.geometry
  const fit = JSON.parse(readFileSync(new URL('assets/monsters/thief_king/grip_v1/fit.json', root), 'utf8'))
  const grip = new Map<string, THREE.Quaternion>()
  for (const [finger, angles] of Object.entries(fit.fingers_degrees_x) as [string, number[]][]) {
    for (let i = 1; i <= 3; i++) {
      const bone = mesh.skeleton.getBoneByName(`RightHand${finger}${i}`)!
      const rotation = bone.quaternion.clone().multiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), THREE.MathUtils.degToRad(angles[i - 1])))
      if (finger === 'Thumb') {
        rotation.multiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), THREE.MathUtils.degToRad(fit.thumb_degrees_y[i - 1])))
        rotation.multiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 0, 1), THREE.MathUtils.degToRad(fit.thumb_degrees_z[i - 1])))
      }
      grip.set(bone.name, rotation)
    }
  }
  const pos = geometry.getAttribute('position'), weights = geometry.getAttribute('skinWeight'), joints = geometry.getAttribute('skinIndex')
  const footJoints = new Set(mesh.skeleton.bones.map((b, i) => /foot|toe/i.test(b.name) ? i : -1))
  const sole: number[] = []
  for (let i = 0; i < pos.count; i++) {
    let influence = 0
    for (let k = 0; k < 4; k++) if (footJoints.has(joints.getComponent(i, k))) influence += weights.getComponent(i, k)
    if (influence > .5) sole.push(i)
  }
  expect(sole.length).toBeGreaterThan(100)
  const sourceFeet: number[] = []
  const sourceMesh = findMesh(locomotion.scene)
  const sourceMixer = new THREE.AnimationMixer(locomotion.scene)
  const run = locomotion.animations.find(c => c.name === 'run')!
  const sourceAction = sourceMixer.clipAction(run).setLoop(THREE.LoopOnce, 1); sourceAction.clampWhenFinished = true; sourceAction.play()
  const sourcePoint = new THREE.Vector3()
  for (let i = 0; i <= Math.ceil(run.duration * 60); i++) {
    sourceMixer.setTime(Math.min(run.duration - 1e-6, i / 60)); locomotion.scene.updateMatrixWorld(true)
    let low = Infinity
    for (const b of sourceMesh.skeleton.bones) if (/foot|toe/i.test(b.name)) low = Math.min(low, b.getWorldPosition(sourcePoint).y)
    sourceFeet.push(low)
  }
  const sourceFloor = Math.min(...sourceFeet)
  const mixer = new THREE.AnimationMixer(target.scene)
  const vector = new THREE.Vector3()
  const output = []
  for (const clip of clips) {
    const action = mixer.clipAction(clip).setLoop(THREE.LoopOnce, 1); action.clampWhenFinished = true; action.play()
    const steps = Math.ceil(clip.duration * 60), times = [], rawHeights: number[] = [], hipPositions: number[][] = []
    const hips = target.scene.getObjectByName('Hips')!
    for (let i = 0; i <= steps; i++) {
      const t = clip.duration * i / steps
      mixer.setTime(Math.min(t, clip.duration - 1e-6))
      for (const [name, rotation] of grip) mesh.skeleton.getBoneByName(name)!.quaternion.copy(rotation)
      target.scene.updateMatrixWorld(true); mesh.skeleton.update()
      let lowest = Infinity
      const ids = clip.name === 'dying' ? [...Array(pos.count).keys()] : sole
      for (const id of ids) { mesh.getVertexPosition(id, vector).applyMatrix4(mesh.matrixWorld); lowest = Math.min(lowest, vector.y) }
      times.push(t); rawHeights.push(lowest); hipPositions.push(hips.position.toArray())
    }
    const finalRestOffset = .002 - rawHeights.at(-1)!
    const values = hipPositions.flatMap((p, i) => {
      let correction
      if (clip.name === 'dying') {
        const blend = THREE.MathUtils.smoothstep(i / steps, .45, 1)
        correction = Math.max(-rawHeights[i] + .002, finalRestOffset * blend)
      } else {
        const flight = clip.name === 'run' ? Math.max(0, sourceFeet[Math.round(i * (sourceFeet.length - 1) / steps)] - sourceFloor - .015) : 0
        correction = .004 + flight - rawHeights[i]
      }
      return [p[0], p[1] + correction, p[2]]
    })
    const tracks = clip.tracks.filter(t => t.name !== 'Hips.position').map(t => ({ name: t.name, times: Array.from(t.times), values: Array.from(t.values), size: t.getValueSize() }))
    tracks.push({ name: 'Hips.position', times, values, size: 3 })
    output.push({ name: clip.name, duration: clip.duration, tracks })
    action.stop(); mixer.uncacheClip(clip)
  }
  mkdirSync('/tmp/thief-king-bake', { recursive: true })
  writeFileSync('/tmp/thief-king-bake/clips.json', JSON.stringify({ clips: output, source_run_foot_height: sourceFeet, source_run_min: sourceFloor }))
}, 180000)
