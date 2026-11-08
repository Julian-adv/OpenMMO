import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import { RoomEnvironment } from 'three/examples/jsm/environments/RoomEnvironment.js'
import { bindModularPart, skinnedParts } from '../lib/utils/modularCharacter'
import {
  bindFemaleFacePart,
  FEMALE_FACE_PART_PATH,
  markFemaleBase,
} from '../lib/utils/femaleCharacter'
import { MODULAR_FEMALE_MODEL_PATH } from '../lib/utils/modelPaths'
import { disposeObjects } from './dispose-objects'
import { faceHairFitPanel, faceHairFitEditor } from './female-face-hair-fit'
import type { HairFitFace, HairFitStyle } from '../lib/utils/femaleHairFit'

export async function showFemaleFace(
  scene: THREE.Scene,
  resources: THREE.Object3D[],
  cleanup: (() => void)[],
  disposed: () => boolean
) {
  document.title = '여성 새 얼굴 · 미리보기'
  document.querySelector('h1')!.textContent = '여성 얼굴 미리보기'
  document.querySelector('header p:last-child')!.textContent =
    '얼굴별로 헤어를 맞추고 저장한 뒤 동작을 확인하세요.'
  const status = document.getElementById('status')!
  const aside = document.querySelector('aside')!
  aside.replaceChildren(status)
  aside.insertAdjacentHTML(
    'beforeend',
    `<fieldset id="face-controls" disabled>
      <legend>얼굴 살펴보기</legend>
      <label for="female-face">얼굴</label>
      <select id="female-face"><option value="custom">새 얼굴</option><option value="default">기존 얼굴</option></select>
      <label for="face-hair">헤어</label>
      <select id="face-hair"><option value="bob">단발</option><option value="long">긴 머리</option><option value="ponytail">묶은 머리</option><option value="none">없음</option></select>
      <label for="face-view">시점</label>
      <select id="face-view"><option value="face">얼굴 확대</option><option value="angle">얼굴 사선</option><option value="side">얼굴 측면</option><option value="back">뒤통수</option><option value="full">전신</option></select>
      <label for="face-motion">동작</label>
      <select id="face-motion"><option value="rest">기본 자세</option></select>
      <button id="face-pause" type="button">일시정지</button>
      <label class="toggle"><input id="face-wireframe" type="checkbox" /> 와이어프레임</label>
      <button id="face-reset" type="button">시점 초기화</button>
    </fieldset>
    ${faceHairFitPanel}
    <p id="face-stats" class="details"></p>
    <p class="details">드래그로 회전하고 마우스 휠로 확대할 수 있습니다.</p>`
  )
  const select = (id: string) =>
    document.getElementById(id) as HTMLSelectElement
  const face = select('female-face')
  const hair = select('face-hair')
  const view = select('face-view')
  const motion = select('face-motion')
  const params = new URLSearchParams(location.search)
  face.value = params.get('face') === 'default' ? 'default' : 'custom'
  const requestedHair =
    params.get('hair') === 'warrior' ? 'long' : params.get('hair')
  hair.value =
    requestedHair && ['bob', 'long', 'ponytail', 'none'].includes(requestedHair)
      ? requestedHair
      : 'bob'
  const reload = () => {
    const query = new URLSearchParams(location.search)
    query.set('face', face.value)
    query.set('hair', hair.value)
    query.set('camera', view.value)
    location.search = query.toString()
  }
  face.onchange = hair.onchange = reload
  status.textContent = '얼굴과 헤어를 불러오는 중…'
  const loader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder)
  const load = async (url: string) => {
    const asset = await loader.loadAsync(url)
    if (disposed()) {
      disposeObjects([asset.scene])
      throw new Error('미리보기가 닫혔습니다.')
    }
    resources.push(asset.scene)
    return asset
  }
  // ?source=preview shows the fitting source instead of the game's base.glb and face part.
  const fitting = params.get('source') === 'preview'
  const [base, facePart, hairstyle] = await Promise.all([
    load(
      fitting
        ? `/__female-preview/${face.value === 'custom' ? 'face-custom' : 'base'}.glb`
        : MODULAR_FEMALE_MODEL_PATH
    ),
    face.value === 'custom' && !fitting ? load(FEMALE_FACE_PART_PATH) : null,
    hair.value === 'none'
      ? null
      : load(
          `/models/characters/modular_female/${({ bob: 'hair', long: 'valkyrie_hair', ponytail: 'ranger_hair' } as Record<string, string>)[hair.value]}.glb`
        ),
  ])
  if (disposed()) return
  const model = base.scene
  if (!fitting) markFemaleBase(base, model, !!facePart)
  if (facePart) {
    bindFemaleFacePart(model, facePart)
    const body = skinnedParts(model).find((mesh) => mesh.userData.female_body)!
    const hidden = new Set<number>(
      model.userData.female_face_part.hide_triangle_indices
    )
    body.geometry.setIndex(
      Array.from(body.geometry.index!.array).filter(
        (_, i) => !hidden.has(Math.floor(i / 3))
      )
    )
  }
  const hairMeshes = hairstyle ? bindModularPart(model, hairstyle.scene) : []
  await faceHairFitEditor(
    hairMeshes,
    face.value as HairFitFace,
    hair.value as HairFitStyle | 'none',
    disposed
  )
  if (disposed()) return
  scene.add(model)
  let triangles = 0
  const materials = new Set<THREE.Material>()
  model.traverse((object) => {
    if (!(object instanceof THREE.Mesh) || !object.visible) return
    triangles +=
      (object.geometry.index?.count ??
        object.geometry.attributes.position.count) / 3
    for (const material of Array.isArray(object.material)
      ? object.material
      : [object.material])
      materials.add(material)
  })
  document.getElementById('face-stats')!.textContent =
    `${face.value === 'custom' ? '새 얼굴' : '기존 얼굴'} · ${triangles.toLocaleString()}삼각형`
  const host = document.getElementById('viewport')!
  const renderer = new THREE.WebGLRenderer({ antialias: true })
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2))
  renderer.setClearColor('#252525')
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
  scene.environmentIntensity = 0.45
  cleanup.push(() => environment.dispose())
  scene.add(new THREE.HemisphereLight('#e1ecf2', '#62716b', 1.4))
  const key = new THREE.DirectionalLight('#fff1e0', 1.7)
  key.position.set(-3, 6, 5)
  const fill = new THREE.DirectionalLight('#b1cbe8', 0.65)
  fill.position.set(3, 3, -3)
  scene.add(key, fill)
  const camera = new THREE.PerspectiveCamera(35, 1, 0.01, 30)
  const controls = new OrbitControls(camera, renderer.domElement)
  controls.enableDamping = true
  controls.minDistance = 0.2
  controls.maxDistance = 6
  cleanup.push(() => controls.dispose())
  const views: Record<string, [number, number, number]> = {
    face: [0.001, 1.6821, 0.813],
    angle: [0.5, 1.66, 0.7],
    side: [0.8, 1.66, 0],
    back: [0, 1.66, -0.8],
    full: [0, 1, 3.6],
  }
  const requestedView = params.get('camera')
  if (requestedView && Object.hasOwn(views, requestedView))
    view.value = requestedView
  const reset = () => {
    camera.fov = view.value === 'face' ? 20 : 35
    camera.updateProjectionMatrix()
    controls.target.set(
      view.value === 'face' ? 0.001 : 0,
      view.value === 'full' ? 0.9 : view.value === 'face' ? 1.6821 : 1.64,
      0
    )
    camera.position.fromArray(views[view.value])
    controls.update()
  }
  view.onchange = reset
  document.getElementById('face-reset')!.onclick = reset
  const wireframe = document.getElementById(
    'face-wireframe'
  ) as HTMLInputElement
  wireframe.onchange = () => {
    for (const material of materials)
      if (material instanceof THREE.MeshStandardMaterial)
        material.wireframe = wireframe.checked
  }
  const rest: {
    bone: THREE.Bone
    position: THREE.Vector3
    quaternion: THREE.Quaternion
    scale: THREE.Vector3
  }[] = []
  model.traverse((object) => {
    if (object instanceof THREE.Bone)
      rest.push({
        bone: object,
        position: object.position.clone(),
        quaternion: object.quaternion.clone(),
        scale: object.scale.clone(),
      })
  })
  const mixer = new THREE.AnimationMixer(model)
  cleanup.push(() => {
    mixer.stopAllAction()
    mixer.uncacheRoot(model)
  })
  const names: Record<string, string> = {
    idle1: '대기',
    idle_armed: '무장 대기',
    walk: '걷기',
    run: '달리기',
    jump: '점프',
    combat_idle: '전투 대기',
    slash1: '공격',
    dying: '사망',
    sit_idle: '앉기',
  }
  for (const clip of base.animations.filter((c) => !c.name.includes('|')))
    motion.add(new Option(names[clip.name] ?? clip.name, clip.name))
  motion.onchange = () => {
    mixer.stopAllAction()
    for (const { bone, position, quaternion, scale } of rest) {
      bone.position.copy(position)
      bone.quaternion.copy(quaternion)
      bone.scale.copy(scale)
    }
    const clip = base.animations.find((clip) => clip.name === motion.value)
    if (clip) mixer.clipAction(clip).reset().play()
    if (clip && view.value !== 'full') {
      view.value = 'full'
      reset()
    }
  }
  let paused = false
  const pause = document.getElementById('face-pause') as HTMLButtonElement
  pause.onclick = () => {
    paused = !paused
    pause.textContent = paused ? '재생' : '일시정지'
  }
  const resize = () => {
    camera.aspect = host.clientWidth / Math.max(host.clientHeight, 1)
    camera.updateProjectionMatrix()
    renderer.setSize(host.clientWidth, host.clientHeight)
  }
  const observer = new ResizeObserver(resize)
  observer.observe(host)
  cleanup.push(() => observer.disconnect())
  resize()
  reset()
  let previous = performance.now()
  renderer.setAnimationLoop((now) => {
    const dt = Math.min((now - previous) / 1000, 0.05)
    previous = now
    if (!paused) mixer.update(dt)
    controls.update()
    renderer.render(scene, camera)
  })
  status.textContent = '얼굴과 헤어를 불러왔습니다.'
  ;(document.getElementById('face-controls') as HTMLFieldSetElement).disabled =
    false
  document.body.dataset.ready = 'true'
}
