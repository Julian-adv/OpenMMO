import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import {
  createCharacterModelRoot,
  findBoneByName,
  groundRetargetedClips,
  retargetAnimationsForCharacterModel,
} from '../lib/utils/characterAnimationUtils'
import { poseMainHandProp, poseOffHandProp } from '../lib/utils/handProps'
import {
  SwordGuardEffect,
  GUARD_EFFECT_DURATION,
} from '../lib/effects/sword-guard'
import {
  playAbilitySound,
  preloadAbilitySounds,
} from '../lib/managers/sfxManager'

const PREVIEW_DURATION = 2.5

const el = <T extends HTMLElement>(id: string) => {
  const element = document.getElementById(id)
  if (!element) throw new Error(`Missing preview element: ${id}`)
  return element as T
}
const actors = [
  {
    name: '시전자',
    role: 'Sword + Shield',
    model: 'knight',
    position: new THREE.Vector3(0, 0, 0),
    rotation: 0.25,
  },
  {
    name: '파티원 A',
    role: 'Rogue',
    model: 'rogue',
    position: new THREE.Vector3(-2.5, 0, 1.4),
    rotation: -0.4,
  },
  {
    name: '파티원 B',
    role: 'Priest',
    model: 'priest',
    position: new THREE.Vector3(2.1, 0, -2.2),
    rotation: 0.65,
  },
  {
    name: '파티원 C',
    role: 'Knight',
    model: 'knight',
    position: new THREE.Vector3(6.3, 0, 1.8),
    rotation: -0.5,
  },
]
const host = el('view')
const replay = el<HTMLButtonElement>('replay')
const pause = el<HTMLButtonElement>('pause')
const scrub = el<HTMLInputElement>('scrub')
const speed = el<HTMLSelectElement>('speed')
const loop = el<HTMLInputElement>('loop')
const vfx = el<HTMLInputElement>('vfx')
const radiusInput = el<HTMLInputElement>('radius')
const guideInput = el<HTMLInputElement>('guide')
const palette = el<HTMLSelectElement>('palette')
const cameraMode = el<HTMLSelectElement>('camera')
const status = el('status')
const events = new AbortController()
let cleanup = () => events.abort()
let disposed = false
const dispose = () => {
  disposed = true
  cleanup()
}
window.addEventListener('pagehide', dispose, { once: true })
if (import.meta.hot) import.meta.hot.dispose(dispose)

async function main() {
  const loader = new GLTFLoader()
  const names = ['knight', 'rogue', 'priest']
  const [characters, pack, sword, shield] = await Promise.all([
    Promise.all(
      names.map((name) => loader.loadAsync(`/models/characters/${name}.glb`))
    ),
    loader.loadAsync('/models/animations/combat_melee.glb'),
    loader.loadAsync('/models/weapons/sword.glb'),
    loader.loadAsync('/models/armor/shield_raven.glb'),
  ])
  const idleSource = pack.animations.find((clip) => clip.name === 'combat_idle')
  if (!idleSource)
    throw new Error('기존 combat_idle 애니메이션을 찾을 수 없습니다.')
  const clips = await Promise.all(
    characters.map(async (character) => {
      const retargeted = await retargetAnimationsForCharacterModel(
        character.scene,
        pack.scene,
        [idleSource]
      )
      return (await groundRetargetedClips(character.scene, retargeted))[0]
    })
  )
  if (disposed) return

  const renderer = new THREE.WebGLRenderer({ antialias: true })
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2))
  renderer.setClearColor('#172024')
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  renderer.shadowMap.enabled = true
  renderer.shadowMap.type = THREE.PCFShadowMap
  host.prepend(renderer.domElement)
  const scene = new THREE.Scene()
  scene.fog = new THREE.Fog('#172024', 26, 53)
  scene.add(new THREE.HemisphereLight('#dbe8ed', '#596165', 2.3))
  const light = new THREE.DirectionalLight('#fff0d7', 3)
  light.position.set(-5, 10, 6)
  light.castShadow = true
  light.shadow.mapSize.set(2048, 2048)
  Object.assign(light.shadow.camera, {
    left: -12,
    right: 12,
    top: 12,
    bottom: -12,
  })
  light.shadow.bias = -0.0002
  scene.add(light)
  const fill = new THREE.DirectionalLight('#a6c7e7', 1.7)
  fill.position.set(5, 4, -6)
  scene.add(fill)
  const floor = new THREE.Mesh(
    new THREE.PlaneGeometry(100, 100),
    new THREE.MeshStandardMaterial({ color: '#293337', roughness: 1 })
  )
  floor.rotation.x = -Math.PI / 2
  floor.position.y = -0.015
  floor.receiveShadow = true
  scene.add(floor)
  const grid = new THREE.GridHelper(40, 40, '#3c4749', '#354044')
  grid.position.y = -0.01
  scene.add(grid)
  const guide = new THREE.Group()
  const guidePoints = Array.from(
    { length: 181 },
    (_, i) =>
      new THREE.Vector3(
        Math.cos((i / 180) * Math.PI * 2),
        0.01,
        Math.sin((i / 180) * Math.PI * 2)
      )
  )
  const rangeLine = new THREE.Line(
    new THREE.BufferGeometry().setFromPoints(guidePoints),
    new THREE.LineDashedMaterial({
      color: '#c5b689',
      dashSize: 0.06,
      gapSize: 0.04,
      transparent: true,
      opacity: 0.33,
    })
  )
  rangeLine.computeLineDistances()
  guide.add(rangeLine)
  scene.add(guide)

  const actorViews = actors.map((actor, index) => {
    const modelIndex = names.indexOf(actor.model)
    const { modelRoot, clonedScene } = createCharacterModelRoot(
      characters[modelIndex].scene
    )
    modelRoot.position.copy(actor.position)
    modelRoot.rotation.y = actor.rotation
    scene.add(modelRoot)
    let shieldObject: THREE.Object3D | null = null
    if (index === 0) {
      const rightHand = findBoneByName(clonedScene, 'RightHand')
      const leftHand = findBoneByName(clonedScene, 'LeftHand')
      if (!rightHand || !leftHand)
        throw new Error('기사의 손 본을 찾을 수 없습니다.')
      const weapon = sword.scene.clone(true)
      poseMainHandProp(weapon, 'sword')
      rightHand.add(weapon)
      shieldObject = shield.scene.clone(true)
      poseOffHandProp(shieldObject)
      leftHand.add(shieldObject)
    }
    const mixer = new THREE.AnimationMixer(modelRoot)
    const action = mixer.clipAction(clips[modelIndex]).play()
    action.paused = true
    const label = document.createElement('div')
    label.className = 'actor-label'
    const labelName = document.createElement('span')
    labelName.textContent = actor.name
    label.append(labelName)
    const labelState = document.createElement('small')
    label.append(labelState)
    el('labels').append(label)
    const row = document.createElement('li')
    const text = document.createElement('div')
    text.textContent = actor.name
    const role = document.createElement('small')
    role.textContent = `${actor.role} · ${actor.position.length().toFixed(1)} m`
    text.append(role)
    const state = document.createElement('span')
    state.className = 'party-state'
    row.append(text, state)
    el('party').append(row)
    return {
      ...actor,
      modelRoot,
      mixer,
      action,
      shieldObject,
      label,
      labelState,
      row,
      state,
    }
  })
  const effect = new SwordGuardEffect(actors.map((actor) => actor.position))
  scene.add(effect.group)
  const camera = new THREE.PerspectiveCamera(39, 1, 0.05, 100)
  const controls = new OrbitControls(camera, renderer.domElement)
  controls.minDistance = 3
  controls.maxDistance = 32
  controls.maxPolarAngle = Math.PI / 2 - 0.08
  controls.enablePan = true
  const setCamera = () => {
    const mode = cameraMode.value
    if (mode === 'detail') {
      camera.position.set(3.2, 3.5, 5.4)
      controls.target.set(0, 1, 0)
    } else {
      camera.position.set(
        mode === 'game' ? 1.3 : 9,
        mode === 'game' ? 18 : 11.5,
        mode === 'game' ? 14 : 17
      )
      controls.target.set(1.4, 0.45, 0)
    }
    controls.update()
  }
  setCamera()
  cameraMode.addEventListener('change', setCamera, { signal: events.signal })
  const resize = () => {
    camera.aspect = host.clientWidth / Math.max(1, host.clientHeight)
    camera.updateProjectionMatrix()
    renderer.setSize(host.clientWidth, host.clientHeight)
  }
  const observer = new ResizeObserver(resize)
  observer.observe(host)
  resize()
  let time = 0
  preloadAbilitySounds()
  scrub.max = String(PREVIEW_DURATION)
  let playing = true
  let last = performance.now()
  let frame = 0
  const setPlaying = (value: boolean) => {
    playing = value
    pause.textContent = value ? '일시정지' : '계속 재생'
  }
  const restart = () => {
    time = 0
    setPlaying(true)
  }
  replay.disabled = pause.disabled = false
  replay.addEventListener(
    'click',
    () => {
      restart()
      if (Number(speed.value) === 1) playAbilitySound('guardian_ward')
    },
    { signal: events.signal }
  )
  pause.addEventListener(
    'click',
    () => {
      if (!playing && time >= PREVIEW_DURATION) time = 0
      setPlaying(!playing)
    },
    { signal: events.signal }
  )
  scrub.addEventListener(
    'input',
    () => {
      time = Number(scrub.value)
      setPlaying(false)
    },
    { signal: events.signal }
  )
  radiusInput.addEventListener('input', restart, { signal: events.signal })
  palette.addEventListener(
    'change',
    () => {
      effect.setPalette(palette.value)
      document.documentElement.style.setProperty(
        '--accent',
        palette.value === 'silver' ? '#c9deed' : '#dfc99a'
      )
    },
    { signal: events.signal }
  )
  const projection = new THREE.Vector3()
  const shieldPosition = new THREE.Vector3()
  const clockText = el('time')
  const phaseText = el('phase')
  const radiusText = el('radius-value')
  status.textContent = '기존 기사·도적·사제 / 검·방패 / 전투 대기 동작 사용'
  const tick = (now: number) => {
    const dt = Math.min((now - last) / 1000, 0.05)
    last = now
    if (playing && !document.hidden) time += dt * Number(speed.value)
    if (time > PREVIEW_DURATION) {
      if (loop.checked) time %= PREVIEW_DURATION
      else {
        time = PREVIEW_DURATION
        setPlaying(false)
      }
    }
    const radius = Number(radiusInput.value)
    guide.visible = guideInput.checked
    guide.scale.setScalar(radius)
    radiusText.textContent = `${radius.toFixed(1)} m`
    for (let i = 0; i < actorViews.length; i++) {
      const actor = actorViews[i]
      actor.action.time = (time + i * 0.6) % actor.action.getClip().duration
      actor.mixer.update(0)
      actor.modelRoot.updateMatrixWorld(true)
      const inRange = actor.position.length() <= radius
      const buffed = inRange && time > 0
      const text = buffed ? '방어력 +10%' : !inRange ? '범위 밖' : '범위 내'
      actor.state.textContent = actor.labelState.textContent = text
      actor.state.dataset.buffed = actor.label.dataset.buffed = String(buffed)
      projection
        .copy(actor.position)
        .add(new THREE.Vector3(0, 2.9, 0))
        .project(camera)
      actor.label.hidden = projection.z > 1 || projection.z < -1
      actor.label.style.transform = `translate(${(projection.x * 0.5 + 0.5) * host.clientWidth}px, ${(-projection.y * 0.5 + 0.5) * host.clientHeight}px) translate(-50%, -100%)`
    }
    actorViews[0].shieldObject!.getWorldPosition(shieldPosition)
    effect.group.visible = vfx.checked
    effect.update(time, radius, camera, shieldPosition)
    scrub.value = String(time)
    clockText.textContent = `${time.toFixed(2)} / ${PREVIEW_DURATION.toFixed(1)}s`
    phaseText.textContent =
      time < 0.1
        ? '01 / 방패가 즉시 맺힙니다'
        : time < 0.35
          ? '02 / 짧은 두 번째 박자'
          : time < GUARD_EFFECT_DURATION
            ? '03 / 소리의 여운과 함께 잦아듭니다'
            : '연출 종료 · 버프 유지'
    renderer.render(scene, camera)
    frame = requestAnimationFrame(tick)
  }
  cleanup = () => {
    events.abort()
    cancelAnimationFrame(frame)
    observer.disconnect()
    controls.dispose()
    effect.dispose()
    const geometries = new Set<THREE.BufferGeometry>()
    const materials = new Set<THREE.Material>()
    const textures = new Set<THREE.Texture>()
    scene.traverse((object) => {
      if (!(object instanceof THREE.Mesh || object instanceof THREE.Line))
        return
      geometries.add(object.geometry)
      for (const material of Array.isArray(object.material)
        ? object.material
        : [object.material]) {
        materials.add(material)
        for (const value of Object.values(material))
          if (value instanceof THREE.Texture) textures.add(value)
      }
    })
    geometries.forEach((geometry) => geometry.dispose())
    materials.forEach((material) => material.dispose())
    textures.forEach((texture) => texture.dispose())
    actorViews.forEach((actor) => {
      actor.mixer.stopAllAction()
      actor.mixer.uncacheRoot(actor.modelRoot)
      actor.label.remove()
      actor.row.remove()
    })
    light.shadow.dispose()
    renderer.dispose()
    renderer.domElement.remove()
    window.removeEventListener('pagehide', dispose)
  }
  tick(performance.now())
}

main().catch((error: unknown) => {
  cleanup()
  status.dataset.error = 'true'
  status.textContent = `프리뷰를 준비하지 못했습니다: ${error instanceof Error ? error.message : String(error)}`
  console.error(error)
})
