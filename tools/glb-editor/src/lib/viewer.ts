import * as THREE from 'three'
import { GLTFExporter } from 'three/examples/jsm/exporters/GLTFExporter.js'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js'
import { downloadArrayBuffer, loadGLTFFromFile } from './gltf-io'

export interface CandidateSummary {
  index: number
  name: string
  stats: string
}

interface ViewerCallbacks {
  log: (message: string) => void
  onMetaChange: (message: string) => void
  onCandidatesChange: (items: CandidateSummary[], selectedIndex: number) => void
  onClipsChange: (
    clips: string[],
    selectedClipIndex: number,
    info: string
  ) => void
}

const exporter = new GLTFExporter()

export class GlbViewer {
  private readonly container: HTMLElement
  private readonly callbacks: ViewerCallbacks

  private renderer: THREE.WebGLRenderer
  private scene: THREE.Scene
  private camera: THREE.PerspectiveCamera
  private controls: OrbitControls
  private clock = new THREE.Clock()
  private resizeObserver: ResizeObserver
  private rafId = 0

  private srcGLTF: GLTF | null = null
  private sourceFileName = ''
  private candidates: THREE.Object3D[] = []
  private selectedIndex = -1

  private modelRoot: THREE.Group | null = null
  private mixer: THREE.AnimationMixer | null = null
  private currentActions: THREE.AnimationAction[] = []
  private relatedClips: THREE.AnimationClip[] = []

  private autoRotate = false
  private loop = true
  private selectedClipIndex = 0

  constructor(container: HTMLElement, callbacks: ViewerCallbacks) {
    this.container = container
    this.callbacks = callbacks

    this.renderer = new THREE.WebGLRenderer({ antialias: true })
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))

    this.scene = new THREE.Scene()
    this.scene.background = new THREE.Color(0x0b0d12)

    this.camera = new THREE.PerspectiveCamera(60, 1, 0.1, 2000)
    this.camera.position.set(0, 1.5, 3)

    this.controls = new OrbitControls(this.camera, this.renderer.domElement)
    this.controls.enableDamping = true
    this.controls.target.set(0, 1, 0)

    this.container.appendChild(this.renderer.domElement)
    this.initSceneLights()

    this.resizeObserver = new ResizeObserver(() => this.onResize())
    this.resizeObserver.observe(this.container)
    this.onResize()

    this.animate = this.animate.bind(this)
    this.animate()
  }

  getSourceGLTF(): GLTF | null {
    return this.srcGLTF
  }

  setAutoRotate(enabled: boolean): void {
    this.autoRotate = enabled
  }

  setLoop(enabled: boolean): void {
    this.loop = enabled
    if (this.relatedClips.length > 0) {
      this.playClip(this.selectedClipIndex)
    }
  }

  async loadFile(file: File): Promise<void> {
    this.reset()
    this.sourceFileName = file.name
    this.callbacks.log(`파일 로드 시작: ${file.name} (${file.size} bytes)`)

    const gltf = await loadGLTFFromFile(file)
    this.srcGLTF = gltf

    this.callbacks.log(`로드 완료. animations: ${gltf.animations?.length ?? 0}`)

    const exportRoot = this.findExportRoot(gltf.scene)
    this.candidates = this.findCandidates(exportRoot)
    this.selectedIndex = -1

    this.callbacks.onMetaChange(`오브젝트 ${this.candidates.length}개`)
    this.pushCandidates()

    if (this.candidates.length > 0) {
      this.selectCandidate(0)
    }
  }

  selectCandidate(index: number): void {
    if (index < 0 || index >= this.candidates.length) return

    this.selectedIndex = index
    this.pushCandidates()
    this.loadPreview(this.candidates[index])
  }

  playClip(index: number): void {
    if (!this.mixer || this.relatedClips.length === 0) return

    this.currentActions.forEach((action) => {
      action.stop()
      if (this.modelRoot) {
        this.mixer?.uncacheAction(action.getClip(), this.modelRoot)
      }
    })
    this.currentActions = []

    const safeIndex = Math.max(0, Math.min(index, this.relatedClips.length - 1))
    this.selectedClipIndex = safeIndex

    const clip = this.relatedClips[safeIndex]
    const action = this.mixer.clipAction(clip)
    action.reset()
    action.paused = false
    action.loop = this.loop ? THREE.LoopRepeat : THREE.LoopOnce
    action.clampWhenFinished = true
    action.play()

    this.currentActions = [action]
    this.pushClips()
  }

  pause(): void {
    this.currentActions.forEach((action) => {
      action.paused = true
    })
  }

  refreshPreview(): void {
    if (
      this.selectedIndex >= 0 &&
      this.selectedIndex < this.candidates.length
    ) {
      this.loadPreview(this.candidates[this.selectedIndex])
    }
  }

  async saveCurrentGLB(): Promise<void> {
    if (!this.srcGLTF) return

    const allAnims = this.srcGLTF.animations ?? []
    const outputFileName = this.getMergedFileName()
    this.callbacks.log(`GLB 저장 시작 (animations: ${allAnims.length})`)

    try {
      const arrayBuffer = await this.exportScene(
        this.srcGLTF.scene as unknown as THREE.Scene,
        allAnims
      )
      downloadArrayBuffer(outputFileName, arrayBuffer)
      this.callbacks.log(`GLB 저장 완료: ${outputFileName} 다운로드`)
    } catch (error) {
      this.callbacks.log(`GLB 저장 실패: ${String(error)}`)
    }
  }

  async exportSelected(): Promise<void> {
    if (this.selectedIndex < 0 || this.selectedIndex >= this.candidates.length)
      return

    await this.doExportOne(
      this.candidates[this.selectedIndex],
      this.relatedClips,
      this.selectedIndex
    )
  }

  async exportAll(): Promise<void> {
    if (this.candidates.length === 0 || !this.srcGLTF) return

    this.callbacks.log('=== 전체 일괄 내보내기 시작 ===')

    for (let i = 0; i < this.candidates.length; i += 1) {
      const node = this.candidates[i]
      const cloneForNames = SkeletonUtils.clone(node) as THREE.Object3D
      const clips = this.filterAnimations(
        this.srcGLTF.animations ?? [],
        this.collectNodeNames(cloneForNames)
      )
      await this.doExportOne(node, clips, i, true)
    }

    this.callbacks.log('=== 전체 일괄 내보내기 완료 ===')
  }

  reset(): void {
    this.candidates = []
    this.selectedIndex = -1
    this.srcGLTF = null
    this.sourceFileName = ''
    this.callbacks.onMetaChange('')
    this.disposePreview()
    this.pushCandidates()
    this.pushClips()
    this.callbacks.log('상태 초기화 완료')
  }

  destroy(): void {
    cancelAnimationFrame(this.rafId)
    this.resizeObserver.disconnect()

    this.disposePreview()

    this.controls.dispose()
    this.scene.clear()
    this.renderer.dispose()
    if (this.renderer.domElement.parentElement === this.container) {
      this.container.removeChild(this.renderer.domElement)
    }
  }

  private initSceneLights(): void {
    const hemi = new THREE.HemisphereLight(0xffffff, 0x222233, 0.6)
    this.scene.add(hemi)

    const dir = new THREE.DirectionalLight(0xffffff, 0.8)
    dir.position.set(5, 10, 7.5)
    this.scene.add(dir)

    const grid = new THREE.GridHelper(10, 10, 0x20252f, 0x20252f)
    grid.position.y = 0
    this.scene.add(grid)
  }

  private onResize(): void {
    const w = this.container.clientWidth
    const h = this.container.clientHeight
    if (w <= 0 || h <= 0) return

    this.camera.aspect = w / h
    this.camera.updateProjectionMatrix()
    this.renderer.setSize(w, h)
  }

  private animate(): void {
    const dt = this.clock.getDelta()
    this.mixer?.update(dt)

    this.controls.autoRotate = this.autoRotate
    this.controls.update()
    this.renderer.render(this.scene, this.camera)

    this.rafId = requestAnimationFrame(this.animate)
  }

  private findExportRoot(scene: THREE.Object3D): THREE.Object3D {
    let root: THREE.Object3D | null = null
    scene.traverse((node) => {
      if (node.name === 'GLTF_SceneRootNode') root = node
    })
    return root ?? scene
  }

  private findCandidates(root: THREE.Object3D): THREE.Object3D[] {
    const out: THREE.Object3D[] = []
    const children = root.children.length > 0 ? root.children : [root]

    children.forEach((child) => {
      let meshCount = 0
      child.traverse((node) => {
        if ((node as THREE.Mesh).isMesh) meshCount += 1
      })
      if (meshCount > 0) out.push(child)
    })

    if (out.length === 0) {
      root.children.forEach((child) => out.push(child))
    }

    this.callbacks.log(`후보 수집: ${out.length}`)
    return out
  }

  private loadPreview(sourceNode: THREE.Object3D): void {
    if (!this.srcGLTF) return

    this.disposePreview()

    const cloned = SkeletonUtils.clone(sourceNode) as THREE.Object3D
    this.modelRoot = new THREE.Group()
    this.modelRoot.add(cloned)
    this.scene.add(this.modelRoot)

    this.frameObject(this.modelRoot)

    const allowedNames = this.collectNodeNames(cloned)
    this.relatedClips = this.filterAnimations(
      this.srcGLTF.animations ?? [],
      allowedNames
    )

    if (this.relatedClips.length > 0) {
      this.mixer = new THREE.AnimationMixer(this.modelRoot)
      this.selectedClipIndex = 0
      this.playClip(0)
    } else {
      this.mixer = null
      this.selectedClipIndex = 0
      this.pushClips()
    }
  }

  private frameObject(root: THREE.Object3D): void {
    const box = new THREE.Box3().setFromObject(root)
    if (box.isEmpty()) return

    const sphere = box.getBoundingSphere(new THREE.Sphere())
    this.controls.target.copy(sphere.center)

    const dist = sphere.radius * 2.5 + 0.5
    this.camera.position
      .copy(sphere.center)
      .add(new THREE.Vector3(dist, dist * 0.8, dist))

    this.camera.near = Math.max(0.01, sphere.radius / 100)
    this.camera.far = Math.max(10, dist * 20)
    this.camera.updateProjectionMatrix()
  }

  private disposePreview(): void {
    if (this.mixer && this.modelRoot) {
      this.mixer.stopAllAction()
      this.mixer.uncacheRoot(this.modelRoot)
    }

    this.mixer = null
    this.currentActions = []
    this.relatedClips = []
    this.selectedClipIndex = 0

    if (this.modelRoot) {
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

    this.pushClips()
  }

  private collectNodeNames(root: THREE.Object3D): Set<string> {
    const set = new Set<string>()
    root.traverse((obj) => {
      if (obj.name) set.add(obj.name)
    })
    return set
  }

  private filterAnimations(
    anims: THREE.AnimationClip[],
    allowed: Set<string>
  ): THREE.AnimationClip[] {
    const out: THREE.AnimationClip[] = []

    for (const clip of anims) {
      const kept = clip.tracks.filter((track) => {
        const target = track.name.split('.')[0]
        return allowed.has(target)
      })

      if (kept.length === 0) continue

      const newClip = clip.clone()
      newClip.tracks = kept
      out.push(newClip)
    }

    return out
  }

  private async doExportOne(
    sourceNode: THREE.Object3D,
    clips: THREE.AnimationClip[],
    index: number,
    silent = false
  ): Promise<void> {
    const cloned = SkeletonUtils.clone(sourceNode) as THREE.Object3D

    cloned.position.set(0, 0, 0)
    cloned.rotation.set(Math.PI / 2, 0, 0)
    cloned.scale.set(1, 1, 1)

    const exportScene = new THREE.Scene()
    exportScene.add(cloned)

    const safeName = (sourceNode.name || `object_${index + 1}`).replace(
      /[^a-zA-Z0-9_-]/g,
      '_'
    )
    const fileName = `animated_${String(index + 1).padStart(2, '0')}_${safeName}.glb`

    try {
      const arrayBuffer = await this.exportScene(exportScene, clips)
      downloadArrayBuffer(fileName, arrayBuffer)
      if (!silent) this.callbacks.log(`내보내기 완료: ${fileName}`)
    } catch (error) {
      this.callbacks.log(`내보내기 실패: ${fileName} (${String(error)})`)
    }
  }

  private exportScene(
    scene: THREE.Scene,
    animations: THREE.AnimationClip[]
  ): Promise<ArrayBuffer> {
    return new Promise((resolve, reject) => {
      exporter.parse(
        scene,
        (result) => {
          if (result instanceof ArrayBuffer) {
            resolve(result)
            return
          }
          reject(
            new Error('GLB binary export failed: non-binary result returned')
          )
        },
        (err) => {
          reject(err instanceof Error ? err : new Error(String(err)))
        },
        {
          binary: true,
          animations,
        }
      )
    })
  }

  private getMergedFileName(): string {
    if (!this.sourceFileName) return 'merged.glb'

    const trimmed = this.sourceFileName.trim()
    if (!trimmed) return 'merged.glb'

    const dotIndex = trimmed.lastIndexOf('.')
    if (dotIndex <= 0) return `${trimmed}.glb`

    return `${trimmed.slice(0, dotIndex)}.glb`
  }

  private getMeshStats(node: THREE.Object3D): string {
    let meshes = 0
    let skinned = 0
    let tris = 0

    node.traverse((obj) => {
      const mesh = obj as THREE.Mesh
      if (!mesh.isMesh) return

      meshes += 1
      if ((mesh as THREE.SkinnedMesh).isSkinnedMesh) skinned += 1

      const geom = mesh.geometry
      if (!geom) return

      tris += geom.index
        ? geom.index.count / 3
        : (geom.attributes.position?.count ?? 0) / 3
    })

    return `meshes:${meshes} skinned:${skinned} ~tris:${Math.round(tris)}`
  }

  private pushCandidates(): void {
    const items = this.candidates.map((node, index) => ({
      index,
      name: node.name || `(unnamed_${index + 1})`,
      stats: this.getMeshStats(node),
    }))

    this.callbacks.onCandidatesChange(items, this.selectedIndex)
  }

  private pushClips(): void {
    if (this.relatedClips.length === 0) {
      this.callbacks.onClipsChange([], 0, '애니메이션 없음')
      return
    }

    const clips = this.relatedClips.map(
      (clip, index) => clip.name || `Clip ${index + 1}`
    )
    this.callbacks.onClipsChange(
      clips,
      this.selectedClipIndex,
      `${clips.length} clip(s)`
    )
  }
}
