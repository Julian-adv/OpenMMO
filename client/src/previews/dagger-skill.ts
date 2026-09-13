import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import {
  createCharacterModelRoot,
  findBoneByName,
  groundRetargetedClips,
  retargetAnimationsForCharacterModel,
} from '../lib/utils/characterAnimationUtils'
import { poseMainHandProp } from '../lib/utils/handProps'
import { DaggerSlashEffect, type BladeSample } from './dagger-slash-effect'
import { createDaggerComboClip } from '../lib/utils/daggerSkillAnimation'
import { DAGGER_SKILL } from '../lib/data/daggerSkill'

const DURATION = DAGGER_SKILL.duration
const HITS = [...DAGGER_SKILL.hits]
const el = <T extends HTMLElement>(id: string) => {
  const element = document.getElementById(id)
  if (!element) throw new Error(`Missing preview element: ${id}`)
  return element as T
}
const replay = el<HTMLButtonElement>('replay')
const pause = el<HTMLButtonElement>('pause')
const speed = el<HTMLSelectElement>('speed')
const cameraMode = el<HTMLSelectElement>('camera')
const loop = el<HTMLInputElement>('loop')
const vfx = el<HTMLInputElement>('vfx')
const scrub = el<HTMLInputElement>('scrub')
const status = el('status')

async function main() {
  const loader = new GLTFLoader()
  const [character, pack, dagger] = await Promise.all([
    loader.loadAsync('/models/characters/rogue.glb'),
    loader.loadAsync(DAGGER_SKILL.pack),
    loader.loadAsync('/models/weapons/dagger.glb'),
  ])
  const clips = await groundRetargetedClips(
    character.scene,
    await retargetAnimationsForCharacterModel(
      character.scene,
      pack.scene,
      pack.animations
    )
  )
  const inward = clips.find((c) => c.name === 'dagger_inward')
  const outward = clips.find((c) => c.name === 'dagger_outward')
  if (!inward || !outward) throw new Error('두 베기 클립을 찾을 수 없습니다.')
  const combo = createDaggerComboClip(inward, outward)

  const views = ['view-a', 'view-b'].map((id, i) => {
    const host = el(id)
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false })
    renderer.setPixelRatio(Math.min(devicePixelRatio, 2))
    renderer.setClearColor('#111d26')
    renderer.toneMapping = THREE.ACESFilmicToneMapping
    renderer.shadowMap.enabled = true
    renderer.shadowMap.type = THREE.PCFSoftShadowMap
    host.append(renderer.domElement)
    const scene = new THREE.Scene()
    scene.add(new THREE.HemisphereLight('#d1e7f0', '#47565a', 2.5))
    const light = new THREE.DirectionalLight('#fff0d7', 3.2)
    light.position.set(-3, 7, 4)
    light.castShadow = true
    light.shadow.mapSize.set(1024, 1024)
    light.shadow.camera.left = light.shadow.camera.bottom = -3
    light.shadow.camera.right = light.shadow.camera.top = 3
    light.shadow.bias = -0.0003
    scene.add(light)
    const rim = new THREE.DirectionalLight('#8fbfff', 2)
    rim.position.set(2, 3, -3)
    scene.add(rim)
    const fill = new THREE.DirectionalLight('#daeaff', 2)
    fill.position.set(2, 2, 5)
    scene.add(fill)
    const floor = new THREE.Mesh(
      new THREE.CircleGeometry(3.5, 80),
      new THREE.MeshStandardMaterial({ color: '#23333e', roughness: 1 })
    )
    floor.rotation.x = -Math.PI / 2
    floor.position.y = -0.012
    floor.receiveShadow = true
    scene.add(floor)
    const ring = new THREE.Mesh(
      new THREE.RingGeometry(1.45, 1.459, 96),
      new THREE.MeshBasicMaterial({
        color: '#526976',
        transparent: true,
        opacity: 0.45,
        side: THREE.DoubleSide,
      })
    )
    ring.rotation.x = -Math.PI / 2
    ring.position.y = -0.009
    scene.add(ring)
    const { modelRoot, clonedScene } = createCharacterModelRoot(character.scene)
    scene.add(modelRoot)
    const hand = findBoneByName(clonedScene, 'RightHand')
    if (!hand) throw new Error('캐릭터의 오른손 본을 찾을 수 없습니다.')
    const weapon = dagger.scene.clone(true)
    poseMainHandProp(weapon, 'dagger')
    hand.add(weapon)
    const mixer = new THREE.AnimationMixer(modelRoot)
    const action = mixer.clipAction(combo).play()
    action.paused = true
    const pose = (time: number) => {
      action.time = Math.min(time, DURATION)
      mixer.update(0)
      modelRoot.updateMatrixWorld(true)
    }
    const samples: BladeSample[] = []
    for (let frame = 0; frame <= Math.ceil(DURATION * 240); frame++) {
      const time = frame / 240
      pose(time)
      samples.push({
        time,
        base: weapon.localToWorld(new THREE.Vector3(0.075, 0, 0)),
        tip: weapon.localToWorld(new THREE.Vector3(0.378, 0, 0)),
      })
    }
    pose(0)
    const effect = new DaggerSlashEffect(samples, HITS, i === 1)
    scene.add(effect.group)
    const camera = new THREE.PerspectiveCamera(35, 1, 0.05, 100)
    const controls = new OrbitControls(camera, renderer.domElement)
    controls.target.set(0, 0.95, 0)
    controls.minDistance = 2.5
    controls.maxDistance = 12
    controls.maxPolarAngle = Math.PI / 2 - 0.03
    controls.enablePan = false
    const resize = () => {
      camera.aspect = host.clientWidth / host.clientHeight
      camera.updateProjectionMatrix()
      renderer.setSize(host.clientWidth, host.clientHeight)
    }
    const observer = new ResizeObserver(resize)
    observer.observe(host)
    resize()
    return { scene, renderer, camera, controls, pose, effect, observer, mixer }
  })
  for (const view of views) {
    view.controls.addEventListener('change', () => {
      const other = views.find((v) => v !== view)!
      other.camera.position.copy(view.camera.position)
      other.camera.quaternion.copy(view.camera.quaternion)
      other.controls.target.copy(view.controls.target)
    })
  }
  const setCamera = () => {
    const mode = cameraMode.value
    for (const view of views) {
      view.camera.position.set(
        ...((mode === 'game'
          ? [0, 7.5, 10.6]
          : mode === 'front'
            ? [0, 1.7, 5.4]
            : [2.7, 2.8, 4.3]) as [number, number, number])
      )
      view.controls.target.set(0, 0.95, 0)
      view.controls.update()
    }
  }
  setCamera()
  cameraMode.onchange = setCamera
  let elapsed = 0
  let playing = true
  let last = performance.now()
  let animationFrame = 0
  const setPlaying = (value: boolean) => {
    playing = value
    pause.textContent = playing ? '일시정지' : '계속 재생'
  }
  replay.disabled = pause.disabled = false
  replay.onclick = () => {
    elapsed = 0
    setPlaying(true)
  }
  pause.onclick = () => {
    if (!playing && elapsed >= DURATION) elapsed = 0
    setPlaying(!playing)
  }
  scrub.oninput = () => {
    elapsed = Number(scrub.value)
    setPlaying(false)
  }
  status.textContent = '도적 + 단검 · 동작 0.82초 · 타격 간격 0.24초'
  const tick = (now: number) => {
    const dt = Math.min((now - last) / 1000, 0.05)
    last = now
    if (playing) elapsed += dt * Number(speed.value)
    if (elapsed >= DURATION && !loop.checked) {
      elapsed = DURATION
      setPlaying(false)
    }
    if (elapsed > DURATION + 0.95 && loop.checked) elapsed = 0
    const time = Math.min(elapsed, DURATION)
    scrub.value = String(time)
    el('time').textContent = `${time.toFixed(2)} / 0.82초`
    HITS.forEach((hit, i) =>
      el(`hit-${i + 1}`).classList.toggle(
        'active',
        elapsed >= hit && elapsed < hit + 0.13
      )
    )
    for (const view of views) {
      view.pose(time)
      view.effect.update(elapsed)
      view.effect.group.visible = vfx.checked
      view.renderer.render(view.scene, view.camera)
    }
    animationFrame = requestAnimationFrame(tick)
  }
  animationFrame = requestAnimationFrame(tick)
  window.addEventListener(
    'pagehide',
    () => {
      cancelAnimationFrame(animationFrame)
      views.forEach((view) => {
        view.observer.disconnect()
        view.controls.dispose()
        view.mixer.stopAllAction()
        view.scene.traverse((object) => {
          if (object instanceof THREE.Mesh) {
            object.geometry.dispose()
            const materials = Array.isArray(object.material)
              ? object.material
              : [object.material]
            materials.forEach((material) => material.dispose())
          }
        })
        view.renderer.dispose()
      })
    },
    { once: true }
  )
}

main().catch((error) => {
  status.textContent = `프리뷰를 불러오지 못했습니다: ${error instanceof Error ? error.message : String(error)}`
  status.dataset.error = 'true'
  console.error(error)
})
