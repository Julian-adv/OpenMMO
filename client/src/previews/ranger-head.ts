import * as THREE from 'three'
import { disposeObjects } from './dispose-objects'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import { RoomEnvironment } from 'three/examples/jsm/environments/RoomEnvironment.js'

const HEIGHT = 1.9
const HEAD_Y = 0.905 * HEIGHT

export async function showRangerHead(
  scene: THREE.Scene,
  resources: THREE.Object3D[],
  cleanup: (() => void)[],
  disposed: () => boolean
) {
  document.title = '순찰자 머리 접합 실험 · 미리보기'
  document.querySelector('h1')!.textContent = '순찰자 머리 접합 실험'
  document.querySelector('header p:last-child')!.textContent =
    '전신 Tripo 모델의 얼굴과, 얼굴만 따로 생성해 붙인 머리를 비교합니다.'
  document
    .querySelector('nav[aria-label="광휘의 투구 착용 비교"]')
    ?.setAttribute('hidden', '')
  const status = document.getElementById('status')!
  const aside = document.querySelector('aside')!
  aside.replaceChildren(status)
  aside.insertAdjacentHTML(
    'beforeend',
    `<fieldset id="ranger-head-controls" disabled>
      <legend>비교</legend>
      <label for="ranger-head-mode">모델</label>
      <select id="ranger-head-mode">
        <option value="both">나란히 (왼쪽 기존 · 오른쪽 새 머리)</option>
        <option value="new">새 머리 접합</option>
        <option value="old">기존 전신 모델</option>
      </select>
      <label for="ranger-head-view">시점</label>
      <select id="ranger-head-view">
        <option value="front">얼굴 정면</option>
        <option value="angle">얼굴 3/4</option>
        <option value="side">얼굴 측면</option>
        <option value="body">전신</option>
      </select>
      <label class="toggle"><input id="ranger-head-rotate" type="checkbox" /> 자동 회전</label>
      <label class="toggle"><input id="ranger-head-wireframe" type="checkbox" /> 와이어프레임</label>
      <button id="ranger-head-reset" type="button">시점 초기화</button>
    </fieldset>
    <p id="ranger-head-stats" class="details"></p>
    <p class="details">리깅 전 실험용 모델입니다. 새 머리는 회전·균일 스케일·이동만 적용했고 목은 한 평면에서 잘라 맞댔습니다.</p>`
  )
  status.textContent = '모델을 불러오는 중…'
  const loader = new GLTFLoader()
  const loaded = await Promise.all(
    ['old', 'new'].map((id) =>
      loader.loadAsync(
        `/__modular-character/parts/female_ranger_head_${id}.glb`
      )
    )
  )
  const models = { old: loaded[0].scene, new: loaded[1].scene }
  resources.push(models.old, models.new)
  if (disposed()) {
    disposeObjects([models.old, models.new])
    return
  }
  const materials = new Set<THREE.MeshStandardMaterial>()
  const counts: string[] = []
  for (const [id, model] of Object.entries(models)) {
    const bounds = new THREE.Box3().setFromObject(model)
    model.scale.setScalar(HEIGHT / (bounds.max.y - bounds.min.y))
    model.position.y = -bounds.min.y * model.scale.y
    let triangles = 0
    model.traverse((node) => {
      if (!(node instanceof THREE.Mesh)) return
      triangles +=
        (node.geometry.index?.count ??
          node.geometry.attributes.position.count) / 3
      for (const material of Array.isArray(node.material)
        ? node.material
        : [node.material])
        if (material instanceof THREE.MeshStandardMaterial)
          materials.add(material)
    })
    counts.push(
      `${id === 'old' ? '기존' : '새 머리'} ${triangles.toLocaleString()}삼각형`
    )
    scene.add(model)
  }
  document.getElementById('ranger-head-stats')!.textContent = counts.join(' · ')
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
  scene.environmentIntensity = 0.7
  cleanup.push(() => environment.dispose())
  scene.add(new THREE.HemisphereLight('#e1ecf2', '#62716b', 2))
  const key = new THREE.DirectionalLight('#fff1e0', 2.5)
  key.position.set(-3, 6, 5)
  const fill = new THREE.DirectionalLight('#b1cbe8', 1.6)
  fill.position.set(3, 3, -3)
  scene.add(key, fill)
  const camera = new THREE.PerspectiveCamera(25, 1, 0.01, 60)
  const controls = new OrbitControls(camera, renderer.domElement)
  controls.enableDamping = true
  controls.minDistance = 0.2
  controls.maxDistance = 20
  cleanup.push(() => controls.dispose())
  const views: Record<string, [number, number, number]> = {
    front: [0, 0.05, 1],
    angle: [0.65, 0.05, 0.76],
    side: [1, 0.05, 0],
    body: [0, 0.05, 1],
  }
  const params = new URLSearchParams(location.search)
  const mode = document.getElementById('ranger-head-mode') as HTMLSelectElement
  const view = document.getElementById('ranger-head-view') as HTMLSelectElement
  const requestedMode = params.get('mode')
  if (requestedMode && requestedMode in { both: 1, new: 1, old: 1 })
    mode.value = requestedMode
  const requestedView = params.get('view')
  if (requestedView && Object.hasOwn(views, requestedView))
    view.value = requestedView
  const reset = () => {
    const both = mode.value === 'both'
    const body = view.value === 'body'
    const gap = both ? (body ? 0.9 : 0.24) : 0
    models.old.visible = mode.value !== 'new'
    models.new.visible = mode.value !== 'old'
    models.old.position.x = -gap
    models.new.position.x = gap
    const height = body ? HEIGHT * 1.1 : 0.5
    const width = both ? gap * 2 + (body ? 1.5 : 0.45) : body ? 1.5 : 0.45
    const halfFov = THREE.MathUtils.degToRad(camera.fov / 2)
    const distance =
      Math.max(height, width / camera.aspect) / 2 / Math.tan(halfFov)
    controls.target.set(0, body ? HEIGHT / 2 : HEAD_Y, 0)
    camera.position
      .fromArray(views[view.value])
      .normalize()
      .multiplyScalar(distance)
      .add(controls.target)
    controls.update()
  }
  mode.onchange = reset
  view.onchange = reset
  document.getElementById('ranger-head-reset')!.onclick = reset
  const rotate = document.getElementById(
    'ranger-head-rotate'
  ) as HTMLInputElement
  rotate.onchange = () => {
    controls.autoRotate = rotate.checked
  }
  const wireframe = document.getElementById(
    'ranger-head-wireframe'
  ) as HTMLInputElement
  wireframe.onchange = () => {
    for (const material of materials) material.wireframe = wireframe.checked
  }
  const resize = () => {
    camera.aspect = host.clientWidth / Math.max(host.clientHeight, 1)
    camera.updateProjectionMatrix()
    renderer.setSize(host.clientWidth, host.clientHeight)
  }
  const observer = new ResizeObserver(() => {
    resize()
  })
  observer.observe(host)
  cleanup.push(() => observer.disconnect())
  resize()
  reset()
  renderer.setAnimationLoop(() => {
    controls.update()
    renderer.render(scene, camera)
  })
  status.textContent = '모델을 불러왔습니다.'
  const fieldset = document.getElementById(
    'ranger-head-controls'
  ) as HTMLFieldSetElement
  fieldset.disabled = false
  document.body.dataset.ready = 'true'
}
