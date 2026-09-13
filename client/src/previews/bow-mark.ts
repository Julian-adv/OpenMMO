import * as THREE from 'three'
import { GLTFLoader, type GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import {
  createCharacterModelRoot,
  findBoneByName,
  groundRetargetedClips,
  retargetAnimationsForCharacterModel,
} from '../lib/utils/characterAnimationUtils'
import {
  mainHandBoneFor,
  poseMainHandProp,
  forearmLength,
} from '../lib/utils/handProps'
import {
  BowMarkEffect,
  MARK_LOCK,
  MARK_END,
  MARK_FADE,
  MARK_PREVIEW_DURATION,
} from './bow-mark-effect'

const el = <T extends HTMLElement>(id: string) => {
  const element = document.getElementById(id)
  if (!element) throw new Error(`Missing preview control: ${id}`)
  return element as T
}
const host = el('view')
const loading = el('loading')
const cast = el<HTMLButtonElement>('cast')
const pause = el<HTMLButtonElement>('pause')
const expiry = el<HTMLButtonElement>('expiry')
const scrub = el<HTMLInputElement>('scrub')
const targetSelect = el<HTMLSelectElement>('target')
const cameraMode = el<HTMLSelectElement>('camera')
const palette = el<HTMLSelectElement>('palette')
const size = el<HTMLInputElement>('size')
const sizeValue = el('size-value')
const movement = el<HTMLInputElement>('movement')
const slow = el<HTMLInputElement>('slow')
const loop = el<HTMLInputElement>('loop')
const vfx = el<HTMLInputElement>('vfx')
const phase = el('phase')
const markState = el('mark-state')
const targetName = el('target-name')
const clock = el('clock')
const events = new AbortController()
const { signal } = events
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
  material ??= new THREE.MeshStandardMaterial({ color: '#4e5550' })
  material.roughness = 0.96
  material.metalness = 0
  material.color.multiplyScalar(0.28)
  for (const value of Object.values(material)) {
    if (!(value instanceof THREE.Texture)) continue
    value.wrapS = value.wrapT = THREE.RepeatWrapping
    value.repeat.set(18, 18)
  }
  return material
}

async function main() {
  const loader = new GLTFLoader()
  const [ranger, ranged, melee, bow, orc, goblin, stone, rock, crate] =
    await Promise.all([
      loader.loadAsync('/models/characters/ranger.glb'),
      loader.loadAsync('/models/animations/combat_ranged.glb'),
      loader.loadAsync('/models/animations/combat_melee.glb'),
      loader.loadAsync('/models/weapons/bow.glb'),
      loader.loadAsync('/models/monsters/orc.glb'),
      loader.loadAsync('/models/monsters/goblin.glb'),
      loader.loadAsync('/textures/cobblestone_color_1k.glb'),
      loader.loadAsync('/models/objects/river_rock_01.glb'),
      loader.loadAsync('/models/objects/crate.glb'),
    ])
  if (disposed) return
  const retarget = async (asset: GLTF, name: string) => {
    const clip = asset.animations.find((clip) => clip.name === name)
    if (!clip) throw new Error(`기존 ${name} 애니메이션을 찾을 수 없습니다.`)
    return (
      await groundRetargetedClips(
        ranger.scene,
        await retargetAnimationsForCharacterModel(ranger.scene, asset.scene, [
          clip,
        ])
      )
    )[0]
  }
  const [aimClip, idleClip] = await Promise.all([
    retarget(ranged, 'bow_shoot'),
    retarget(melee, 'combat_idle'),
  ])
  if (disposed) return

  const renderer = new THREE.WebGLRenderer({ antialias: true })
  renderer.setPixelRatio(Math.min(devicePixelRatio, 1.75))
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  renderer.shadowMap.enabled = true
  renderer.shadowMap.type = THREE.PCFSoftShadowMap
  renderer.setClearColor('#141d20')
  host.prepend(renderer.domElement)
  const scene = new THREE.Scene()
  scene.fog = new THREE.FogExp2('#141d20', 0.025)
  scene.add(new THREE.HemisphereLight('#bfcedb', '#474333', 1.8))
  const sun = new THREE.DirectionalLight('#ffe6bc', 2.5)
  sun.position.set(-5, 10, 6)
  sun.castShadow = true
  sun.shadow.mapSize.set(2048, 2048)
  Object.assign(sun.shadow.camera, {
    left: -12,
    right: 12,
    top: 12,
    bottom: -12,
  })
  sun.shadow.bias = -0.0003
  scene.add(sun)
  const rim = new THREE.DirectionalLight('#aac8dd', 1.1)
  rim.position.set(4, 6, -8)
  scene.add(rim)
  const fill = new THREE.DirectionalLight('#dce8eb', 1.5)
  fill.position.set(0, 5, 10)
  scene.add(fill)
  const floor = new THREE.Mesh(
    new THREE.PlaneGeometry(60, 60),
    groundMaterial(stone)
  )
  floor.rotation.x = -Math.PI / 2
  floor.position.y = -0.015
  floor.receiveShadow = true
  scene.add(floor)
  const placeProp = (
    source: THREE.Object3D,
    x: number,
    z: number,
    height: number
  ) => {
    const model = source.clone(true)
    const box = new THREE.Box3().setFromObject(model)
    const scale = height / Math.max(0.01, box.max.y - box.min.y)
    model.scale.multiplyScalar(scale)
    model.position.set(x, -box.min.y * scale, z)
    model.rotation.y = x * 0.7
    model.traverse((object) => {
      if (object instanceof THREE.Mesh)
        object.castShadow = object.receiveShadow = true
    })
    scene.add(model)
  }
  for (const [x, z, height] of [
    [-5, -3, 0.7],
    [-2, -5, 0.6],
    [5.5, 3, 0.65],
    [7, -5, 0.95],
    [-6, 4, 0.9],
  ])
    placeProp(rock.scene, x, z, height)
  placeProp(crate.scene, -3.8, -3.2, 0.8)
  placeProp(crate.scene, -4.6, -3.6, 0.55)

  const { modelRoot: casterRoot, clonedScene } = createCharacterModelRoot(
    ranger.scene
  )
  casterRoot.position.set(-2.9, 0, 1.25)
  const handName = mainHandBoneFor('bow')
  const hand = findBoneByName(clonedScene, handName)
  if (!hand) throw new Error('레인저의 활을 장착할 손 본을 찾지 못했습니다.')
  const heldBow = bow.scene.clone(true)
  poseMainHandProp(
    heldBow,
    'bow',
    forearmLength(clonedScene, handName, 'bow-preview-ranger')
  )
  hand.add(heldBow)
  scene.add(casterRoot)
  const casterMixer = new THREE.AnimationMixer(casterRoot)
  const idle = casterMixer.clipAction(idleClip).play()
  const aim = casterMixer.clipAction(aimClip).play()
  idle.paused = aim.paused = true

  const targets = [
    { id: 'orc', asset: orc, height: 2.1, x: 1.3, z: 0 },
    { id: 'goblin', asset: goblin, height: 1.65, x: 4.0, z: -2.4 },
  ].map((spec) => {
    const { modelRoot, clonedScene } = createCharacterModelRoot(
      spec.asset.scene
    )
    const box = new THREE.Box3().setFromObject(modelRoot)
    modelRoot.scale.setScalar(
      spec.height / Math.max(0.01, box.max.y - box.min.y)
    )
    clonedScene.position.y -= box.min.y
    modelRoot.position.set(spec.x, 0, spec.z)
    scene.add(modelRoot)
    const mixer = new THREE.AnimationMixer(modelRoot)
    const idleClip = spec.asset.animations.find(
      (clip) => clip.name === 'Idle_Loop_Rig'
    )
    const walkClip = spec.asset.animations.find(
      (clip) => clip.name === 'Walk_Loop_Rig'
    )
    if (!idleClip || !walkClip)
      throw new Error(`${spec.id}의 기존 동작을 찾지 못했습니다.`)
    const idle = mixer.clipAction(idleClip).play()
    const walk = mixer.clipAction(walkClip).play()
    idle.paused = walk.paused = true
    return {
      ...spec,
      root: modelRoot,
      mixer,
      idle,
      walk,
      label: el(`${spec.id}-label`),
    }
  })
  let selected = targets[0]
  const effect = new BowMarkEffect()
  scene.add(effect.group)
  const anchor = new THREE.Vector3()
  const projected = new THREE.Vector3()
  const casterLabel = el('caster-label')
  const camera = new THREE.PerspectiveCamera(40, 1, 0.1, 80)
  const controls = new OrbitControls(camera, renderer.domElement)
  controls.minDistance = 3
  controls.maxDistance = 28
  controls.maxPolarAngle = Math.PI / 2 - 0.06
  const setCamera = () => {
    if (cameraMode.value === 'detail') {
      controls.target
        .copy(selected.root.position)
        .add(new THREE.Vector3(0, 1.7, 0))
      camera.position.copy(controls.target).add(new THREE.Vector3(0, 3.2, 5.8))
    } else {
      controls.target.set(0.6, 0.6, -0.4)
      camera.position.set(
        ...((cameraMode.value === 'scene' ? [8, 7, 12] : [0.6, 13.8, 11.3]) as [
          number,
          number,
          number,
        ])
      )
    }
    controls.update()
  }
  setCamera()
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
  if (!playing) {
    time = MARK_LOCK + 1
    loop.checked = false
  }
  let frame = 0
  let last = performance.now()
  const setPlaying = (value: boolean) => {
    playing = value
    pause.textContent = value ? '일시정지' : '계속 재생'
  }
  const restart = () => {
    time = 0
    setPlaying(true)
  }
  const chooseTarget = (id: string) => {
    selected = targets.find((target) => target.id === id) ?? targets[0]
    targetSelect.value = selected.id
    targetName.textContent = selected.id.toUpperCase()
    if (cameraMode.value === 'detail') setCamera()
    restart()
  }
  cast.addEventListener('click', restart, { signal })
  pause.addEventListener(
    'click',
    () => {
      if (time >= MARK_PREVIEW_DURATION) restart()
      else setPlaying(!playing)
    },
    { signal }
  )
  expiry.addEventListener(
    'click',
    () => {
      time = MARK_END - 0.7
      setPlaying(true)
    },
    { signal }
  )
  scrub.addEventListener(
    'input',
    () => {
      time = Number(scrub.value)
      setPlaying(false)
    },
    { signal }
  )
  targetSelect.addEventListener(
    'change',
    () => chooseTarget(targetSelect.value),
    { signal }
  )
  cameraMode.addEventListener('change', setCamera, { signal })
  const setPalette = () => {
    effect.setColor(palette.value)
    document.documentElement.style.setProperty('--accent', palette.value)
  }
  palette.addEventListener('change', setPalette, { signal })
  setPalette()
  const pointerDown = new THREE.Vector2()
  const pointer = new THREE.Vector2()
  const raycaster = new THREE.Raycaster()
  renderer.domElement.addEventListener(
    'pointerdown',
    (event) => pointerDown.set(event.clientX, event.clientY),
    { signal }
  )
  renderer.domElement.addEventListener(
    'pointerup',
    (event) => {
      if (
        event.button !== 0 ||
        pointerDown.distanceTo(
          new THREE.Vector2(event.clientX, event.clientY)
        ) > 5
      )
        return
      const rect = renderer.domElement.getBoundingClientRect()
      pointer.set(
        ((event.clientX - rect.left) / rect.width) * 2 - 1,
        (-(event.clientY - rect.top) / rect.height) * 2 + 1
      )
      raycaster.setFromCamera(pointer, camera)
      const hit = raycaster.intersectObjects(
        targets.map((target) => target.root),
        true
      )[0]
      if (!hit) return
      let object: THREE.Object3D | null = hit.object
      while (object) {
        const target = targets.find((target) => target.root === object)
        if (target) {
          chooseTarget(target.id)
          return
        }
        object = object.parent
      }
    },
    { signal }
  )
  setPlaying(playing)
  const positionLabel = (label: HTMLElement, root: THREE.Object3D) => {
    projected.copy(root.position).project(camera)
    label.hidden =
      Math.abs(projected.x) > 1 ||
      Math.abs(projected.y) > 0.95 ||
      Math.abs(projected.z) > 1
    label.style.left = `${((projected.x + 1) / 2) * host.clientWidth}px`
    label.style.top = `${((-projected.y + 1) / 2) * host.clientHeight}px`
  }

  cleanup = () => {
    events.abort()
    cancelAnimationFrame(frame)
    observer.disconnect()
    controls.dispose()
    casterMixer.stopAllAction()
    casterMixer.uncacheRoot(casterRoot)
    targets.forEach((target) => {
      target.mixer.stopAllAction()
      target.mixer.uncacheRoot(target.root)
    })
    effect.group.removeFromParent()
    effect.dispose()
    const geometries = new Set<THREE.BufferGeometry>()
    const materials = new Set<THREE.Material>()
    const textures = new Set<THREE.Texture>()
    const roots = [
      scene,
      ...[ranger, ranged, melee, bow, orc, goblin, stone, rock, crate].map(
        (asset) => asset.scene
      ),
    ]
    roots.forEach((root) =>
      root.traverse((object) => {
        if (!(object instanceof THREE.Mesh)) return
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
    sun.shadow.dispose()
    renderer.dispose()
    renderer.domElement.remove()
    window.removeEventListener('pagehide', dispose)
  }
  const tick = (now: number) => {
    const dt = Math.max(0, Math.min((now - last) / 1000, 0.06))
    last = now
    if (document.hidden) {
      frame = requestAnimationFrame(tick)
      return
    }
    if (playing) time += dt * (slow.checked ? 0.25 : 1)
    if (time > MARK_PREVIEW_DURATION) {
      if (loop.checked) time %= MARK_PREVIEW_DURATION
      else {
        time = MARK_PREVIEW_DURATION
        setPlaying(false)
      }
    }
    for (const target of targets) {
      const moving = movement.checked
      const angle = time * 0.65
      target.root.position.set(
        target.x + (moving ? Math.sin(angle) * 0.8 : 0),
        0,
        target.z + (moving ? Math.cos(angle) * 0.45 : 0)
      )
      target.root.rotation.y = moving
        ? Math.atan2(Math.cos(angle) * 0.8, -Math.sin(angle) * 0.45)
        : -1.1
      target.idle.setEffectiveWeight(moving ? 0 : 1)
      target.walk.setEffectiveWeight(moving ? 1 : 0)
      target.idle.time = time % target.idle.getClip().duration
      target.walk.time = time % target.walk.getClip().duration
      target.mixer.update(0)
      positionLabel(target.label, target.root)
      const marked =
        target === selected && time < MARK_END + MARK_FADE && vfx.checked
      target.label.classList.toggle('selected', marked)
      target.label.querySelector('span')!.textContent = marked
        ? '지정 대상'
        : '표식 없음'
    }
    const aimWeight = 1 - THREE.MathUtils.smoothstep(time, 0.65, 0.95)
    idle.setEffectiveWeight(1 - aimWeight)
    aim.setEffectiveWeight(aimWeight)
    idle.time = time % idleClip.duration
    aim.time = Math.min(time * 0.4, aimClip.duration * 0.27)
    casterMixer.update(0)
    casterRoot.rotation.y = Math.atan2(
      selected.root.position.x - casterRoot.position.x,
      selected.root.position.z - casterRoot.position.z
    )
    positionLabel(casterLabel, casterRoot)
    anchor.copy(selected.root.position)
    anchor.y += selected.height + (0.62 * Number(size.value)) / 100
    effect.update(time, anchor, camera, Number(size.value) / 100)
    if (!vfx.checked) effect.group.visible = false
    phase.textContent = !vfx.checked
      ? '다른 플레이어에게는 표식이 보이지 않습니다'
      : time < MARK_LOCK
        ? '01 / 조준점이 모이며 대상을 고정합니다'
        : time < MARK_END
          ? '02 / 본인에게만 보이는 표식이 맥동합니다'
          : time < MARK_END + MARK_FADE
            ? '03 / 표식이 부드럽게 사라집니다'
            : '표식 해제'
    markState.textContent = !vfx.checked
      ? 'OTHER PLAYER VIEW'
      : time < MARK_LOCK
        ? 'ACQUIRING'
        : time < MARK_END
          ? 'MARKED'
          : 'RELEASED'
    scrub.value = String(time)
    clock.textContent = `${time.toFixed(2)} / ${MARK_PREVIEW_DURATION.toFixed(2)} s`
    sizeValue.textContent = `${size.value}%`
    renderer.render(scene, camera)
    frame = requestAnimationFrame(tick)
  }
  loading.hidden = true
  cast.disabled = pause.disabled = expiry.disabled = false
  tick(performance.now())
}

main().catch((error: unknown) => {
  cleanup()
  loading.hidden = false
  loading.dataset.error = 'true'
  loading.textContent = `프리뷰를 준비하지 못했습니다: ${error instanceof Error ? error.message : String(error)}`
  console.error(error)
})
