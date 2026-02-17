import * as THREE from 'three'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js'

export class ClipPreviewer {
  private readonly container: HTMLElement
  private readonly renderer: THREE.WebGLRenderer
  private readonly scene: THREE.Scene
  private readonly camera: THREE.PerspectiveCamera
  private readonly controls: OrbitControls
  private readonly clock = new THREE.Clock()
  private readonly resizeObserver: ResizeObserver

  private rafId = 0
  private modelRoot: THREE.Object3D | null = null
  private mixer: THREE.AnimationMixer | null = null
  private currentAction: THREE.AnimationAction | null = null

  private clips: THREE.AnimationClip[] = []
  private selectedClipIndex = 0
  private loop = true

  constructor(container: HTMLElement) {
    this.container = container

    this.renderer = new THREE.WebGLRenderer({ antialias: true })
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))

    this.scene = new THREE.Scene()
    this.scene.background = new THREE.Color(0x090b12)

    this.camera = new THREE.PerspectiveCamera(60, 1, 0.1, 2000)
    this.camera.position.set(0, 1.2, 2.4)

    this.controls = new OrbitControls(this.camera, this.renderer.domElement)
    this.controls.enableDamping = true
    this.controls.autoRotate = false

    this.container.appendChild(this.renderer.domElement)
    this.initLights()

    this.resizeObserver = new ResizeObserver(() => this.onResize())
    this.resizeObserver.observe(this.container)
    this.onResize()

    this.animate = this.animate.bind(this)
    this.animate()
  }

  setLoop(enabled: boolean): void {
    this.loop = enabled
    if (this.clips.length > 0) {
      this.playClip(this.selectedClipIndex)
    }
  }

  clear(): void {
    this.disposeModel()
    this.clips = []
    this.selectedClipIndex = 0
  }

  loadGLTF(gltf: GLTF): void {
    this.disposeModel()

    const cloned = SkeletonUtils.clone(gltf.scene) as THREE.Object3D
    this.modelRoot = cloned
    this.scene.add(cloned)

    this.frameObject(cloned)
    this.clips = gltf.animations ?? []
    this.selectedClipIndex = 0

    if (this.clips.length > 0) {
      this.mixer = new THREE.AnimationMixer(cloned)
      this.playClip(0)
    }
  }

  playClip(index: number): void {
    if (!this.mixer || !this.modelRoot || this.clips.length === 0) return

    const safeIndex = Math.max(0, Math.min(index, this.clips.length - 1))
    this.selectedClipIndex = safeIndex

    if (this.currentAction) {
      this.currentAction.stop()
      this.mixer.uncacheAction(this.currentAction.getClip(), this.modelRoot)
      this.currentAction = null
    }

    const clip = this.clips[safeIndex]
    const action = this.mixer.clipAction(clip)
    action.reset()
    action.paused = false
    action.loop = this.loop ? THREE.LoopRepeat : THREE.LoopOnce
    action.clampWhenFinished = true
    action.play()

    this.currentAction = action
  }

  pause(): void {
    if (this.currentAction) {
      this.currentAction.paused = true
    }
  }

  destroy(): void {
    cancelAnimationFrame(this.rafId)
    this.resizeObserver.disconnect()

    this.disposeModel()

    this.controls.dispose()
    this.scene.clear()
    this.renderer.dispose()

    if (this.renderer.domElement.parentElement === this.container) {
      this.container.removeChild(this.renderer.domElement)
    }
  }

  private onResize(): void {
    const width = this.container.clientWidth
    const height = this.container.clientHeight
    if (width <= 0 || height <= 0) return

    this.camera.aspect = width / height
    this.camera.updateProjectionMatrix()
    this.renderer.setSize(width, height)
  }

  private animate(): void {
    const dt = this.clock.getDelta()
    this.mixer?.update(dt)

    this.controls.update()
    this.renderer.render(this.scene, this.camera)

    this.rafId = requestAnimationFrame(this.animate)
  }

  private initLights(): void {
    const hemi = new THREE.HemisphereLight(0xffffff, 0x222233, 0.65)
    this.scene.add(hemi)

    const dir = new THREE.DirectionalLight(0xffffff, 0.75)
    dir.position.set(4, 8, 6)
    this.scene.add(dir)

    const grid = new THREE.GridHelper(8, 8, 0x232a3a, 0x1b2231)
    this.scene.add(grid)
  }

  private frameObject(root: THREE.Object3D): void {
    const box = new THREE.Box3().setFromObject(root)
    if (box.isEmpty()) return

    const sphere = box.getBoundingSphere(new THREE.Sphere())
    this.controls.target.copy(sphere.center)

    const dist = sphere.radius * 2.4 + 0.4
    this.camera.position
      .copy(sphere.center)
      .add(new THREE.Vector3(dist, dist * 0.8, dist))

    this.camera.near = Math.max(0.01, sphere.radius / 100)
    this.camera.far = Math.max(10, dist * 20)
    this.camera.updateProjectionMatrix()
  }

  private disposeModel(): void {
    if (this.mixer && this.modelRoot) {
      this.mixer.stopAllAction()
      this.mixer.uncacheRoot(this.modelRoot)
    }

    this.mixer = null
    this.currentAction = null

    if (!this.modelRoot) return

    this.scene.remove(this.modelRoot)
    this.modelRoot.traverse((obj) => {
      const mesh = obj as THREE.Mesh
      if (!mesh.isMesh) return

      mesh.geometry?.dispose()
      if (Array.isArray(mesh.material)) {
        mesh.material.forEach((material) => material.dispose())
      } else {
        mesh.material?.dispose()
      }
    })

    this.modelRoot = null
  }
}
