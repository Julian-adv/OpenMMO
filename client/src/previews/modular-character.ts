import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import {
  computeSoleGroundOffset,
  createCharacterModelRoot,
} from '../lib/utils/characterAnimationUtils'
import {
  bindModularPart,
  modularAnimationClips,
  modularRigId,
  modularSwordTracks,
  parseModularHandProfile,
  showModularOutfit,
  skinnedParts,
  type ModularOutfit,
} from '../lib/utils/modularCharacter'
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
const showWeapon = el<HTMLInputElement>('weapon')
const hairSelect = el<HTMLSelectElement>('hair')
const hairColor = el<HTMLInputElement>('hair-color')
const eyeColor = el<HTMLInputElement>('eye-color')
const topSelect = el<HTMLSelectElement>('top')
const gloves = el<HTMLInputElement>('gloves')
const boots = el<HTMLInputElement>('boots')
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
      parts: Map<string, THREE.SkinnedMesh[]>
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
  const ids = [
    'hair_crop',
    'hair_sidepart',
    'top_linen',
    'top_leather',
    'pants_cloth',
    'gloves_leather',
    'boots_leather',
  ]
  const [base, sources, animations, sword, profile] = await Promise.all([
    load('/__modular-character/parts/base.glb'),
    Promise.all(ids.map((id) => load(`/__modular-character/parts/${id}.glb`))),
    load('/__modular-character/animations.glb'),
    load('/models/weapons/sword.glb'),
    fetch('/__modular-character/hand-grips.json').then(async (response) => {
      if (!response.ok) throw new Error('손 보정 파일을 불러오지 못했습니다.')
      return parseModularHandProfile(await response.json())
    }),
  ])
  const clips = modularAnimationClips(base.scene, animations, 'corrected')
  for (const option of clipSelect.options)
    if (!clips.some((clip) => clip.name === option.value))
      throw new Error(`동작이 없습니다: ${option.value}`)
  const { modelRoot, clonedScene: body } = createCharacterModelRoot(base.scene)
  scene.add(modelRoot)
  const rigId = modularRigId(body)
  const bodyMeshes = skinnedParts(body)
  modelRoot.position.y += computeSoleGroundOffset(modelRoot)
  const parts = new Map(
    ids.map((id, i) => [id, bindModularPart(body, sources[i].scene)])
  )
  const meshes = skinnedParts(body)
  for (const mesh of meshes) {
    mesh.frustumCulled = false
  }
  for (const id of ['hair_crop', 'hair_sidepart'])
    for (const mesh of parts.get(id)!) {
      mesh.material = Array.isArray(mesh.material)
        ? mesh.material.map((m) => m.clone())
        : mesh.material.clone()
    }
  const irisColor = { value: new THREE.Color(eyeColor.value) }
  for (const mesh of bodyMeshes.filter(
    (mesh) => mesh.userData.region === 'head'
  )) {
    const tint = (source: THREE.Material) => {
      if (!(source instanceof THREE.MeshStandardMaterial)) return source
      const material = source.clone()
      material.onBeforeCompile = (shader) => {
        shader.uniforms.irisColor = irisColor
        shader.vertexShader =
          'varying vec3 irisRestPosition;\n' +
          shader.vertexShader.replace(
            '#include <begin_vertex>',
            '#include <begin_vertex>\nirisRestPosition = position;'
          )
        shader.fragmentShader =
          'varying vec3 irisRestPosition;\nuniform vec3 irisColor;\n' +
          shader.fragmentShader.replace(
            '#include <map_fragment>',
            `#include <map_fragment>
            vec2 irisPoint = vec2(abs(irisRestPosition.x) - 0.034, irisRestPosition.y - 1.7825);
            float irisRadius = length(irisPoint / vec2(0.005, 0.0045));
            float irisMask = (1.0 - smoothstep(0.85, 1.0, irisRadius)) * smoothstep(0.26, 0.48, irisRadius);
            irisMask *= step(0.07, irisRestPosition.z);
            float irisDetail = dot(diffuseColor.rgb, vec3(0.2126, 0.7152, 0.0722));
            diffuseColor.rgb = mix(diffuseColor.rgb, irisColor * clamp(irisDetail * 6.0, 0.15, 1.2), irisMask * 0.9);`
          )
      }
      material.customProgramCacheKey = () => 'modular-male-iris-v1'
      return material
    }
    mesh.material = Array.isArray(mesh.material)
      ? mesh.material.map(tint)
      : tint(mesh.material)
  }
  eyeColor.oninput = () => irisColor.value.set(eyeColor.value)
  let equipped = new Set<string>()
  const dress = () => {
    equipped = showModularOutfit(bodyMeshes, parts, {
      hair: hairSelect.value as ModularOutfit['hair'],
      top: topSelect.value as ModularOutfit['top'],
      gloves: gloves.checked,
      boots: boots.checked,
    })
    for (const id of ['hair_crop', 'hair_sidepart'])
      for (const mesh of parts.get(id)!)
        for (const mat of Array.isArray(mesh.material)
          ? mesh.material
          : [mesh.material])
          if (mat instanceof THREE.MeshStandardMaterial)
            mat.color.set(hairColor.value)
    updateStats()
  }
  for (const element of [hairSelect, topSelect, gloves, boots])
    element.onchange = dress
  hairColor.oninput = dress
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
  const actions = new Map(
    clips.map((clip) => {
      const posed = clip.clone()
      posed.tracks.push(
        ...modularSwordTracks(profile, rigId, clip, weapon.name)
      )
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
  const setPlaying = (value: boolean) => {
    playing = value
    pause.textContent = playing ? '일시정지' : '계속 재생'
  }
  const stopSequence = () => {
    sequencing = false
    sequence.textContent = '동작 전환 연속 확인'
  }
  const play = (name: string, fade = true) => {
    const next = actions.get(name)
    if (!next) throw new Error(`재생할 동작이 없습니다: ${name}`)
    for (const action of actions.values()) {
      action.stopFading()
      if (action !== active) action.stop()
    }
    const previous = active
    next.reset().setEffectiveTimeScale(1).setEffectiveWeight(1).play()
    if (previous && previous !== next) {
      if (fade && playing) {
        previous.crossFadeTo(next, 0.25, false)
        transitionLeft = 0.25
      } else previous.stop()
    }
    active = next
    clipSelect.value = name
    mixer.update(0)
    status.textContent = clipSelect.selectedOptions[0].textContent
  }
  play('combat_idle', false)
  preview = { modelRoot, mixer, currentAction: () => active, weapon, parts }
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
    for (const action of actions.values()) {
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
  showWeapon.onchange = () => {
    weapon.visible = showWeapon.checked
    updateStats()
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
    const replacement = bindModularPart(
      body,
      sources[ids.indexOf('pants_cloth')].scene
    )
    for (const mesh of parts.get('pants_cloth')!) mesh.removeFromParent()
    parts.set('pants_cloth', replacement)
    for (const mesh of replacement) mesh.frustumCulled = false
    dress()
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
  function updateStats() {
    const selected = [
      ...bodyMeshes,
      ...[...parts]
        .filter(([id]) => equipped.has(id))
        .flatMap(([, meshes]) => meshes),
    ]
    const total =
      selected.reduce((sum, mesh) => sum + triangles(mesh, false), 0) +
      (showWeapon.checked ? 302 : 0)
    const visible =
      selected.reduce((sum, mesh) => sum + triangles(mesh, true), 0) +
      (showWeapon.checked ? 302 : 0)
    el('stats').textContent =
      `조합 ${total.toLocaleString()}삼각형 · 표시 ${visible.toLocaleString()} · 얼굴 1,505`
  }
  dress()
  let last = performance.now()
  const render = (now: number) => {
    const dt = Math.min((now - last) / 1000, 0.05) * Number(speed.value)
    last = now
    if (playing) {
      mixer.update(dt)
      if (transitionLeft > 0) {
        transitionLeft -= dt
        if (transitionLeft <= 0) {
          for (const action of actions.values())
            if (action !== active) action.stop()
        }
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
