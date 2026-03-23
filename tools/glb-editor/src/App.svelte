<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { AnimationClip } from 'three'
  import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { mergeAnimationClips, type MergeOptions } from './lib/merge'
  import PreviewPanel from './lib/components/PreviewPanel.svelte'
  import { GlbViewer, type CandidateSummary } from './lib/viewer'
  import { type MixamoDetectionResult } from './lib/mixamo-bones'
  import BoneMappingModal from './lib/components/BoneMappingModal.svelte'
  import ExtractAnimationModal from './lib/components/ExtractAnimationModal.svelte'
  import ObjectSidebar from './lib/components/ObjectSidebar.svelte'
  import HeaderToolbar from './lib/components/HeaderToolbar.svelte'
  import MergePanel from './lib/components/MergePanel.svelte'
  import {
    ANIMATION_PACK_PATH_HINT,
    loadBasePackFile,
    savePackFileToAnimationsDir,
  } from './lib/animation-pack-api'

  let viewerHost = $state<HTMLDivElement | null>(null)
  let viewer = $state<GlbViewer | null>(null)
  let logEl = $state<HTMLPreElement | null>(null)
  let mergePanelRef = $state<MergePanel | null>(null)

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
  let isLoadingMain = $state(false)
  let hasMergedUnsaved = $state(false)
  let animsBeforeMerge = $state<AnimationClip[] | null>(null)
  let mergePanelHeight = $state(360)
  let isResizingMergePanelHeight = $state(false)

  let boneDetection = $state<MixamoDetectionResult | null>(null)
  let showBonePanel = $state(false)
  let showExtractPanel = $state(false)

  let resizeStartY = 0
  let resizeStartMergeHeight = 0

  const MIN_MERGE_HEIGHT = 240

  const hasCandidate = $derived(selectedCandidateIndex >= 0)
  const hasCandidates = $derived(candidates.length > 0)
  const hasMainClip = $derived(clipNames.length > 0)


  function openExtractPanel(): void {
    if (!hasMainClip) {
      appendLog('추출할 애니메이션이 없습니다.')
      return
    }
    showExtractPanel = true
  }

  function closeExtractPanel(): void {
    showExtractPanel = false
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

  })

  onDestroy(() => {
    viewer?.destroy()
  })

  $effect(() => {
    viewer?.setAutoRotate(autoRotate)
  })

  $effect(() => {
    viewer?.setLoop(loop)
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
    mergePanelRef?.reset()
    hasMergedUnsaved = false
    animsBeforeMerge = null
    showBonePanel = false
    showExtractPanel = false
    boneDetection = null
  }

  function onMerge(gltfB: GLTF, options: MergeOptions): void {
    const gltfA = viewer?.getSourceGLTF() ?? null
    if (!gltfA) return

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
    showBonePanel = true
  }

  function onApplyBoneRename(mapping: Record<string, string>): void {
    if (!viewer || !boneDetection) return

    const changed = viewer.applyBoneRename(mapping)
    if (changed) {
      hasMergedUnsaved = true
    }

    showBonePanel = false
    boneDetection = null
  }

  function onCancelBonePanel(): void {
    showBonePanel = false
    boneDetection = null
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

  async function onExtractAnimation(
    packName: string,
    clipName: string,
    packFilesByName: Record<string, string>,
  ): Promise<void> {
    if (!viewer) return

    try {
      const basePackFile = await loadBasePackFile(packName, packFilesByName)
      const result = await viewer.extractSelectedClipToPack({
        packName,
        clipName,
        basePackFile,
        outputMode: 'return-buffer',
      })
      await savePackFileToAnimationsDir(result.fileName, result.arrayBuffer)
      appendLog(
        `애니메이션 추출 완료: ${result.fileName} (${result.mode === 'append-pack' ? '기존 팩 갱신' : '신규 팩'})`
      )
      closeExtractPanel()
    } catch (error) {
      appendLog(`애니메이션 추출 실패: ${String(error)}`)
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
  }

  function stopMergeHeightResize(): void {
    isResizingMergePanelHeight = false
  }
</script>

<div
  class="app"
  class:resizing={isResizingMergePanelHeight}
  style:grid-template-rows="56px minmax(0,1fr) 10px {mergePanelHeight}px 190px"
>
  <HeaderToolbar
    {metaText}
    isLoading={isLoadingMain}
    {hasCandidate}
    {hasCandidates}
    {hasMainClip}
    {hasMergedUnsaved}
    {autoRotate}
    {loop}
    {onMainFileChange}
    {onExportSelected}
    {onExportAll}
    onExtract={openExtractPanel}
    {onStandardizeBones}
    {onSave}
    {onReset}
    onAutoRotateChange={(v) => (autoRotate = v)}
    onLoopChange={(v) => (loop = v)}
  />

  <ObjectSidebar
    {candidates}
    selectedIndex={selectedCandidateIndex}
    onSelect={onSelectCandidate}
  />

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

  <MergePanel
    bind:this={mergePanelRef}
    {clipNames}
    {hasCandidates}
    {loop}
    canUndo={animsBeforeMerge !== null}
    {appendLog}
    onMerge={onMerge}
    onUndo={onUndoMerge}
  />

  <section class="log">
    <pre bind:this={logEl}>{logText}</pre>
  </section>

  {#if showExtractPanel}
    <ExtractAnimationModal
      {clipNames}
      {selectedClipIndex}
      onExtract={onExtractAnimation}
      onClose={closeExtractPanel}
    />
  {/if}

  {#if showBonePanel && boneDetection}
    <BoneMappingModal
      {boneDetection}
      onApply={onApplyBoneRename}
      onCancel={onCancelBonePanel}
    />
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



</style>
