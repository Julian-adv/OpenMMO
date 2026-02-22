import * as THREE from 'three'
import { GLTFExporter } from 'three/examples/jsm/exporters/GLTFExporter.js'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js'
import { downloadArrayBuffer, loadGLTFFromFile } from './gltf-io'
import {
  detectMixamoSkeleton,
  renameBonesToMixamo,
  type MixamoDetectionResult,
} from './mixamo-bones'

export interface CandidateSummary {
  index: number
  name: string
  stats: string
}

export interface ExtractAnimationPackOptions {
  packName: string
  clipName: string
  basePackFile?: File | null
  outputMode?: 'download' | 'return-buffer'
}

export interface ExtractAnimationPackResult {
  fileName: string
  clipName: string
  totalTracks: number
  keptTracks: number
  mode: 'new-pack' | 'append-pack'
  arrayBuffer: ArrayBuffer
}

export interface SaveCurrentGLBResult {
  fileName: string
  arrayBuffer: ArrayBuffer
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
  private relatedClipSourceIndices: number[] = []

  private autoRotate = false
  private loop = true
  private selectedClipIndex = 0
  private readonly onMixerFinished = (event: THREE.Event): void => {
    if (this.loop) return
    if (!this.mixer) return

    const finished = event as THREE.Event & {
      action?: THREE.AnimationAction
    }
    const action = finished.action
    if (!action) return

    const clip = action.getClip()
    action.enabled = true
    action.paused = true
    action.time = clip.duration
    action.clampWhenFinished = true
    action.setEffectiveWeight(1)
    this.mixer.update(0)
  }

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

  deleteCurrentClip(): boolean {
    if (!this.srcGLTF) return false
    const sourceIndex = this.relatedClipSourceIndices[this.selectedClipIndex]
    if (sourceIndex === undefined) return false

    this.srcGLTF.animations.splice(sourceIndex, 1)
    this.refreshPreview()
    return true
  }

  detectBones(): MixamoDetectionResult | null {
    if (!this.srcGLTF) {
      this.callbacks.log('본 감지 실패: GLB가 로드되지 않았습니다')
      return null
    }

    const scene = this.srcGLTF.scene
    const bones: THREE.Bone[] = []
    scene.traverse((node) => {
      if ((node as THREE.Bone).isBone) {
        bones.push(node as THREE.Bone)
      }
    })

    if (bones.length === 0) {
      this.callbacks.log('본 감지 실패: 스켈레톤이 없습니다')
      return null
    }

    const detection = detectMixamoSkeleton(bones)
    this.callbacks.log(
      `Mixamo 감지: core=${(detection.coreMatchRatio * 100).toFixed(0)}% ` +
        `전체=${(detection.matchRatio * 100).toFixed(0)}% ` +
        `(${Object.keys(detection.nameMap).length}/${bones.length} 본 매칭)`
    )

    if (!detection.isMixamo) {
      this.callbacks.log(
        '본 감지 실패: Mixamo 스켈레톤 구조가 아닙니다 (core 매칭률 70% 미만)'
      )
      this.callbacks.log(
        '자동 감지는 실패했지만 수동 매핑 다이얼로그를 열어 계속 진행합니다.'
      )
    }

    return detection
  }

  applyBoneRename(nameMap: Record<string, string>): boolean {
    if (!this.srcGLTF) return false

    const needsRename = Object.entries(nameMap).filter(
      ([from, to]) => from !== to
    )

    if (needsRename.length === 0) {
      this.callbacks.log('본 이름이 이미 표준 Mixamo 이름입니다')
      return false
    }

    const animations = this.srcGLTF.animations ?? []
    const renamed = renameBonesToMixamo(this.srcGLTF.scene, animations, nameMap)

    this.callbacks.log(`본 이름 표준화 완료: ${renamed}개 본 이름 변경`)

    for (const [from, to] of needsRename) {
      this.callbacks.log(`  ${from} → ${to}`)
    }

    this.refreshPreview()
    return true
  }

  async saveCurrentGLB(): Promise<SaveCurrentGLBResult | null> {
    if (!this.srcGLTF) return null

    const allAnims = this.srcGLTF.animations ?? []
    const outputFileName = this.getMergedFileName()
    this.callbacks.log(`GLB 저장 시작 (animations: ${allAnims.length})`)

    try {
      const arrayBuffer = await this.exportScene(
        this.srcGLTF.scene as unknown as THREE.Scene,
        allAnims
      )
      return {
        fileName: outputFileName,
        arrayBuffer,
      }
    } catch (error) {
      this.callbacks.log(`GLB 저장 실패: ${String(error)}`)
      return null
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
      ).map((f) => f.clip)
      await this.doExportOne(node, clips, i, true)
    }

    this.callbacks.log('=== 전체 일괄 내보내기 완료 ===')
  }

  async extractSelectedClipToPack(
    options: ExtractAnimationPackOptions
  ): Promise<ExtractAnimationPackResult> {
    if (
      this.selectedIndex < 0 ||
      this.selectedIndex >= this.candidates.length
    ) {
      throw new Error('먼저 추출할 오브젝트를 선택하세요.')
    }

    if (this.relatedClips.length === 0) {
      throw new Error('추출할 애니메이션이 없습니다.')
    }

    const sourceClip = this.relatedClips[this.selectedClipIndex]
    if (!sourceClip) {
      throw new Error('선택된 애니메이션이 없습니다.')
    }

    const packName = this.sanitizePackName(options.packName)
    const requestedClipName =
      options.clipName.trim() || sourceClip.name || 'Animation'
    const outputMode = options.outputMode ?? 'download'

    if (options.basePackFile) {
      const basePack = await loadGLTFFromFile(options.basePackFile)
      const clonedRoot = SkeletonUtils.clone(basePack.scene) as THREE.Object3D
      const exportScene = new THREE.Scene()
      exportScene.add(clonedRoot)

      const filtered = this.filterClipTracksByNodeNames(
        sourceClip,
        this.collectNodeNames(clonedRoot)
      )
      if (filtered.keptTracks === 0) {
        throw new Error(
          '선택한 팩 GLB와 본 이름이 맞지 않아 트랙이 모두 제외되었습니다.'
        )
      }

      const totalTracks = filtered.totalTracks
      const keptTracks = filtered.keptTracks

      const takenNames = new Set(
        (basePack.animations ?? [])
          .map((clip) => clip.name || '')
          .filter(Boolean)
      )
      const clipName = this.ensureUniqueClipName(requestedClipName, takenNames)
      filtered.clip.name = clipName

      const outAnimations = [...(basePack.animations ?? []), filtered.clip]
      const arrayBuffer = await this.exportScene(exportScene, outAnimations)
      const fileName = `${packName}.glb`
      if (outputMode === 'download') {
        downloadArrayBuffer(fileName, arrayBuffer)
      }
      this.callbacks.log(
        outputMode === 'download'
          ? `애니메이션 팩 저장 완료: ${fileName} (기존 팩 +1, ${keptTracks}/${totalTracks} 트랙 유지)`
          : `애니메이션 팩 생성 완료: ${fileName} (기존 팩 +1, ${keptTracks}/${totalTracks} 트랙 유지)`
      )

      return {
        fileName,
        clipName,
        totalTracks,
        keptTracks,
        mode: 'append-pack',
        arrayBuffer,
      }
    }

    const sourceNode = this.candidates[this.selectedIndex]
    const exportRoot = this.buildAnimationPackRoot(sourceNode, sourceClip)
    const exportScene = new THREE.Scene()
    exportScene.add(exportRoot)

    const filtered = this.filterClipTracksByNodeNames(
      sourceClip,
      this.collectNodeNames(exportRoot)
    )
    if (filtered.keptTracks === 0) {
      throw new Error('현재 오브젝트에서 추출 가능한 트랙이 없습니다.')
    }

    const totalTracks = filtered.totalTracks
    const keptTracks = filtered.keptTracks
    filtered.clip.name = requestedClipName

    const arrayBuffer = await this.exportScene(exportScene, [filtered.clip])
    const fileName = `${packName}.glb`
    if (outputMode === 'download') {
      downloadArrayBuffer(fileName, arrayBuffer)
    }
    this.callbacks.log(
      outputMode === 'download'
        ? `애니메이션 팩 저장 완료: ${fileName} (신규 팩, ${keptTracks}/${totalTracks} 트랙 유지)`
        : `애니메이션 팩 생성 완료: ${fileName} (신규 팩, ${keptTracks}/${totalTracks} 트랙 유지)`
    )

    return {
      fileName,
      clipName: filtered.clip.name,
      totalTracks,
      keptTracks,
      mode: 'new-pack',
      arrayBuffer,
    }
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
    const filtered = this.filterAnimations(
      this.srcGLTF.animations ?? [],
      allowedNames
    )
    this.relatedClips = filtered.map((f) => f.clip)
    this.relatedClipSourceIndices = filtered.map((f) => f.sourceIndex)

    if (this.relatedClips.length > 0) {
      this.mixer = new THREE.AnimationMixer(this.modelRoot)
      this.mixer.addEventListener('finished', this.onMixerFinished)
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
      this.mixer.removeEventListener('finished', this.onMixerFinished)
      this.mixer.stopAllAction()
      this.mixer.uncacheRoot(this.modelRoot)
    }

    this.mixer = null
    this.currentActions = []
    this.relatedClips = []
    this.relatedClipSourceIndices = []
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
  ): { clip: THREE.AnimationClip; sourceIndex: number }[] {
    const out: { clip: THREE.AnimationClip; sourceIndex: number }[] = []

    for (let i = 0; i < anims.length; i++) {
      const clip = anims[i]
      const kept = clip.tracks.filter((track) => {
        const target = track.name.split('.')[0]
        return allowed.has(target)
      })

      if (kept.length === 0) continue

      const newClip = clip.clone()
      newClip.tracks = kept
      out.push({ clip: newClip, sourceIndex: i })
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

  private buildAnimationPackRoot(
    sourceNode: THREE.Object3D,
    sourceClip: THREE.AnimationClip
  ): THREE.Object3D {
    const stripped = SkeletonUtils.clone(sourceNode) as THREE.Object3D
    this.stripMeshes(stripped)

    const strippedTrackCount = this.filterClipTracksByNodeNames(
      sourceClip,
      this.collectNodeNames(stripped)
    ).keptTracks
    if (strippedTrackCount > 0) {
      stripped.position.set(0, 0, 0)
      stripped.rotation.set(0, 0, 0)
      stripped.scale.set(1, 1, 1)
      return stripped
    }

    const fallback = SkeletonUtils.clone(sourceNode) as THREE.Object3D
    fallback.position.set(0, 0, 0)
    fallback.rotation.set(0, 0, 0)
    fallback.scale.set(1, 1, 1)
    return fallback
  }

  private stripMeshes(root: THREE.Object3D): void {
    const toRemove: THREE.Object3D[] = []
    root.traverse((obj) => {
      if ((obj as THREE.Mesh).isMesh) {
        toRemove.push(obj)
      }
    })

    for (const mesh of toRemove) {
      mesh.parent?.remove(mesh)
    }
  }

  private filterClipTracksByNodeNames(
    clip: THREE.AnimationClip,
    allowedNames: Set<string>
  ): { clip: THREE.AnimationClip; totalTracks: number; keptTracks: number } {
    const keptTracks = clip.tracks
      .filter((track) => {
        const targetName = track.name.split('.')[0]
        return allowedNames.has(targetName)
      })
      .map((track) => track.clone())

    const nextClip = clip.clone()
    nextClip.tracks = keptTracks

    return {
      clip: nextClip,
      totalTracks: clip.tracks.length,
      keptTracks: keptTracks.length,
    }
  }

  private ensureUniqueClipName(name: string, taken: Set<string>): string {
    const base = name.trim() || 'Animation'
    if (!taken.has(base)) {
      taken.add(base)
      return base
    }

    let index = 2
    while (true) {
      const candidate = `${base}_${String(index).padStart(2, '0')}`
      if (!taken.has(candidate)) {
        taken.add(candidate)
        return candidate
      }
      index += 1
    }
  }

  private sanitizePackName(name: string): string {
    const trimmed = name.trim()
    if (!trimmed) return 'animations_pack'

    const safe = trimmed.replace(/[^a-zA-Z0-9_-]/g, '_')
    return safe || 'animations_pack'
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
