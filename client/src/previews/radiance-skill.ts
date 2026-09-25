import * as THREE from 'three'
import { GLTFLoader, type GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import {
  createCharacterModelRoot,
  findBoneByName,
  groundRetargetedClips,
  retargetAnimationsForCharacterModel,
} from '../lib/utils/characterAnimationUtils'
import { mainHandBoneFor, poseMainHandProp } from '../lib/utils/handProps'
import {
  LIGHT_COOLDOWN,
  LIGHT_DURATION,
  LIGHT_FADE,
  LIGHT_WAKE,
  RadianceEffect,
} from './radiance-effect'

const el = <T extends HTMLElement>(id: string) => {
  const node = document.getElementById(id)
  if (!node) throw new Error(`Missing preview control: ${id}`)
  return node as T
}
const host = el('view')
const loading = el('loading')
const cast = el<HTMLButtonElement>('cast')
const pause = el<HTMLButtonElement>('pause')
const compare = el<HTMLButtonElement>('compare')
const expiry = el<HTMLButtonElement>('expiry')
const replay = el<HTMLButtonElement>('replay')
const scrub = el<HTMLInputElement>('scrub')
const power = el<HTMLInputElement>('power')
const color = el<HTMLSelectElement>('color')
const weapon = el<HTMLSelectElement>('weapon')
const cameraMode = el<HTMLSelectElement>('camera')
const movement = el<HTMLInputElement>('movement')
const slow = el<HTMLInputElement>('slow')
const phase = el('phase')
const remaining = el('remaining')
const buffState = el('buff-state')
const clock = el('clock')
const events = new AbortController()
const signal = events.signal
let disposed = false
let cleanup = () => events.abort()
function dispose() {
  disposed = true
  cleanup()
}
window.addEventListener('pagehide', dispose, { once: true })
if (import.meta.hot) import.meta.hot.dispose(dispose)

function groundMaterial(asset: GLTF) {
  let material: THREE.MeshStandardMaterial | undefined
  asset.scene.traverse((object) => {
    if (material || !(object instanceof THREE.Mesh)) return
    const candidate = Array.isArray(object.material)
      ? object.material[0]
      : object.material
    if (candidate instanceof THREE.MeshStandardMaterial)
      material = candidate.clone()
  })
  material ??= new THREE.MeshStandardMaterial({ color: '#57605b' })
  material.roughness = 0.95
  material.metalness = 0
  for (const value of Object.values(material)) {
    if (!(value instanceof THREE.Texture)) continue
    value.wrapS = value.wrapT = THREE.RepeatWrapping
    value.repeat.set(18, 18)
  }
  return material
}

function placeProp(
  source: THREE.Object3D,
  x: number,
  z: number,
  height: number,
  rotation = 0
) {
  const object = source.clone(true)
  const box = new THREE.Box3().setFromObject(object)
  const scale = height / Math.max(0.01, box.max.y - box.min.y)
  object.scale.multiplyScalar(scale)
  object.rotation.y = rotation
  object.position.set(x, -box.min.y * scale, z)
  object.traverse((child) => {
    if (child instanceof THREE.Mesh)
      child.castShadow = child.receiveShadow = true
  })
  return object
}

async function main() {
  const loader = new GLTFLoader()
  const [character, combat, locomotion, stone, rock, crate, tree] =
    await Promise.all([
      loader.loadAsync('/models/characters/knight.glb'),
      loader.loadAsync('/models/animations/combat_melee.glb'),
      loader.loadAsync('/models/animations/locomotion.glb'),
      loader.loadAsync('/textures/cobblestone_color_1k.glb'),
      loader.loadAsync('/models/objects/river_rock_01.glb'),
      loader.loadAsync('/models/objects/crate.glb'),
      loader.loadAsync('/models/vegetation/tree.glb'),
    ])
  if (disposed) return
  const idleSource = combat.animations.find(
    (clip) => clip.name === 'combat_idle'
  )
  const walkSource = locomotion.animations.find((clip) => clip.name === 'walk')
  if (!idleSource || !walkSource)
    throw new Error('기존 캐릭터 동작을 불러오지 못했습니다.')
  const retarget = async (pack: GLTF, clip: THREE.AnimationClip) =>
    (
      await groundRetargetedClips(
        character.scene,
        await retargetAnimationsForCharacterModel(character.scene, pack.scene, [
          clip,
        ])
      )
    )[0]
  const [idleClip, walkClip] = await Promise.all([
    retarget(combat, idleSource),
    retarget(locomotion, walkSource),
  ])
  if (disposed) return

  const renderer = new THREE.WebGLRenderer({ antialias: true })
  renderer.setPixelRatio(Math.min(devicePixelRatio, 1.75))
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  renderer.toneMappingExposure = 1.05
  renderer.shadowMap.enabled = true
  renderer.shadowMap.type = THREE.PCFShadowMap
  renderer.setClearColor('#080e17')
  host.prepend(renderer.domElement)
  const scene = new THREE.Scene()
  scene.fog = new THREE.FogExp2('#080e17', 0.027)
  scene.add(new THREE.HemisphereLight('#96b4de', '#252b29', 0.19))
  const moon = new THREE.DirectionalLight('#aac6fa', 0.28)
  moon.position.set(-10, 18, -5)
  scene.add(moon)
  const floor = new THREE.Mesh(
    new THREE.PlaneGeometry(64, 64),
    groundMaterial(stone)
  )
  floor.rotation.x = -Math.PI / 2
  floor.position.y = -0.015
  floor.receiveShadow = true
  scene.add(floor)

  for (const [x, z, height, rotation] of [
    [-3.1, 2.2, 0.6, 0.5],
    [3.7, 2.7, 0.8, -0.4],
    [4.7, -2.8, 1.05, 0.8],
    [-5.3, -1.5, 1.1, 0.3],
    [-7.6, 4.2, 0.7, 1.4],
    [8.8, 3.1, 1.3, 0.2],
    [-4.4, -6.3, 0.6, 1.4],
    [1.8, -6, 0.4, 0.6],
  ])
    scene.add(placeProp(rock.scene, x, z, height, rotation))
  scene.add(placeProp(crate.scene, 3.5, -0.5, 1, -0.25))
  scene.add(placeProp(crate.scene, 4.4, -1.1, 0.65, 0.2))
  for (const [x, z, height] of [
    [-7.5, -5, 7],
    [7, -7, 8],
    [-11, 1, 8],
    [12, -4, 8],
    [-3, -11, 8],
  ]) {
    scene.add(placeProp(tree.scene, x, z, height, x * 0.2))
  }
  const pillarMaterial = new THREE.MeshStandardMaterial({
    color: '#747571',
    roughness: 0.96,
  })
  const pillarGeometry = new THREE.CylinderGeometry(0.3, 0.38, 2.1, 8)
  for (const x of [-2.6, 2.6]) {
    const pillar = new THREE.Mesh(pillarGeometry, pillarMaterial)
    pillar.position.set(x, 1.05, -5.5)
    pillar.castShadow = pillar.receiveShadow = true
    scene.add(pillar)
  }

  const { modelRoot, clonedScene } = createCharacterModelRoot(character.scene)
  modelRoot.traverse((object) => {
    if (object instanceof THREE.Mesh)
      object.castShadow = object.receiveShadow = true
  })
  scene.add(modelRoot)
  const mixer = new THREE.AnimationMixer(modelRoot)
  const idle = mixer.clipAction(idleClip).play()
  const walk = mixer.clipAction(walkClip).play()
  idle.paused = walk.paused = true
  const weapons = new Map<string, GLTF>()
  let held: THREE.Object3D | null = null
  let weaponRequest = 0
  const setWeapon = async () => {
    const request = ++weaponRequest
    const value = weapon.value
    try {
      if (value !== 'none' && !weapons.has(value))
        weapons.set(
          value,
          await loader.loadAsync(`/models/weapons/${value}.glb`)
        )
      if (disposed || request !== weaponRequest) return
      held?.removeFromParent()
      held = null
      if (value !== 'none') {
        const item = value === 'sword' ? 'iron_sword' : value
        const hand = findBoneByName(clonedScene, mainHandBoneFor(item))
        if (!hand) throw new Error('무기를 표시할 손 본을 찾지 못했습니다.')
        held = weapons.get(value)!.scene.clone(true)
        poseMainHandProp(held, item)
        held.traverse((object) => {
          if (object instanceof THREE.Mesh)
            object.castShadow = object.receiveShadow = true
        })
        hand.add(held)
      }
    } catch (error) {
      loading.hidden = false
      loading.textContent = `무기 표시 실패: ${String(error)}`
      console.error(error)
    }
  }

  const effect = new RadianceEffect()
  scene.add(effect.group)
  const camera = new THREE.PerspectiveCamera(42, 1, 0.1, 90)
  const controls = new OrbitControls(camera, renderer.domElement)
  controls.minDistance = 4
  controls.maxDistance = 32
  controls.maxPolarAngle = Math.PI / 2 - 0.05
  const setCamera = () => {
    const mode = cameraMode.value
    camera.position.set(
      ...((mode === 'detail'
        ? [4, 4.5, 6.5]
        : mode === 'game'
          ? [0.8, 18, 13]
          : [10.4, 11.5, 14.8]) as [number, number, number])
    )
    controls.target.set(0, mode === 'detail' ? 1.8 : 0.6, -0.7)
    controls.update()
  }
  setCamera()
  cameraMode.addEventListener('change', setCamera, { signal })
  const resize = () => {
    camera.aspect = host.clientWidth / Math.max(1, host.clientHeight)
    camera.updateProjectionMatrix()
    renderer.setSize(host.clientWidth, host.clientHeight)
  }
  const observer = new ResizeObserver(resize)
  observer.observe(host)
  resize()
  let time = 0
  let playing = !window.matchMedia('(prefers-reduced-motion: reduce)').matches
  if (!playing) time = LIGHT_WAKE + 1
  let comparison = false
  let switchedOffAt: number | null = null
  let frame = 0
  let last = performance.now()
  const total = LIGHT_DURATION + 1.4
  const active = () => switchedOffAt === null && time < LIGHT_DURATION
  const cooldownRemaining = () =>
    Math.max(0, (switchedOffAt ?? 0) + LIGHT_COOLDOWN - time)
  const setPlaying = (value: boolean) => {
    playing = value
    pause.textContent = playing ? '일시정지' : '계속 재생'
  }
  const restart = () => {
    time = 0
    switchedOffAt = null
    comparison = false
    compare.setAttribute('aria-pressed', 'false')
    compare.textContent = '어둠과 비교'
    setPlaying(true)
  }
  cast.addEventListener(
    'click',
    () => {
      if (cooldownRemaining() > 0) return
      if (active()) {
        switchedOffAt = time
        setPlaying(true)
      } else restart()
    },
    { signal }
  )
  replay.addEventListener('click', restart, { signal })
  pause.addEventListener(
    'click',
    () => {
      if (time >= total) restart()
      else setPlaying(!playing)
    },
    { signal }
  )
  compare.addEventListener(
    'click',
    () => {
      comparison = !comparison
      compare.setAttribute('aria-pressed', String(comparison))
      compare.textContent = comparison ? '빛 다시 보기' : '어둠과 비교'
    },
    { signal }
  )
  scrub.addEventListener(
    'input',
    () => {
      time = Number(scrub.value)
      switchedOffAt = null
      setPlaying(false)
    },
    { signal }
  )
  expiry.addEventListener(
    'click',
    () => {
      switchedOffAt = null
      comparison = false
      compare.setAttribute('aria-pressed', 'false')
      compare.textContent = '어둠과 비교'
      time = LIGHT_DURATION - 3
      setPlaying(true)
    },
    { signal }
  )
  color.addEventListener('change', () => effect.setColor(color.value), {
    signal,
  })
  weapon.addEventListener(
    'change',
    () => {
      void setWeapon()
    },
    { signal }
  )
  setPlaying(playing)
  const powerValue = el('power-value')

  cleanup = () => {
    events.abort()
    cancelAnimationFrame(frame)
    observer.disconnect()
    controls.dispose()
    mixer.stopAllAction()
    mixer.uncacheRoot(modelRoot)
    effect.dispose()
    const geometries = new Set<THREE.BufferGeometry>()
    const materials = new Set<THREE.Material>()
    const textures = new Set<THREE.Texture>()
    const roots = [
      scene,
      ...[
        character,
        combat,
        locomotion,
        stone,
        rock,
        crate,
        tree,
        ...weapons.values(),
      ].map((asset) => asset.scene),
    ]
    roots.forEach((root) =>
      root.traverse((object) => {
        if (!(object instanceof THREE.Mesh || object instanceof THREE.Line))
          return
        geometries.add(object.geometry)
        for (const material of Array.isArray(object.material)
          ? object.material
          : [object.material]) {
          materials.add(material)
          Object.values(material).forEach((value) => {
            if (value instanceof THREE.Texture) textures.add(value)
          })
        }
      })
    )
    geometries.forEach((value) => value.dispose())
    materials.forEach((value) => value.dispose())
    textures.forEach((value) => value.dispose())
    renderer.dispose()
    renderer.domElement.remove()
    window.removeEventListener('pagehide', dispose)
  }
  const tick = (now: number) => {
    const dt = Math.min((now - last) / 1000, 0.06)
    last = now
    if (document.hidden) {
      frame = requestAnimationFrame(tick)
      return
    }
    if (playing) time += dt * (slow.checked ? 0.5 : 1)
    if (time > total) {
      time = total
      setPlaying(false)
    }
    const moving = movement.checked
    idle.setEffectiveWeight(moving ? 0 : 1)
    walk.setEffectiveWeight(moving ? 1 : 0)
    idle.time = time % idleClip.duration
    walk.time = (time * 1.2) % walkClip.duration
    mixer.update(0)
    const angle = time * 0.48
    modelRoot.position.set(
      moving ? Math.sin(angle) * 2.2 : 0,
      0,
      moving ? (Math.cos(angle) - 1) * 2.2 : 0
    )
    modelRoot.rotation.y = moving
      ? Math.atan2(Math.cos(angle), -Math.sin(angle))
      : 0.2
    effect.group.position.x = modelRoot.position.x
    effect.group.position.z = modelRoot.position.z
    effect.group.rotation.y = modelRoot.rotation.y
    effect.update(time, Number(power.value) / 100, !comparison, switchedOffAt)
    const buffActive = active()
    const cooldown = cooldownRemaining()
    const toggleLabel = buffActive ? '빛 끄기' : '빛 켜기'
    cast.textContent =
      cooldown > 0
        ? `${toggleLabel} (${(Math.ceil(cooldown * 10) / 10).toFixed(1)}s)`
        : toggleLabel
    cast.disabled = cooldown > 0
    cast.setAttribute('aria-pressed', String(buffActive))
    buffState.textContent = buffActive ? 'BUFF ACTIVE' : 'BUFF OFF'
    phase.textContent = comparison
      ? '비교 / 원래의 어두운 장면'
      : switchedOffAt !== null
        ? time < switchedOffAt + LIGHT_FADE
          ? 'OFF / 빛이 잦아듭니다'
          : 'OFF / 바로 다시 켤 수 있습니다'
        : time < LIGHT_WAKE
          ? '01 / 빛이 한 바퀴 돌고 사라집니다'
          : time < LIGHT_DURATION - LIGHT_FADE
            ? 'ON / 횃불을 대신하는 빛'
            : time < LIGHT_DURATION
              ? '03 / 지속 시간이 끝나갑니다'
              : 'OFF / 2분 경과 · 버프 해제'
    const seconds = buffActive
      ? Math.ceil(Math.max(0, LIGHT_DURATION - time))
      : 0
    remaining.textContent = `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
    scrub.max = String(total)
    scrub.value = String(time)
    clock.textContent = `${Math.min(time, LIGHT_DURATION).toFixed(1)} / ${LIGHT_DURATION}s`
    powerValue.textContent = `${power.value}%`
    renderer.render(scene, camera)
    frame = requestAnimationFrame(tick)
  }
  await setWeapon()
  if (disposed) {
    cleanup()
    return
  }
  loading.hidden = true
  cast.disabled =
    pause.disabled =
    compare.disabled =
    expiry.disabled =
    replay.disabled =
      false
  tick(performance.now())
}

main().catch((error: unknown) => {
  cleanup()
  loading.hidden = false
  loading.dataset.error = 'true'
  loading.textContent = `프리뷰를 준비하지 못했습니다: ${error instanceof Error ? error.message : String(error)}`
  console.error(error)
})
