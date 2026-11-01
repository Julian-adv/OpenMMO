import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import { RoomEnvironment } from 'three/examples/jsm/environments/RoomEnvironment.js'
import { disposeObjects } from './dispose-objects'
import { getItemDef } from '../lib/data/itemDefs'
import {
  getMonsterDef,
  splitClipNames,
  type MonsterDefinition,
} from '../lib/data/monsterDefs'
import { seatMonsterWeapon } from '../lib/utils/monsterWeapon'
import { loadSharedPackClipsForModel } from '../lib/utils/characterAnimationUtils'

const MOTIONS: [keyof MonsterDefinition, string][] = [
  ['animIdle', '대기'],
  ['animWalk', '걷기'],
  ['animRun', '달리기'],
  ['animAttackIdle', '전투 대기'],
  ['animAttack', '공격'],
  ['animDie', '사망'],
]

/** Any monster row as the game builds it: model, scale, clips and hand weapon. */
export async function showMonster(
  id: string,
  scene: THREE.Scene,
  resources: THREE.Object3D[],
  cleanup: (() => void)[],
  disposed: () => boolean
) {
  const def = getMonsterDef(id)
  if (!def) throw new Error(`몬스터 정의가 없습니다: ${id}`)
  document.title = `${def.name} · 캐릭터 미리보기`
  document.querySelector('h1')!.textContent = `${def.name} 미리보기`
  document.querySelector('header p:last-child')!.textContent =
    '게임과 같은 모델·배율·동작·손 무기로 몬스터를 살펴보세요.'
  document
    .querySelector('nav[aria-label="광휘의 투구 착용 비교"]')
    ?.setAttribute('hidden', '')
  const status = document.getElementById('status')!
  const aside = document.querySelector('aside')!
  aside.replaceChildren(status)
  const motions = MOTIONS.flatMap(([key, label]) =>
    splitClipNames(def[key] as string | undefined).map(
      (clip) => [clip, label] as const
    )
  ).filter(
    ([clip], index, list) => list.findIndex(([c]) => c === clip) === index
  )
  aside.insertAdjacentHTML(
    'beforeend',
    `<fieldset id="monster-controls" disabled>
      <legend>모델과 동작</legend>
      <label for="monster-motion">동작</label>
      <select id="monster-motion">
        <option value="">기준 자세</option>
        ${motions.map(([clip, label]) => `<option value="${clip}">${label} · ${clip}</option>`).join('')}
      </select>
      <div class="buttons">
        <button id="monster-pause" type="button">일시정지</button>
        <button id="monster-replay" type="button">처음부터</button>
      </div>
      <label for="monster-scrub">재생 위치 <output id="monster-time">0.00 / 0.00초</output></label>
      <input id="monster-scrub" type="range" min="0" max="1" step="0.001" value="0" />
      <label for="monster-speed">재생 속도</label>
      <select id="monster-speed">
        <option value="0.25">0.25배</option>
        <option value="0.5">0.5배</option>
        <option value="1" selected>1배</option>
      </select>
      <label for="monster-view">시점</label>
      <select id="monster-view">
        <option value="angle">전체 · 사선</option>
        <option value="front">정면</option>
        <option value="side">측면</option>
        <option value="back">후면</option>
        <option value="weapon">손 무기 확대</option>
      </select>
      <label class="toggle"><input id="monster-weapon" type="checkbox" checked /> 손 무기 장착</label>
      <label class="toggle"><input id="monster-reference" type="checkbox" checked /> 1.9m 플레이어와 크기 비교</label>
      <label class="toggle"><input id="monster-skeleton" type="checkbox" /> 뼈대 보기</label>
      <button id="monster-reset" type="button">시점 초기화</button>
    </fieldset>
    <p id="monster-stats" class="details"></p>
    <p class="details">바닥 격자 한 칸은 1m입니다.</p>`
  )
  const el = <T extends HTMLElement>(name: string) =>
    document.getElementById(`monster-${name}`) as T
  const motion = el<HTMLSelectElement>('motion')
  const view = el<HTMLSelectElement>('view')
  const scrub = el<HTMLInputElement>('scrub')
  const speed = el<HTMLSelectElement>('speed')
  const pause = el<HTMLButtonElement>('pause')
  const replay = el<HTMLButtonElement>('replay')
  const time = el<HTMLOutputElement>('time')
  const weaponToggle = el<HTMLInputElement>('weapon')
  const referenceToggle = el<HTMLInputElement>('reference')
  const params = new URLSearchParams(location.search)
  status.textContent = `${def.name}와 비교용 플레이어를 불러오는 중…`

  const load = async (loader: GLTFLoader, url: string) => {
    const gltf = await loader.loadAsync(url)
    resources.push(gltf.scene)
    if (disposed()) {
      disposeObjects([gltf.scene])
      throw new Error('Preview closed')
    }
    return gltf
  }
  const referenceLoader = new GLTFLoader().register(() => ({
    name: 'reference-material',
    loadMaterial: async () =>
      new THREE.MeshStandardMaterial({ color: '#7f9daa', roughness: 1 }),
  }))
  const weaponModel = def.weapon
    ? (getItemDef(def.weapon)?.worldModel ?? def.weapon)
    : undefined
  const [gltf, player, weaponGltf] = await Promise.all([
    load(
      new GLTFLoader().setMeshoptDecoder(MeshoptDecoder),
      `/models/${def.model}`
    ),
    load(referenceLoader, '/__modular-character/parts/base.glb'),
    weaponModel
      ? load(
          new GLTFLoader().setMeshoptDecoder(MeshoptDecoder),
          `/models/${weaponModel}`
        )
      : undefined,
  ])
  if (disposed()) return
  const scale = def.scale ?? 1
  const model = gltf.scene
  model.scale.setScalar(scale)
  const reference = player.scene
  reference.position.x = -1.75
  reference.visible = params.get('reference') !== '0'
  referenceToggle.checked = reference.visible
  scene.add(model, reference)

  scene.updateMatrixWorld(true)
  const modelBounds = new THREE.Box3().setFromObject(model)
  let triangles = 0
  let joints = 0
  model.traverse((node) => {
    if (node instanceof THREE.Mesh)
      triangles +=
        (node.geometry.index?.count ??
          node.geometry.attributes.position.count) / 3
    if (node instanceof THREE.SkinnedMesh)
      joints = Math.max(joints, node.skeleton.bones.length)
  })
  el('stats').textContent =
    `${def.model} · 배율 ${scale} · 키 ${modelBounds.getSize(new THREE.Vector3()).y.toFixed(2)}m · ${triangles.toLocaleString()}삼각형 · 뼈 ${joints}개 · ${def.sharedAnims ? '공용 동작 팩 리타게팅' : 'GLB 내장 동작'}`

  const hand = def.weaponBone
    ? model.getObjectByName(def.weaponBone)
    : undefined
  const weapon = weaponGltf?.scene
  if (weapon && hand) {
    seatMonsterWeapon(hand, weapon, def).add(weapon)
    weapon.visible = params.get('weapon') !== '0'
  }
  weaponToggle.checked = !!weapon?.visible
  weaponToggle.disabled = !weapon || !hand

  const skeleton = new THREE.SkeletonHelper(model)
  skeleton.visible = params.get('skeleton') === '1'
  skeleton.renderOrder = 10
  for (const material of [skeleton.material].flat()) {
    material.depthTest = false
    material.depthWrite = false
  }
  scene.add(skeleton)
  cleanup.push(() => skeleton.dispose())
  const skeletonToggle = el<HTMLInputElement>('skeleton')
  skeletonToggle.checked = skeleton.visible
  skeletonToggle.onchange = () => {
    skeleton.visible = skeletonToggle.checked
  }
  const grid = new THREE.GridHelper(10, 10, '#57717a', '#30454e')
  grid.position.y = -0.003
  scene.add(grid)
  cleanup.push(() => grid.dispose())

  const host = document.getElementById('viewport')!
  const renderer = new THREE.WebGLRenderer({ antialias: true })
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2))
  renderer.setClearColor('#17252d')
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  host.append(renderer.domElement)
  cleanup.push(() => {
    renderer.setAnimationLoop(null)
    renderer.dispose()
    renderer.domElement.remove()
  })
  const room = new RoomEnvironment()
  const pmrem = new THREE.PMREMGenerator(renderer)
  const environment = pmrem.fromScene(room)
  room.dispose()
  pmrem.dispose()
  scene.environment = environment.texture
  scene.environmentIntensity = 0.75
  cleanup.push(() => environment.dispose())
  scene.add(new THREE.HemisphereLight('#e1ecf2', '#62716b', 2))
  const key = new THREE.DirectionalLight('#fff1e0', 2.5)
  key.position.set(-3, 6, 5)
  const fill = new THREE.DirectionalLight('#b1cbe8', 1.6)
  fill.position.set(3, 3, -3)
  scene.add(key, fill)

  const camera = new THREE.PerspectiveCamera(35, 1, 0.02, 100)
  const controls = new OrbitControls(camera, renderer.domElement)
  controls.enableDamping = true
  controls.minDistance = 0.3
  controls.maxDistance = 15
  cleanup.push(() => controls.dispose())
  const views: Record<string, [number, number, number]> = {
    angle: [3, 1.2, 6],
    front: [0, 0.1, 1],
    side: [1, 0.1, 0],
    back: [0, 0.1, -1],
    weapon: [1, 0.5, 1],
  }
  const requestedView = params.get('camera') ?? ''
  if (Object.hasOwn(views, requestedView)) view.value = requestedView
  const remember = () => {
    const query = new URLSearchParams(location.search)
    query.set('motion', motion.value)
    query.set('camera', view.value)
    query.set('reference', reference.visible ? '1' : '0')
    query.set('weapon', weapon?.visible ? '1' : '0')
    history.replaceState(null, '', `?${query}`)
  }
  const reset = () => {
    model.updateWorldMatrix(true, true)
    const bounds = modelBounds.clone()
    if (reference.visible && view.value !== 'weapon')
      bounds.union(new THREE.Box3().setFromObject(reference))
    const sphere = bounds.getBoundingSphere(new THREE.Sphere())
    if (view.value === 'weapon' && weapon?.visible)
      new THREE.Box3().setFromObject(weapon).getBoundingSphere(sphere)
    const halfFov = Math.atan(
      Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)) *
        Math.min(camera.aspect, 1)
    )
    controls.target.copy(sphere.center)
    camera.position
      .fromArray(views[view.value])
      .normalize()
      .multiplyScalar((sphere.radius * 1.12) / Math.sin(halfFov))
      .add(sphere.center)
    controls.update()
  }
  view.onchange = () => {
    reset()
    remember()
  }
  referenceToggle.onchange = () => {
    reference.visible = referenceToggle.checked
    reset()
    remember()
  }
  weaponToggle.onchange = () => {
    if (weapon) weapon.visible = weaponToggle.checked
    remember()
  }
  el('reset').onclick = reset

  const clipNames = motions.map(([clip]) => clip)
  status.textContent = def.sharedAnims
    ? '공용 동작 팩을 이 골격에 맞추는 중…'
    : '동작을 준비하는 중…'
  const clips = def.sharedAnims
    ? await loadSharedPackClipsForModel(def.model, model, clipNames, {
        restClip: def.animDie,
        restOffset: (def.corpseGroundOffset ?? 0) / scale,
      })
    : gltf.animations
  if (disposed()) return
  const byName = new Map(clips.map((clip) => [clip.name, clip]))
  for (const option of motion.options)
    if (option.value && !byName.has(option.value)) {
      option.disabled = true
      option.textContent += ' (없음)'
    }

  const mixer = new THREE.AnimationMixer(model)
  let action: THREE.AnimationAction | undefined
  let paused = false
  const play = () => {
    mixer.stopAllAction()
    const clip = byName.get(motion.value)
    action = clip ? mixer.clipAction(clip).reset().play() : undefined
    if (action && motion.value === def.animDie) {
      action.setLoop(THREE.LoopOnce, 1)
      action.clampWhenFinished = true
    }
    paused = false
    pause.textContent = '일시정지'
    pause.disabled = replay.disabled = scrub.disabled = !action
    scrub.value = '0'
    mixer.update(0)
    remember()
  }
  motion.onchange = play
  replay.onclick = play
  pause.onclick = () => {
    paused = !paused
    pause.textContent = paused ? '재생' : '일시정지'
  }
  scrub.oninput = () => {
    if (!action) return
    paused = true
    pause.textContent = '재생'
    action.time = Math.min(
      Number(scrub.value) * action.getClip().duration,
      action.getClip().duration - 1e-6
    )
    mixer.update(0)
  }
  const requestedMotion = params.get('motion') ?? def.animIdle ?? ''
  if (byName.has(requestedMotion)) motion.value = requestedMotion

  const resize = () => {
    camera.aspect = host.clientWidth / Math.max(host.clientHeight, 1)
    camera.updateProjectionMatrix()
    renderer.setSize(host.clientWidth, host.clientHeight)
  }
  const observer = new ResizeObserver(resize)
  observer.observe(host)
  cleanup.push(() => observer.disconnect())
  resize()
  play()
  reset()
  let previous = performance.now()
  renderer.setAnimationLoop((now) => {
    const dt = Math.min((now - previous) / 1000, 0.05)
    previous = now
    if (!paused) mixer.update(dt * Number(speed.value))
    if (action) scrub.value = String(action.time / action.getClip().duration)
    time.textContent = `${(action?.time ?? 0).toFixed(2)} / ${(action?.getClip().duration ?? 0).toFixed(2)}초`
    controls.update()
    renderer.render(scene, camera)
  })
  cleanup.push(() => {
    mixer.stopAllAction()
    mixer.uncacheRoot(model)
  })
  el<HTMLFieldSetElement>('controls').disabled = false
  status.textContent = `${def.name}를 불러왔습니다. 동작과 시점을 선택해 보세요.`
  document.body.dataset.ready = 'true'
}
