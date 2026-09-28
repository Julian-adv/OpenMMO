import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import {
  computeSoleGroundOffset,
  createCharacterModelRoot,
  groundRetargetedClips,
  retargetAnimationsForCharacterModel,
} from '../lib/utils/characterAnimationUtils'
import {
  applyModularFingerPose,
  bindModularPart,
  modularRigId,
  modularSwordTracks,
  parseModularHandProfile,
  skinnedParts,
} from '../lib/utils/modularCharacter'
import { poseMainHandProp } from '../lib/utils/handProps'
import './modular-character.css'

const el = <T extends HTMLElement>(id: string): T => {
  const node = document.getElementById(id)
  if (!node) throw new Error(`Missing preview element: ${id}`)
  return node as T
}
const status = el('status')
const fieldset = el<HTMLFieldSetElement>('controls')
const clipSelect = el<HTMLSelectElement>('clip')
const cameraSelect = el<HTMLSelectElement>('camera')
const speed = el<HTMLSelectElement>('speed')
const scrub = el<HTMLInputElement>('scrub')
const correction = el<HTMLInputElement>('correction')
const showWeapon = el<HTMLInputElement>('weapon')
const pause = el<HTMLButtonElement>('pause')
const sequence = el<HTMLButtonElement>('sequence')
const clockLabel = el<HTMLOutputElement>('time')
const host = el('viewport')
const scene = new THREE.Scene()
const resources: THREE.Object3D[] = [scene]
const cleanup: (() => void)[] = []
let disposed = false

function disposeObjects(roots: THREE.Object3D[]) {
  const geometries = new Set<THREE.BufferGeometry>()
  const materials = new Set<THREE.Material>()
  const textures = new Set<THREE.Texture>()
  const skeletons = new Set<THREE.Skeleton>()
  for (const root of roots)
    root.traverse((node) => {
      if (!(node instanceof THREE.Mesh)) return
      geometries.add(node.geometry)
      for (const material of Array.isArray(node.material)
        ? node.material
        : [node.material]) {
        materials.add(material)
        for (const value of Object.values(material)) {
          if (value instanceof THREE.Texture) textures.add(value)
        }
      }
      if (node instanceof THREE.SkinnedMesh) skeletons.add(node.skeleton)
    })
  for (const geometry of geometries) geometry.dispose()
  for (const material of materials) material.dispose()
  for (const texture of textures) {
    if (
      typeof ImageBitmap !== 'undefined' &&
      texture.image instanceof ImageBitmap
    )
      texture.image.close()
    texture.dispose()
  }
  for (const skeleton of skeletons) skeleton.dispose()
}

function dispose() {
  if (disposed) return
  disposed = true
  for (const release of cleanup.reverse()) release()
  disposeObjects(resources)
}
window.addEventListener('pagehide', dispose, { once: true })
import.meta.hot?.dispose(dispose)

export let preview:
  | {
      modelRoot: THREE.Object3D
      mixer: THREE.AnimationMixer
      currentAction: () => THREE.AnimationAction
      weapon: THREE.Object3D
    }
  | undefined

async function main() {
  if (!import.meta.env.DEV)
    throw new Error('개발 서버에서 여는 제작용 미리보기입니다.')
  const loader = new GLTFLoader()
  const load = async (url: string) => {
    const gltf = await loader.loadAsync(url)
    if (disposed) {
      disposeObjects([gltf.scene])
      throw new Error('Preview closed')
    }
    resources.push(gltf.scene)
    return gltf
  }
  const [base, shorts, locomotion, combat, sword, profile] = await Promise.all([
    load('/__modular-character/base.glb'),
    load('/__modular-character/default_shorts.glb'),
    load('/models/animations/locomotion.glb'),
    load('/models/animations/combat_melee.glb'),
    load('/models/weapons/sword.glb'),
    fetch('/__modular-character/hand-grips.json').then(async (response) => {
      if (!response.ok) throw new Error('손 보정 파일을 불러오지 못했습니다.')
      return parseModularHandProfile(await response.json())
    }),
  ])
  const { modelRoot, clonedScene: body } = createCharacterModelRoot(base.scene)
  scene.add(modelRoot)
  const rigId = modularRigId(body)
  let pants = bindModularPart(body, shorts.scene)
  modelRoot.position.y += computeSoleGroundOffset(modelRoot)
  const meshes = skinnedParts(body)
  for (const mesh of meshes) {
    if (!Array.isArray(mesh.material) && mesh.material.name === 'covered_skin')
      mesh.visible = false
    mesh.frustumCulled = false
  }
  status.textContent = '기존 게임 동작과 손 보정을 준비하는 중…'
  const packs = [
    { gltf: locomotion, names: ['idle1', 'walk', 'run', 'jump'] },
    { gltf: combat, names: ['combat_idle', 'slash1', 'dying'] },
  ]
  const raw: THREE.AnimationClip[] = []
  for (const { gltf, names } of packs) {
    const clips = names.map((name) => {
      const clip = gltf.animations.find((clip) => clip.name === name)
      if (!clip) throw new Error(`동작이 없습니다: ${name}`)
      return clip
    })
    raw.push(
      ...(await retargetAnimationsForCharacterModel(
        base.scene,
        gltf.scene,
        clips
      ))
    )
  }
  const original = await groundRetargetedClips(base.scene, raw, {
    restClip: 'dying',
    plantedClips: ['walk'],
    baselineClips: ['jump'],
  })
  const corrected = await groundRetargetedClips(
    base.scene,
    raw.map((clip) => applyModularFingerPose(clip, profile, rigId)),
    { restClip: 'dying', plantedClips: ['walk'], baselineClips: ['jump'] }
  )
  if (disposed) return
  const hand = body.getObjectByName('RightHand')
  const head = body.getObjectByName('Head')
  if (!hand || !head) throw new Error('손 또는 머리 본을 찾지 못했습니다.')
  const weapon = new THREE.Group()
  weapon.name = 'previewSwordAttachment'
  weapon.add(sword.scene.clone(true))
  hand.add(weapon)
  const mixer = new THREE.AnimationMixer(modelRoot)
  cleanup.push(() => {
    mixer.stopAllAction()
    mixer.uncacheRoot(modelRoot)
  })
  let active: THREE.AnimationAction
  let playing = true
  let sequencing = false
  let sequenceTime = 0
  let transitionLeft = 0
  const route = [
    'idle1',
    'walk',
    'run',
    'jump',
    'combat_idle',
    'slash1',
    'dying',
    'idle1',
  ]
  let routeIndex = 0
  const actions = [original, corrected].map(
    (clips, variant) =>
      new Map(
        clips.map((clip) => {
          const posed = clip.clone()
          if (variant === 1) {
            posed.tracks.push(
              ...modularSwordTracks(profile, rigId, clip, weapon.name)
            )
          } else {
            const grip = new THREE.Object3D()
            poseMainHandProp(grip, 'iron_sword')
            posed.tracks.push(
              new THREE.VectorKeyframeTrack(
                `${weapon.name}.position`,
                [0],
                grip.position.toArray()
              ),
              new THREE.QuaternionKeyframeTrack(
                `${weapon.name}.quaternion`,
                [0],
                grip.quaternion.toArray()
              )
            )
          }
          const action = mixer.clipAction(posed)
          action.setLoop(
            ['jump', 'slash1', 'dying'].includes(clip.name)
              ? THREE.LoopOnce
              : THREE.LoopRepeat,
            Infinity
          )
          action.clampWhenFinished = true
          return [clip.name, action]
        })
      )
  )
  const setPlaying = (value: boolean) => {
    playing = value
    pause.textContent = playing ? '일시정지' : '계속 재생'
  }
  const stopSequence = () => {
    sequencing = false
    sequence.textContent = '동작 전환 연속 확인'
  }
  const play = (name: string, fade = true, normalizedTime = 0) => {
    const next = actions[correction.checked ? 1 : 0].get(name)
    if (!next) throw new Error(`재생할 동작이 없습니다: ${name}`)
    for (const action of mixerActions()) {
      action.stopFading()
      if (action !== active) action.stop()
    }
    const previous = active
    next.reset().setEffectiveTimeScale(1).setEffectiveWeight(1).play()
    next.time = normalizedTime * next.getClip().duration
    if (previous && previous !== next) {
      if (fade && playing) {
        previous.crossFadeTo(next, 0.25, false)
        transitionLeft = 0.25
      } else previous.stop()
    }
    active = next
    clipSelect.value = name
    mixer.update(0)
    status.textContent = `${clipSelect.selectedOptions[0].textContent} · ${correction.checked ? '손 보정 적용' : '기본 손 자세·그립'}`
  }
  const mixerActions = () =>
    new Set(actions.flatMap((map) => [...map.values()]))
  play('combat_idle', false)
  preview = { modelRoot, mixer, currentAction: () => active, weapon }
  clipSelect.onchange = () => {
    stopSequence()
    play(clipSelect.value)
  }
  pause.onclick = () => setPlaying(!playing)
  el('replay').onclick = () => {
    stopSequence()
    play(clipSelect.value, false)
    setPlaying(true)
  }
  scrub.oninput = () => {
    stopSequence()
    setPlaying(false)
    for (const action of mixerActions()) {
      action.stopFading()
      if (action !== active) action.stop()
    }
    active.enabled = true
    active.paused = false
    active.setEffectiveWeight(1)
    active.time = Math.min(
      Number(scrub.value) * active.getClip().duration,
      active.getClip().duration - 1e-5
    )
    mixer.update(0)
  }
  correction.onchange = () => {
    const fraction = active.time / active.getClip().duration
    play(clipSelect.value, false, fraction)
  }
  showWeapon.onchange = () => {
    weapon.visible = showWeapon.checked
  }
  sequence.onclick = () => {
    if (sequencing) return stopSequence()
    sequencing = true
    sequence.textContent = '연속 확인 중지'
    routeIndex = 0
    sequenceTime = 0
    setPlaying(true)
    play(route[0])
  }
  el('reattach').onclick = () => {
    const replacement = bindModularPart(body, shorts.scene)
    for (const mesh of pants) mesh.removeFromParent()
    pants = replacement
    for (const mesh of pants) mesh.frustumCulled = false
    status.textContent = '하의를 다시 장착했습니다. 동작은 이어서 재생합니다.'
  }
  const renderer = new THREE.WebGLRenderer({ antialias: true })
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2))
  renderer.setClearColor('#17252d')
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  renderer.shadowMap.enabled = true
  renderer.shadowMap.type = THREE.PCFSoftShadowMap
  host.append(renderer.domElement)
  cleanup.push(() => {
    renderer.dispose()
    renderer.domElement.remove()
  })
  scene.add(new THREE.HemisphereLight('#e1ecf2', '#62716b', 2))
  const key = new THREE.DirectionalLight('#fff1e0', 2.5)
  key.position.set(-3, 6, 5)
  key.castShadow = true
  key.shadow.mapSize.set(2048, 2048)
  key.shadow.camera.left = key.shadow.camera.bottom = -3
  key.shadow.camera.right = key.shadow.camera.top = 3
  key.shadow.bias = -0.0003
  key.shadow.normalBias = 0.02
  scene.add(key)
  cleanup.push(() => key.shadow.dispose())
  const fill = new THREE.DirectionalLight('#b1cbe8', 1.6)
  fill.position.set(3, 3, -3)
  scene.add(fill)
  const floor = new THREE.Mesh(
    new THREE.CircleGeometry(4, 80),
    new THREE.MeshStandardMaterial({ color: '#2e424b', roughness: 1 })
  )
  floor.rotation.x = -Math.PI / 2
  floor.position.y = 0
  floor.receiveShadow = true
  scene.add(floor)
  const camera = new THREE.PerspectiveCamera(36, 1, 0.01, 100)
  const controls = new OrbitControls(camera, renderer.domElement)
  controls.minDistance = 0.18
  controls.maxDistance = 12
  controls.enableDamping = true
  cleanup.push(() => controls.dispose())
  const tracked = new THREE.Vector3()
  const previousTarget = new THREE.Vector3()
  const setCamera = () => {
    modelRoot.updateMatrixWorld(true)
    if (cameraSelect.value === 'hand') {
      hand.getWorldPosition(tracked)
      tracked.y -= 0.08
      camera.position.copy(tracked).add(new THREE.Vector3(-0.44, 0.18, 0.55))
    } else if (cameraSelect.value === 'face') {
      head.getWorldPosition(tracked)
      tracked.y += 0.1
      camera.position.copy(tracked).add(new THREE.Vector3(0.18, 0.04, 0.72))
    } else {
      tracked.set(0, 0.95, 0)
      camera.position.set(2.6, 2, 4.3)
    }
    controls.target.copy(tracked)
    previousTarget.copy(tracked)
    controls.update()
  }
  cameraSelect.onchange = setCamera
  setCamera()
  const resize = () => {
    camera.aspect = host.clientWidth / host.clientHeight
    camera.updateProjectionMatrix()
    renderer.setSize(host.clientWidth, host.clientHeight)
  }
  const observer = new ResizeObserver(resize)
  observer.observe(host)
  cleanup.push(() => observer.disconnect())
  resize()
  const triangles = (root: THREE.Object3D, visible: boolean) => {
    let count = 0
    root.traverse((node) => {
      if (!(node instanceof THREE.Mesh) || (visible && !node.visible)) return
      count +=
        (node.geometry.index?.count ??
          node.geometry.getAttribute('position').count) / 3
    })
    return count
  }
  el('stats').textContent =
    `몸체·하의 ${triangles(body, false) - triangles(weapon, false)}삼각형 · 표시 ${triangles(body, true) - triangles(weapon, true)} · 얼굴 1,505 · 검 302`
  let last = performance.now()
  const render = (now: number) => {
    const dt = Math.min((now - last) / 1000, 0.05) * Number(speed.value)
    last = now
    if (playing) {
      mixer.update(dt)
      transitionLeft -= dt
      if (transitionLeft <= 0) {
        for (const action of mixerActions())
          if (action !== active) action.stop()
      }
      if (sequencing) {
        sequenceTime += dt
        const hold = Math.max(active.getClip().duration, 1.4) + 0.35
        if (sequenceTime >= hold) {
          sequenceTime = 0
          if (++routeIndex >= route.length) stopSequence()
          else play(route[routeIndex])
        }
      }
    }
    modelRoot.updateMatrixWorld(true)
    if (cameraSelect.value !== 'full') {
      const target = cameraSelect.value === 'hand' ? hand : head
      target.getWorldPosition(tracked)
      tracked.y += cameraSelect.value === 'hand' ? -0.08 : 0.1
      const delta = tracked.clone().sub(previousTarget)
      camera.position.add(delta)
      controls.target.add(delta)
      previousTarget.copy(tracked)
    }
    controls.update()
    const duration = active.getClip().duration
    scrub.value = String(active.time / duration)
    clockLabel.textContent = `${active.time.toFixed(2)} / ${duration.toFixed(2)}초`
    renderer.render(scene, camera)
  }
  renderer.setAnimationLoop(render)
  cleanup.push(() => renderer.setAnimationLoop(null))
  fieldset.disabled = false
  document.body.dataset.ready = 'true'
}

main().catch((error: unknown) => {
  if (disposed) return
  status.dataset.error = 'true'
  status.textContent = `${error instanceof Error ? error.message : String(error)} 로컬 제작 에셋과 개발 서버를 확인하세요.`
  console.error(error)
  dispose()
})
