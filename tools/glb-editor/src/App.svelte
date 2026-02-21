<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { AnimationClip } from 'three'
  import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { loadGLTFFromFile } from './lib/gltf-io'
  import {
    mergeAnimationClips,
    type MergeMethod,
    type MergeOptions,
    type RotationFixAxis,
    type RotationFixOrder,
    type RotationFixScope,
  } from './lib/merge'
  import { ClipPreviewer } from './lib/clip-previewer'
  import PreviewPanel from './lib/components/PreviewPanel.svelte'
  import { GlbViewer, type CandidateSummary } from './lib/viewer'
  import { MIXAMO_BONE_NAMES, type MixamoDetectionResult } from './lib/mixamo-bones'

  const DEFAULT_ANIMATION_PACKS = ['locomotion', 'combat_melee', 'combat_ranged', 'emote']
  const ANIMATION_PACK_PATH_HINT = 'client/public/models/animations'
  const PACK_LIST_API = '/__animation_packs'
  const PACK_FILE_API = '/__animation_pack'

  interface AnimationPackEntry {
    packName: string
    fileName: string
  }

  interface AnimationPackListResponse {
    packs: AnimationPackEntry[]
  }

  let viewerHost = $state<HTMLDivElement | null>(null)
  let appHost = $state<HTMLDivElement | null>(null)
  let viewer = $state<GlbViewer | null>(null)
  let bPreviewHost = $state<HTMLDivElement | null>(null)
  let bPreviewer = $state<ClipPreviewer | null>(null)
  let logEl = $state<HTMLPreElement | null>(null)

  let logText = $state('')
  let metaText = $state('')
  let candidates = $state<CandidateSummary[]>([])
  let selectedCandidateIndex = $state(-1)

  let clipNames = $state<string[]>([])
  let selectedClipIndex = $state(0)
  let clipInfo = $state('애니메이션 없음')

  let autoRotate = $state(false)
  let loop = $state(false)
  let dropActive = $state(false)
  let bDropActive = $state(false)
  let isLoadingMain = $state(false)

  let gltfB = $state<GLTF | null>(null)
  let gltfBFileName = $state('')
  let bClipNames = $state<string[]>([])
  let bSelectedClipIndex = $state(0)
  let bClipInfo = $state('애니메이션 없음')
  let isMerging = $state(false)
  let hasMergedUnsaved = $state(false)
  let animsBeforeMerge = $state<AnimationClip[] | null>(null)

  let mergeAnimName = $state('')
  let mergeMethod = $state<MergeMethod>('retarget')
  let retargetKeepRootMotion = $state(true)
  let retargetNormalizeRootStart = $state(true)
  let retargetKeepVerticalRootMotion = $state(false)
  let rotFixEnabled = $state(false)
  let rotFixAxis = $state<RotationFixAxis>('x')
  let rotFixDeg = $state(-90)
  let rotFixScope = $state<RotationFixScope>('root')
  let rotFixOrder = $state<RotationFixOrder>('pre')
  let mergePanelHeight = $state(360)
  let isResizingMergePanelHeight = $state(false)

  let boneDetection = $state<MixamoDetectionResult | null>(null)
  let manualMapping = $state<Record<string, string>>({})
  let showBonePanel = $state(false)
  let showExtractPanel = $state(false)
  let animationPacks = $state<string[]>([...DEFAULT_ANIMATION_PACKS])
  let packFilesByName = $state<Record<string, string>>({})
  let selectedPackName = $state(DEFAULT_ANIMATION_PACKS[0] ?? '')
  let extractClipName = $state('')
  let isLoadingPackCatalog = $state(false)
  let isExtracting = $state(false)

  let resizeStartY = 0
  let resizeStartMergeHeight = 0

  const MIN_MERGE_HEIGHT = 240

  const hasCandidate = $derived(selectedCandidateIndex >= 0)
  const hasCandidates = $derived(candidates.length > 0)
  const hasMainClip = $derived(clipNames.length > 0)
  const hasBClip = $derived(bClipNames.length > 0)
  const trimmedMergeName = $derived(mergeAnimName.trim())
  const trimmedSelectedPackName = $derived(selectedPackName.trim())
  const trimmedExtractClipName = $derived(extractClipName.trim())
  const selectedPackFileName = $derived(packFilesByName[trimmedSelectedPackName] ?? '')
  const selectedPackExists = $derived(selectedPackFileName !== '')
  const mergeNameConflict = $derived(
    trimmedMergeName !== '' && clipNames.includes(trimmedMergeName),
  )
  const canMerge = $derived(
    hasCandidates &&
      gltfB !== null &&
      hasBClip &&
      trimmedMergeName !== '' &&
      !mergeNameConflict,
  )
  const canExtract = $derived(
    hasMainClip && trimmedSelectedPackName !== '' && trimmedExtractClipName !== '',
  )

  const allBoneNames = $derived(
    boneDetection
      ? [
          ...Object.keys(boneDetection.nameMap),
          ...boneDetection.unmatchedBones,
        ]
      : [],
  )
  const assignedMixamoNames = $derived(new Set(Object.values(manualMapping)))

  function uniqueNames(items: string[]): string[] {
    const out: string[] = []
    for (const item of items) {
      const normalized = item.trim()
      if (!normalized || out.includes(normalized)) continue
      out.push(normalized)
    }
    return out
  }

  async function loadAnimationPackCatalog(): Promise<void> {
    isLoadingPackCatalog = true
    try {
      const response = await fetch(PACK_LIST_API, { cache: 'no-store' })
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`)
      }

      const data = (await response.json()) as AnimationPackListResponse
      const nextMap: Record<string, string> = {}
      for (const pack of data.packs ?? []) {
        if (!pack?.packName || !pack?.fileName) continue
        nextMap[pack.packName.trim()] = pack.fileName.trim()
      }

      packFilesByName = nextMap
      animationPacks = uniqueNames([...Object.keys(nextMap), ...DEFAULT_ANIMATION_PACKS])
      if (!animationPacks.includes(selectedPackName)) {
        selectedPackName = animationPacks[0] ?? DEFAULT_ANIMATION_PACKS[0] ?? ''
      }
      appendLog(`애니메이션 팩 스캔 완료: ${Object.keys(nextMap).length}개 파일`)
    } catch (error) {
      packFilesByName = {}
      animationPacks = [...DEFAULT_ANIMATION_PACKS]
      if (!animationPacks.includes(selectedPackName)) {
        selectedPackName = animationPacks[0] ?? ''
      }
      appendLog(`애니메이션 팩 스캔 실패: ${String(error)}`)
    } finally {
      isLoadingPackCatalog = false
    }
  }

  async function loadBasePackFile(packName: string): Promise<File | null> {
    const fileName = packFilesByName[packName]
    if (!fileName) return null

    const response = await fetch(`${PACK_FILE_API}?file=${encodeURIComponent(fileName)}`, {
      cache: 'no-store',
    })
    if (!response.ok) {
      throw new Error(`기존 팩 파일 로드 실패: ${fileName} (HTTP ${response.status})`)
    }

    const blob = await response.blob()
    return new File([blob], fileName, { type: blob.type || 'model/gltf-binary' })
  }

  async function savePackFileToAnimationsDir(
    fileName: string,
    arrayBuffer: ArrayBuffer,
  ): Promise<void> {
    const response = await fetch(`${PACK_FILE_API}?file=${encodeURIComponent(fileName)}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/octet-stream',
      },
      body: arrayBuffer,
    })

    if (!response.ok) {
      throw new Error(`팩 파일 저장 실패: ${fileName} (HTTP ${response.status})`)
    }
  }

  function openExtractPanel(): void {
    if (!hasMainClip) {
      appendLog('추출할 애니메이션이 없습니다.')
      return
    }

    showExtractPanel = true
    selectedPackName = animationPacks[0] ?? DEFAULT_ANIMATION_PACKS[0] ?? ''
    extractClipName = clipNames[selectedClipIndex] ?? 'Animation'
    void loadAnimationPackCatalog()
  }

  function closeExtractPanel(): void {
    showExtractPanel = false
  }

  function onRefreshPackCatalog(): void {
    void loadAnimationPackCatalog()
  }

  function appendLog(message: string): void {
    logText += `${message}\n`
    queueMicrotask(() => {
      if (logEl) {
        logEl.scrollTop = logEl.scrollHeight
      }
    })
  }

  onMount(() => {
    if (viewerHost) {
      viewer = new GlbViewer(viewerHost, {
        log: appendLog,
        onMetaChange: (message) => {
          metaText = message
        },
        onCandidatesChange: (items, selected) => {
          candidates = items
          selectedCandidateIndex = selected
        },
        onClipsChange: (clips, selected, info) => {
          clipNames = clips
          selectedClipIndex = selected
          clipInfo = info
        },
      })
      viewer.setAutoRotate(autoRotate)
      viewer.setLoop(loop)
    }

    if (bPreviewHost) {
      bPreviewer = new ClipPreviewer(bPreviewHost)
      bPreviewer.setLoop(loop)
    }

    void loadAnimationPackCatalog()
  })

  onDestroy(() => {
    viewer?.destroy()
    bPreviewer?.destroy()
  })

  $effect(() => {
    viewer?.setAutoRotate(autoRotate)
  })

  $effect(() => {
    viewer?.setLoop(loop)
    bPreviewer?.setLoop(loop)
  })

  async function handleMainFile(file: File): Promise<void> {
    if (!viewer) return

    isLoadingMain = true
    try {
      await viewer.loadFile(file)
    } catch (error) {
      appendLog(`메인 파일 로드 실패: ${String(error)}`)
    } finally {
      isLoadingMain = false
    }
  }

  async function onMainFileChange(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    if (!file) return

    await handleMainFile(file)
    input.value = ''
  }

  function onDragOver(event: DragEvent): void {
    event.preventDefault()
    dropActive = true
  }

  function onDragLeave(event: DragEvent): void {
    event.preventDefault()
    dropActive = false
  }

  async function onDrop(event: DragEvent): Promise<void> {
    event.preventDefault()
    dropActive = false

    const file = event.dataTransfer?.files?.[0]
    if (!file) return
    await handleMainFile(file)
  }

  function onSelectCandidate(index: number): void {
    viewer?.selectCandidate(index)
  }

  async function onExportSelected(): Promise<void> {
    await viewer?.exportSelected()
  }

  async function onExportAll(): Promise<void> {
    await viewer?.exportAll()
  }

  function onReset(): void {
    viewer?.reset()
    bPreviewer?.clear()
    gltfB = null
    gltfBFileName = ''
    bClipNames = []
    bSelectedClipIndex = 0
    bClipInfo = '애니메이션 없음'
    hasMergedUnsaved = false
    animsBeforeMerge = null
    showBonePanel = false
    showExtractPanel = false
    boneDetection = null
    manualMapping = {}
    extractClipName = ''
  }

  async function handleBFile(file: File): Promise<void> {
    try {
      gltfB = await loadGLTFFromFile(file)
      gltfBFileName = file.name
      const clips = gltfB.animations ?? []
      bClipNames = clips.map((clip, index) => clip.name?.trim() || `Clip ${index + 1}`)
      bSelectedClipIndex = 0
      bClipInfo = clips.length > 0 ? `${clips.length} clip(s)` : '애니메이션 없음'
      bPreviewer?.loadGLTF(gltfB)
      appendLog(`b.glb 로드 완료: ${file.name} (animations: ${gltfB.animations?.length ?? 0})`)
    } catch (error) {
      appendLog(`b.glb 로드 실패: ${String(error)}`)
    }
  }

  async function onLoadBFile(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    if (!file) return
    await handleBFile(file)
    input.value = ''
  }

  function onBDragOver(event: DragEvent): void {
    event.preventDefault()
    bDropActive = true
  }

  function onBDragLeave(event: DragEvent): void {
    event.preventDefault()
    bDropActive = false
  }

  async function onBDrop(event: DragEvent): Promise<void> {
    event.preventDefault()
    bDropActive = false
    const file = event.dataTransfer?.files?.[0]
    if (!file) return
    await handleBFile(file)
  }

  function onMerge(): void {
    const gltfA = viewer?.getSourceGLTF() ?? null
    if (!gltfA || !gltfB) return

    const options: MergeOptions = {
      animName: trimmedMergeName,
      mergeMethod,
      rotationFix: {
        enabled: rotFixEnabled,
        axis: rotFixAxis,
        deg: Number(rotFixDeg),
        scope: rotFixScope,
        order: rotFixOrder,
      },
      retarget: {
        keepRootMotion: retargetKeepRootMotion,
        normalizeRootStart: retargetNormalizeRootStart,
        keepVerticalRootMotion: retargetKeepVerticalRootMotion,
      },
      selectedBClipIndex: bSelectedClipIndex,
    }

    isMerging = true
    try {
      const output = mergeAnimationClips(gltfA, gltfB, options, appendLog)
      if (!gltfA.animations) gltfA.animations = []
      animsBeforeMerge = [...gltfA.animations]
      gltfA.animations.push(...output.clips)
      viewer?.refreshPreview()
      hasMergedUnsaved = true
      appendLog('병합 완료 (메모리). 미리보기에서 확인 후 저장하세요.')
    } catch (error) {
      appendLog(`병합 실패: ${String(error)}`)
    } finally {
      isMerging = false
    }
  }

  async function onSave(): Promise<void> {
    if (!viewer) return

    const result = await viewer.saveCurrentGLB()
    if (!result) return

    try {
      await savePackFileToAnimationsDir(result.fileName, result.arrayBuffer)
      appendLog(
        `GLB 저장 완료: ${ANIMATION_PACK_PATH_HINT}/${result.fileName} (파일시스템 기록)`
      )
      hasMergedUnsaved = false
      animsBeforeMerge = null
      await loadAnimationPackCatalog()
    } catch (error) {
      appendLog(`GLB 파일 저장 실패: ${String(error)}`)
    }
  }

  function onDeleteClip(): void {
    const gltfA = viewer?.getSourceGLTF() ?? null
    if (!gltfA) return

    animsBeforeMerge = [...(gltfA.animations ?? [])]
    const deleted = viewer?.deleteCurrentClip()
    if (deleted) {
      hasMergedUnsaved = true
      appendLog('애니메이션 삭제 완료. 저장 또는 되돌리기 가능.')
    }
  }

  function onStandardizeBones(): void {
    if (!viewer) return
    const result = viewer.detectBones()
    if (!result) return

    boneDetection = result
    manualMapping = { ...result.nameMap }
    showBonePanel = true
  }

  function onManualMappingChange(boneName: string, mixamoName: string): void {
    if (mixamoName === '') {
      const next = { ...manualMapping }
      delete next[boneName]
      manualMapping = next
    } else {
      manualMapping = { ...manualMapping, [boneName]: mixamoName }
    }
  }

  function onApplyBoneRename(): void {
    if (!viewer || !boneDetection) return

    const changed = viewer.applyBoneRename(manualMapping)
    if (changed) {
      hasMergedUnsaved = true
    }

    showBonePanel = false
    boneDetection = null
    manualMapping = {}
  }

  function onCancelBonePanel(): void {
    showBonePanel = false
    boneDetection = null
    manualMapping = {}
  }

  function onUndoMerge(): void {
    const gltfA = viewer?.getSourceGLTF() ?? null
    if (!gltfA || !animsBeforeMerge) return

    gltfA.animations = animsBeforeMerge
    animsBeforeMerge = null
    hasMergedUnsaved = false
    viewer?.refreshPreview()
    appendLog('병합 되돌리기 완료')
  }

  async function onExtractAnimation(): Promise<void> {
    if (!viewer || !canExtract) return

    isExtracting = true
    try {
      const basePackFile = await loadBasePackFile(trimmedSelectedPackName)
      const result = await viewer.extractSelectedClipToPack({
        packName: trimmedSelectedPackName,
        clipName: trimmedExtractClipName,
        basePackFile,
        outputMode: 'return-buffer',
      })
      await savePackFileToAnimationsDir(result.fileName, result.arrayBuffer)
      appendLog(
        `애니메이션 추출 완료: ${result.fileName} (${result.mode === 'append-pack' ? '기존 팩 갱신' : '신규 팩'})`
      )
      await loadAnimationPackCatalog()
      closeExtractPanel()
    } catch (error) {
      appendLog(`애니메이션 추출 실패: ${String(error)}`)
    } finally {
      isExtracting = false
    }
  }

  function clampMergeHeight(next: number): number {
    return Math.max(MIN_MERGE_HEIGHT, next)
  }

  function onMergeHeightResizerPointerDown(event: PointerEvent): void {
    if (event.button !== 0) return
    event.preventDefault()
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
    resizeStartY = event.clientY
    resizeStartMergeHeight = mergePanelHeight
    isResizingMergePanelHeight = true
  }

  function onMergeHeightResizerPointerMove(event: PointerEvent): void {
    if (!isResizingMergePanelHeight) return
    if (event.buttons === 0) {
      stopMergeHeightResize()
      return
    }
    const delta = event.clientY - resizeStartY
    mergePanelHeight = clampMergeHeight(resizeStartMergeHeight - delta)
    console.log('merge panel height:', mergePanelHeight)
  }

  function stopMergeHeightResize(): void {
    isResizingMergePanelHeight = false
  }
</script>

<div
  class="app"
  class:resizing={isResizingMergePanelHeight}
  bind:this={appHost}
  style:grid-template-rows="56px minmax(0,1fr) 10px {mergePanelHeight}px 190px"
>
  <header>
    <h1>GLB Editor</h1>
    <div class="toolbar">
      <label class="btn file">
        메인 GLB 열기
        <input type="file" accept=".glb,.gltf" onchange={onMainFileChange} />
      </label>
      <button class="btn primary" onclick={onExportSelected} disabled={!hasCandidate}>선택 내보내기</button>
      <button class="btn" onclick={onExportAll} disabled={!hasCandidates}>전체 내보내기</button>
      <button class="btn" onclick={openExtractPanel} disabled={!hasMainClip}>애니메이션 추출</button>
      <button class="btn" onclick={onStandardizeBones} disabled={!hasCandidates}>본 이름 표준화</button>
      <button class="btn save" onclick={onSave} disabled={!hasMergedUnsaved}>저장 (파일시스템)</button>
      <button class="btn ghost" onclick={onReset}>초기화</button>
      <span class="small">{isLoadingMain ? '로딩 중...' : metaText}</span>
    </div>
    <div class="spacer"></div>
    <div class="toolbar">
      <label><input type="checkbox" bind:checked={autoRotate} /> AutoRotate</label>
      <label><input type="checkbox" bind:checked={loop} /> Loop</label>
    </div>
  </header>

  <aside class="sidebar">
    <div class="small title">오브젝트 목록 (메시 포함 노드)</div>
    <div class="list">
      {#each candidates as item (item.index)}
        <button
          class="item"
          class:active={item.index === selectedCandidateIndex}
          onclick={() => onSelectCandidate(item.index)}
        >
          <div class="name">{item.name}</div>
          <div class="small">{item.stats}</div>
        </button>
      {/each}
    </div>
  </aside>

  <main class="viewer-panel">
    <PreviewPanel
      clips={clipNames}
      {selectedClipIndex}
      clipInfo={clipInfo}
      {dropActive}
      onClipChange={(index) => {
        selectedClipIndex = index
        viewer?.playClip(selectedClipIndex)
      }}
      onPlay={() => viewer?.playClip(selectedClipIndex)}
      onPause={() => viewer?.pause()}
      onDelete={onDeleteClip}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      bindHost={(el) => (viewerHost = el)}
    />
  </main>

  <button
    class="panel-resizer"
    type="button"
    aria-label="Merge panel height resize handle"
    onpointerdown={onMergeHeightResizerPointerDown}
    onpointermove={onMergeHeightResizerPointerMove}
    onlostpointercapture={stopMergeHeightResize}
  ></button>

  <section class="merge-panel">
    <div class="merge-top">
      <div class="merge-top-left">
        <div class="merge-header">
          <h2>애니메이션 병합</h2>
          <label class="btn file">
            GLB 열기
            <input type="file" accept=".glb,.gltf" onchange={onLoadBFile} />
          </label>
          <button class="btn primary" onclick={onMerge} disabled={!canMerge || isMerging}>
            {isMerging ? '병합 중...' : '병합 실행'}
          </button>
          <button class="btn ghost" onclick={onUndoMerge} disabled={!animsBeforeMerge}>
            되돌리기
          </button>
        </div>

        <div class="small file-name">{gltfBFileName || ''}</div>

        <label class="small">
          <span class="lbl-prefix">애님 이름</span>
          <input class="anim-name-input" class:conflict={mergeNameConflict} type="text" bind:value={mergeAnimName} placeholder="병합할 애님 이름" />
        </label>
        {#if mergeNameConflict}
          <span class="small conflict-msg">이미 존재하는 이름입니다</span>
        {/if}
        <label class="small">
          <span class="lbl-prefix">병합 방식</span>
          <select bind:value={mergeMethod}>
            <option value="retarget">리타겟 (권장)</option>
            <option value="track-map">트랙 매핑</option>
          </select>
        </label>
        {#if mergeMethod === 'retarget'}
          <label class="small"
            ><input type="checkbox" bind:checked={retargetKeepRootMotion} /> 루트 모션 유지</label
          >
          <label class="small indent"
            ><input type="checkbox" bind:checked={retargetNormalizeRootStart} /> 시작점 정렬</label
          >
          <label class="small indent"
            ><input type="checkbox" bind:checked={retargetKeepVerticalRootMotion} /> 수직 루트 모션(Y)
            유지</label
          >
        {/if}
        <label class="small"><input type="checkbox" bind:checked={rotFixEnabled} /> 회전 보정</label>
        <div class="grid-2 indent">
          <label class="small"
            ><span class="lbl">축</span>
            <select bind:value={rotFixAxis}>
              <option value="x">X</option>
              <option value="y">Y</option>
              <option value="z">Z</option>
            </select></label
          >
          <label class="small"
            ><span class="lbl">각도</span>
            <input type="number" bind:value={rotFixDeg} step="1" />
          </label>
          <label class="small"
            ><span class="lbl">대상</span>
            <select bind:value={rotFixScope}>
              <option value="root">루트만</option>
              <option value="all">모든 본</option>
            </select></label
          >
          <label class="small"
            ><span class="lbl">순서</span>
            <select bind:value={rotFixOrder}>
              <option value="pre">pre</option>
              <option value="post">post</option>
            </select></label
          >
        </div>
      </div>

      <div class="b-preview-wrap">
        <PreviewPanel
          clips={bClipNames}
          selectedClipIndex={bSelectedClipIndex}
          clipInfo={bClipInfo}
          dropActive={bDropActive}
          onClipChange={(index) => {
            bSelectedClipIndex = index
            bPreviewer?.playClip(bSelectedClipIndex)
          }}
          onPlay={() => bPreviewer?.playClip(bSelectedClipIndex)}
          onPause={() => bPreviewer?.pause()}
          onDragOver={onBDragOver}
          onDragLeave={onBDragLeave}
          onDrop={onBDrop}
          bindHost={(el) => (bPreviewHost = el)}
        />
      </div>
    </div>
  </section>

  <section class="log">
    <pre bind:this={logEl}>{logText}</pre>
  </section>

  {#if showExtractPanel}
    <div class="dialog-overlay">
      <div class="extract-panel">
        <div class="extract-panel-header">
          <h2>애니메이션 추출</h2>
          <div class="spacer"></div>
          <button class="btn ghost" onclick={closeExtractPanel}>닫기</button>
        </div>

        <div class="extract-panel-body">
          <p class="small path-hint">권장 저장 위치: {ANIMATION_PACK_PATH_HINT}</p>
          <label class="small full-width">
            <span class="lbl-prefix">클립 이름</span>
            <input class="clip-name-input" type="text" bind:value={extractClipName} placeholder="추출할 클립 이름" />
          </label>

          <div class="small">애니메이션 팩 선택</div>
          <div class="pack-list">
            {#each animationPacks as pack (pack)}
              <label class="pack-item">
                <input type="radio" name="animation-pack" value={pack} bind:group={selectedPackName} />
                <span>{pack}</span>
              </label>
            {/each}
          </div>

          <div class="pack-meta-row">
            <span class="small">
              {#if selectedPackExists}
                기존 팩 파일: {selectedPackFileName}
              {:else}
                선택한 팩 파일이 없어 신규 팩으로 저장됩니다.
              {/if}
            </span>
            <button class="btn ghost" onclick={onRefreshPackCatalog} disabled={isLoadingPackCatalog || isExtracting}>
              {isLoadingPackCatalog ? '스캔 중...' : '폴더 다시 스캔'}
            </button>
          </div>
        </div>

        <div class="extract-panel-footer">
          <button class="btn ghost" onclick={closeExtractPanel} disabled={isExtracting}>취소</button>
          <button class="btn primary" onclick={onExtractAnimation} disabled={!canExtract || isExtracting}>
            {isExtracting ? '추출 중...' : '추출 실행'}
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if showBonePanel && boneDetection}
    <div class="bone-overlay">
      <div class="bone-panel">
        <div class="bone-panel-header">
          <h2>본 이름 매핑</h2>
          <span class="small">
            자동 매칭: {Object.keys(boneDetection.nameMap).length}개 /
            매칭 안 됨: {boneDetection.unmatchedBones.length}개
          </span>
          <div class="spacer"></div>
          <button class="btn primary" onclick={onApplyBoneRename}>적용</button>
          <button class="btn ghost" onclick={onCancelBonePanel}>취소</button>
        </div>

        <div class="bone-lists">
          <div class="bone-rows">
            {#each allBoneNames as bone (bone)}
              {@const currentValue = manualMapping[bone] ?? ''}
              {@const isAutoMatched = bone in boneDetection.nameMap}
              <div class="bone-row" class:unmatched={!isAutoMatched}>
                <span class="bone-name">{bone}</span>
                <span class="bone-arrow">→</span>
                <select
                  class="bone-select"
                  value={currentValue}
                  onchange={(e) => onManualMappingChange(bone, (e.currentTarget as HTMLSelectElement).value)}
                >
                  <option value="">(매핑 안 함)</option>
                  {#if currentValue}
                    <option value={currentValue}>{currentValue}</option>
                  {/if}
                  {#each MIXAMO_BONE_NAMES as mx (mx)}
                    {#if !assignedMixamoNames.has(mx) || mx === currentValue}
                      {#if mx !== currentValue}
                        <option value={mx}>{mx}</option>
                      {/if}
                    {/if}
                  {/each}
                </select>
              </div>
            {/each}
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 300px minmax(0, 1fr);
    /* grid-template-rows is set via inline style for reactive resize */
    height: 100%;
    overflow: hidden;
  }

  .app.resizing {
    user-select: none;
  }

  .app.resizing * {
    cursor: row-resize !important;
  }

  header {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    background: #0b0f1a;
    border-bottom: 1px solid #000;
  }

  h1 {
    margin: 0;
    font-size: 15px;
    color: #c7d2fe;
  }

  h2 {
    margin: 0;
    font-size: 15px;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .spacer {
    flex: 1;
  }

  .btn {
    background: #1f2635;
    border: 1px solid #0a0d14;
    color: #e5e7eb;
    border-radius: 8px;
    padding: 7px 10px;
    cursor: pointer;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn.primary {
    background: #0a6f87;
    border-color: #064253;
  }

  .btn.ghost {
    background: transparent;
    border-color: #2c3650;
  }

  .btn.save {
    background: #0a7a3e;
    border-color: #065226;
  }



  .file {
    position: relative;
    overflow: hidden;
  }

  .file input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }

  .small {
    color: #9ca3af;
    font-size: 12px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }

  .title {
    margin-bottom: 10px;
  }

  .sidebar {
    grid-column: 1;
    grid-row: 2;
    background: #1a1f2d;
    border-right: 1px solid #000;
    padding: 10px;
    overflow: auto;
  }

  .list {
    display: grid;
    gap: 8px;
  }

  .item {
    text-align: left;
    border: 1px solid #07090f;
    background: #111622;
    padding: 8px;
    border-radius: 8px;
    cursor: pointer;
    color: inherit;
  }

  .item.active {
    outline: 2px solid #67e8f9;
  }

  .name {
    font-weight: 700;
    margin-bottom: 4px;
    overflow-wrap: anywhere;
  }

  .viewer-panel {
    grid-column: 2;
    grid-row: 2;
    position: relative;
    background: #090b12;
  }


  .panel-resizer {
    grid-column: 1 / -1;
    grid-row: 3;
    width: 100%;
    height: 100%;
    border: 0;
    border-top: 1px solid #000;
    border-bottom: 1px solid #000;
    background: #0f1320;
    cursor: row-resize;
    touch-action: none;
    padding: 0;
    margin: 0;
    opacity: 0.9;
  }

  .panel-resizer:hover {
    background: #182034;
  }

  .merge-panel {
    grid-column: 1 / -1;
    grid-row: 4;
    background: #151b2a;
    border-left: 0;
    border-top: 1px solid #000;
    padding: 12px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }

  .merge-top {
    display: grid;
    grid-template-columns: minmax(220px, 1fr) minmax(280px, 1.2fr);
    grid-template-rows: minmax(0, 1fr);
    gap: 10px;
    flex: 1;
    min-height: 0;
  }

  .merge-top-left {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    overflow: auto;
  }

  .merge-header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .file-name {
    margin-bottom: 4px;
  }

  .b-preview-wrap {
    position: relative;
    border: 1px solid #1f283d;
    border-radius: 10px;
    overflow: hidden;
    background: #090b12;
    min-height: 0;
  }


  .lbl-prefix {
    flex-shrink: 0;
  }

  .anim-name-input {
    width: 140px;
    background: #1f2635;
    border: 1px solid #2c3650;
    border-radius: 4px;
    color: #e5e7eb;
    padding: 2px 6px;
    font-size: 12px;
  }

  .anim-name-input.conflict {
    border-color: #ef4444;
  }

  .conflict-msg {
    color: #ef4444 !important;
  }

  .grid-2 {
    display: grid;
    gap: 8px;
    grid-template-columns: 1fr 1fr;
    align-items: center;
  }

  .indent {
    margin-left: 20px;
  }

  .grid-2 .lbl {
    display: inline-block;
    width: 28px;
    flex-shrink: 0;
  }

  .grid-2 input,
  .grid-2 select {
    width: 60px;
    margin-top: 0;
  }

  .log {
    grid-column: 1 / -1;
    grid-row: 5;
    border-top: 1px solid #000;
    background: #0f1320;
    padding: 8px;
    overflow: auto;
  }

  .log pre {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', monospace;
    font-size: 12px;
    white-space: pre-wrap;
    color: #dbeafe;
  }

  .dialog-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 90;
  }

  .extract-panel {
    background: #151b2a;
    border: 1px solid #2c3650;
    border-radius: 12px;
    width: min(560px, 92vw);
    max-height: 80vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .extract-panel-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid #2c3650;
  }

  .extract-panel-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 16px;
    overflow: auto;
  }

  .extract-panel-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid #2c3650;
  }

  .path-hint {
    white-space: normal;
  }

  .full-width {
    width: 100%;
  }

  .clip-name-input {
    background: #1f2635;
    border: 1px solid #2c3650;
    border-radius: 6px;
    color: #e5e7eb;
    padding: 6px 8px;
    font-size: 12px;
    min-width: 0;
    flex: 1;
  }

  .pack-list {
    display: grid;
    gap: 6px;
    border: 1px solid #1f283d;
    border-radius: 8px;
    padding: 8px;
    max-height: 180px;
    overflow: auto;
    background: #0f1320;
  }

  .pack-item {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #e5e7eb;
    font-size: 13px;
  }

  .pack-meta-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 6px;
  }

  .bone-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .bone-panel {
    background: #151b2a;
    border: 1px solid #2c3650;
    border-radius: 12px;
    width: min(700px, 90vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .bone-panel-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid #2c3650;
    flex-wrap: wrap;
  }

  .bone-lists {
    overflow: auto;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .bone-rows {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .bone-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 12px;
  }

  .bone-row.unmatched {
    background: #1a2236;
  }

  .bone-name {
    flex: 1;
    overflow-wrap: anywhere;
    color: #e5e7eb;
  }

  .bone-arrow {
    color: #4b5563;
    flex-shrink: 0;
  }

  .bone-select {
    flex: 1;
    background: #1f2635;
    border: 1px solid #2c3650;
    border-radius: 4px;
    color: #e5e7eb;
    padding: 3px 6px;
    font-size: 12px;
  }

</style>
