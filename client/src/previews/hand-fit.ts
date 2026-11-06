import * as THREE from 'three'
import { WebGPURenderer } from 'three/webgpu'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import items from '../../../data/items.json'
import { getItemDef, isRangedWeapon } from '../lib/data/itemDefs'
import monsters from '../../../data/monsters.json'
import initialFits from '../lib/data/handFits.json'
import { getWeaponAnimation } from '../lib/data/weaponAnimationDefs'
import {
  createCharacterModelRoot,
  findBoneByName,
} from '../lib/utils/characterAnimationUtils'
import {
  loadCharacterModel,
  modularSwordAttachment,
} from '../lib/utils/characterModel'
import { configureGLTFRenderer, loadGLB } from '../lib/utils/gltfCache'
import {
  forearmLength,
  HAND_FIT_RIGS,
  handFitKeys,
  mainHandBoneFor,
  poseMainHandProp,
  poseOffHandProp,
  type HandFits,
} from '../lib/utils/handProps'
import { getWeaponModelPath } from '../lib/utils/modelPaths'
import { loadWeaponAnimations } from '../lib/utils/weaponAnimations'
import { getWeaponEffectAxis } from '../lib/utils/weaponEffectAxis'
import { disposeObjects } from './dispose-objects'
import sliders from './hand-fit-sliders.json'
import { sliderEditor, sliderPanel } from './slider-editor'

const RIG_LABELS: Record<string, string> = {
  modular_male: '남성',
  modular_female: '여성',
}
const PACKS = ['animations/locomotion.glb', 'animations/combat_melee.glb']
const degrees = Math.PI / 180

// Bows keep their forearm-scaled seat in code.
const held = Object.keys(items)
  .map((id) => getItemDef(id)!)
  .filter(
    (def) =>
      def.worldModel &&
      (def.equipSlot === 'main_hand' || def.equipSlot === 'off_hand') &&
      !isRangedWeapon(def.id)
  )

/** Fit where any player rig holds any hand prop, saved into handFits.json. */
export async function showHandFit(
  rig: string,
  scene: THREE.Scene,
  resources: THREE.Object3D[],
  cleanup: (() => void)[],
  disposed: () => boolean
) {
  if (!HAND_FIT_RIGS[rig]) rig = 'modular_male'
  const modelPath = HAND_FIT_RIGS[rig]
  const rigLabel = RIG_LABELS[rig] ?? rig
  document.title = '손 소지품 위치 · 캐릭터 미리보기'
  document.querySelector('h1')!.textContent = '손 소지품 위치'
  document.querySelector('header p:last-child')!.textContent =
    '체형마다 손에 드는 무기·악기·도구의 위치를 맞추고 저장합니다.'
  const status = document.getElementById('status')!
  const aside = document.querySelector('aside')!
  aside.replaceChildren(status)
  const armed = Object.entries(monsters as Record<string, { weapon?: string }>)
    .filter(([, def]) => def.weapon)
    .map(([id]) => id)
  aside.insertAdjacentHTML(
    'beforeend',
    `<fieldset id="hand-fit-controls" disabled>
      <legend>체형과 물건</legend>
      <label for="hand-fit-rig">체형</label>
      <select id="hand-fit-rig">
        ${Object.keys(HAND_FIT_RIGS)
          .map(
            (key) => `<option value="${key}">${RIG_LABELS[key] ?? key}</option>`
          )
          .join('')}
        ${armed.map((id) => `<option value="monster:${id}">몬스터 · ${id}</option>`).join('')}
      </select>
      <label for="hand-fit-item">물건</label>
      <select id="hand-fit-item">
        ${held.map((def) => `<option value="${def.id}">${def.name} · ${def.id}</option>`).join('')}
      </select>
      <label for="hand-fit-motion">동작</label>
      <select id="hand-fit-motion"></select>
      <label for="hand-fit-speed">재생 속도</label>
      <select id="hand-fit-speed">
        <option value="0.25">0.25배</option>
        <option value="0.5">0.5배</option>
        <option value="1" selected>1배</option>
      </select>
      <button id="hand-fit-pause" type="button">일시정지</button>
      <label for="hand-fit-scope">적용 범위</label>
      <select id="hand-fit-scope">
        <option value="${rig}">이 체형만 (${rigLabel})</option>
        <option value="all">모든 체형</option>
      </select>
      <label for="hand-fit-key">적용 대상</label>
      <select id="hand-fit-key"></select>
      <p class="details">이 체형 전용 값이 있으면 모든 체형 값보다 우선합니다.</p>
    </fieldset>
    ${sliderPanel('hand-fit', sliders, {
      legend: '위치와 각도',
      note: '저장하면 handFits.json에 기록되고 게임은 새로고침하면 반영됩니다.',
      saveLabel: '손 위치 저장',
    })}`
  )
  const el = <T extends HTMLElement>(name: string) =>
    document.getElementById(`hand-fit-${name}`) as T
  const rigSelect = el<HTMLSelectElement>('rig')
  const itemSelect = el<HTMLSelectElement>('item')
  const motion = el<HTMLSelectElement>('motion')
  const speed = el<HTMLSelectElement>('speed')
  const pause = el<HTMLButtonElement>('pause')
  let paused = false
  pause.onclick = () => {
    paused = !paused
    pause.textContent = paused ? '재생' : '일시정지'
  }
  const scope = el<HTMLSelectElement>('scope')
  const keySelect = el<HTMLSelectElement>('key')
  const params = new URLSearchParams(location.search)
  rigSelect.value = rig
  rigSelect.onchange = () => {
    const [kind, id] = rigSelect.value.split(':')
    location.search =
      kind === 'monster' ? `?monster=${id}` : `?handfit=${rigSelect.value}`
  }
  const requestedItem = params.get('item') ?? 'miner_pickaxe'
  if (held.some((def) => def.id === requestedItem))
    itemSelect.value = requestedItem
  status.textContent = `${rigLabel} 체형을 불러오는 중…`

  const renderer = new WebGPURenderer({ antialias: true })
  configureGLTFRenderer(renderer)
  const gltf = await loadCharacterModel(modelPath)
  if (disposed()) return
  const { modelRoot, clonedScene } = createCharacterModelRoot(gltf.scene)
  resources.push(modelRoot)
  scene.add(modelRoot)
  const clips = new Map<string, THREE.AnimationClip>()
  const addPack = async (pack: string) => {
    for (const [name, clip] of await loadWeaponAnimations(
      modelPath,
      modelRoot,
      { pack }
    ))
      clips.set(name, clip)
  }
  await Promise.all(PACKS.map(addPack))
  if (disposed()) return

  const forearm = forearmLength(
    clonedScene,
    'LeftHand',
    `${modelPath}:LeftHand`
  )
  const fits = structuredClone(initialFits) as HandFits
  let prop: THREE.Object3D | undefined
  // A model the rig carries on a baked socket ignores handFits in the game.
  let socketed = false
  const pose = (target: THREE.Object3D | undefined, using: HandFits) => {
    if (!target || socketed) return
    const def = getItemDef(itemSelect.value)!
    if (def.equipSlot === 'off_hand')
      poseOffHandProp(target, def.worldModel, modelPath, using)
    else poseMainHandProp(target, def.id, forearm, modelPath, using)
  }
  type Values = {
    px: number
    py: number
    pz: number
    tilt: number
    lean: number
    spin: number
  }
  // The prop as a stick in its own frame: its long axis, the edge (width)
  // direction, and the flat-side normal. Tilts and spin turn about these.
  const stick = {
    along: new THREE.Vector3(1, 0, 0),
    edge: new THREE.Vector3(0, 1, 0),
    flat: new THREE.Vector3(0, 0, 1),
  }
  // The seat the sliders start from; tilts and spin are added to it.
  let base = new THREE.Quaternion()
  const fitOf = (v: Values) => {
    const about = (axis: THREE.Vector3, angle: number) =>
      new THREE.Quaternion().setFromAxisAngle(axis, angle * degrees)
    const turned = base
      .clone()
      .multiply(about(stick.flat, v.tilt))
      .multiply(about(stick.edge, v.lean))
      .multiply(about(stick.along, v.spin))
    const e = new THREE.Euler().setFromQuaternion(turned)
    return { position: [v.px, v.py, v.pz], rotation: [e.x, e.y, e.z] }
  }
  const zero: Values = { px: 0, py: 0, pz: 0, tilt: 0, lean: 0, spin: 0 }
  const editor: {
    panel: HTMLElement
    values: Values
    load: (v: Values) => void
  } = sliderEditor('hand-fit', sliders, zero, {
    url: '/__hand-fit',
    extra: () => ({
      rig: scope.value,
      key: keySelect.value,
      rotation: fitOf(editor.values).rotation,
    }),
    apply: (v) => {
      if (!prop) return
      pose(prop, {
        ...fits,
        [scope.value]: { ...fits[scope.value], [keySelect.value]: fitOf(v) },
      })
    },
    onSaved: (v) => {
      fits[scope.value] = { ...fits[scope.value], [keySelect.value]: fitOf(v) }
      reload()
    },
  })
  // Start from where the prop sits now, so saving unchanged keeps the look.
  const reload = () => {
    const probe = new THREE.Object3D()
    pose(probe, fits)
    base = probe.quaternion.clone()
    const [px, py, pz] = probe.position.toArray()
    editor.load({ ...zero, px, py, pz })
  }
  editor.panel.hidden = false

  const keyLabels: Record<string, string> = {
    item: '이 아이템',
    model: '같은 모델',
    type: '같은 무기 종류',
    category: '같은 분류',
  }
  const refreshKeys = () => {
    const id = itemSelect.value
    const def = getItemDef(id)!
    const keys =
      def.equipSlot === 'off_hand'
        ? handFitKeys(undefined, def.worldModel)
        : handFitKeys(id)
    keySelect.innerHTML = keys
      .map(
        (key) =>
          `<option value="${key}">${keyLabels[key.split(':')[0]]} · ${key.slice(key.indexOf(':') + 1)}</option>`
      )
      .join('')
    const existing = keys.find((key) => fits[scope.value]?.[key])
    keySelect.value = existing ?? keys[Math.min(1, keys.length - 1)]
  }
  let loading = 0
  const showItem = async () => {
    const ticket = ++loading
    const id = itemSelect.value
    const def = getItemDef(id)!
    const model = def.worldModel!
    const loaded = (await loadGLB(getWeaponModelPath(model))).scene.clone(true)
    if (disposed() || ticket !== loading) return
    if (prop) {
      prop.removeFromParent()
      disposeObjects([prop])
    }
    prop = loaded
    const socket = modularSwordAttachment(modelRoot, model)
    socketed = !!socket
    const hand = def.equipSlot === 'off_hand' ? 'LeftHand' : mainHandBoneFor(id)
    ;(socket ?? findBoneByName(modelRoot, hand))?.add(prop)
    const axis = getWeaponEffectAxis(prop)
    stick.along.copy(axis.direction)
    stick.edge.copy(axis.widthDirection)
    stick.flat.crossVectors(stick.along, stick.edge).normalize()
    editor.panel.toggleAttribute('disabled', socketed)
    status.textContent = socketed
      ? `${model}은 이 체형에서 모델에 박힌 소켓에 붙어 여기서 맞추지 않습니다.`
      : ''
    const pack = getWeaponAnimation(id)?.pack
    if (pack) await addPack(pack)
    if (disposed() || ticket !== loading) return
    const previous = motion.value
    motion.innerHTML = [...clips.keys()]
      .map((name) => `<option value="${name}">${name}</option>`)
      .join('')
    motion.value = clips.has(previous) ? previous : 'idle1'
    play()
    refreshKeys()
    reload()
    const query = new URLSearchParams(location.search)
    query.set('item', id)
    history.replaceState(null, '', `?${query}`)
  }
  itemSelect.onchange = () => void showItem()
  scope.onchange = () => {
    refreshKeys()
    reload()
  }
  keySelect.onchange = reload

  const mixer = new THREE.AnimationMixer(modelRoot)
  const play = () => {
    mixer.stopAllAction()
    const clip = clips.get(motion.value)
    if (clip) mixer.clipAction(clip).play()
  }
  motion.onchange = play

  const host = document.getElementById('viewport')!
  await renderer.init()
  if (disposed()) return
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  host.append(renderer.domElement)
  cleanup.push(() => {
    renderer.setAnimationLoop(null)
    renderer.dispose()
    renderer.domElement.remove()
    mixer.stopAllAction()
  })
  scene.background = new THREE.Color('#17252d')
  scene.add(new THREE.HemisphereLight('#e1ecf2', '#62716b', 3))
  const key = new THREE.DirectionalLight('#fff1e0', 2.5)
  key.position.set(-3, 6, 5)
  scene.add(key)

  const camera = new THREE.PerspectiveCamera(35, 1, 0.02, 100)
  const controls = new OrbitControls(camera, renderer.domElement)
  controls.enableDamping = true
  cleanup.push(() => controls.dispose())
  camera.position.set(1.1, 1.6, 1.4)
  controls.target.set(0.2, 1.1, 0.2)
  const resize = () => {
    camera.aspect = host.clientWidth / Math.max(host.clientHeight, 1)
    camera.updateProjectionMatrix()
    renderer.setSize(host.clientWidth, host.clientHeight)
  }
  const observer = new ResizeObserver(resize)
  observer.observe(host)
  cleanup.push(() => observer.disconnect())
  resize()

  await showItem()
  let previous = performance.now()
  renderer.setAnimationLoop((now) => {
    if (!paused)
      mixer.update(
        Math.min((now - previous) / 1000, 0.05) * Number(speed.value)
      )
    previous = now
    controls.update()
    renderer.render(scene, camera)
  })
  el<HTMLFieldSetElement>('controls').disabled = false
  if (!socketed)
    status.textContent = `${rigLabel} 체형을 불러왔습니다. 물건과 적용 대상을 고르고 맞춰 보세요.`
  document.body.dataset.ready = 'true'
}
