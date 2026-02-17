<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { downloadBlob, loadGLTFFromFile } from './lib/gltf-io'
  import {
    mergeAnimationsIntoA,
    type MergeOptions,
    type RotationFixAxis,
    type RotationFixOrder,
    type RotationFixScope,
  } from './lib/merge'
  import { ClipPreviewer } from './lib/clip-previewer'
  import AnimationClipControls from './lib/components/AnimationClipControls.svelte'
  import { GlbViewer, type CandidateSummary } from './lib/viewer'

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
  let loop = $state(true)
  let dropActive = $state(false)
  let bDropActive = $state(false)
  let isLoadingMain = $state(false)

  let gltfB = $state<GLTF | null>(null)
  let gltfBFileName = $state('')
  let bClipNames = $state<string[]>([])
  let bSelectedClipIndex = $state(0)
  let bClipInfo = $state('애니메이션 없음')
  let isMerging = $state(false)

  let prefixB = $state(true)
  let rotFixEnabled = $state(false)
  let rotFixAxis = $state<RotationFixAxis>('x')
  let rotFixDeg = $state(-90)
  let rotFixScope = $state<RotationFixScope>('root')
  let rotFixOrder = $state<RotationFixOrder>('pre')
  let mergePanelHeight = $state(360)
  let isResizingMergePanelHeight = $state(false)

  let resizeStartY = 0
  let resizeStartMergeHeight = 0

  const MIN_MERGE_HEIGHT = 240

  const hasCandidate = $derived(selectedCandidateIndex >= 0)
  const hasCandidates = $derived(candidates.length > 0)
  const canMerge = $derived(Boolean(viewer?.getSourceGLTF() && gltfB))
  const hasBClip = $derived(bClipNames.length > 0)

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

  async function onMerge(): Promise<void> {
    const gltfA = viewer?.getSourceGLTF() ?? null
    if (!gltfA || !gltfB) return

    const options: MergeOptions = {
      prefixB,
      rotationFix: {
        enabled: rotFixEnabled,
        axis: rotFixAxis,
        deg: Number(rotFixDeg),
        scope: rotFixScope,
        order: rotFixOrder,
      },
      selectedBClipIndex: hasBClip ? bSelectedClipIndex : null,
    }

    isMerging = true
    try {
      const output = await mergeAnimationsIntoA(gltfA, gltfB, options, appendLog)
      downloadBlob('merged.glb', output.merged)
      appendLog('병합 완료: merged.glb 다운로드')
    } catch (error) {
      appendLog(`병합 실패: ${String(error)}`)
    } finally {
      isMerging = false
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
      {#each candidates as item}
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
    <div class="overlay">
      <AnimationClipControls
        clips={clipNames}
        selectedIndex={selectedClipIndex}
        info={clipInfo}
        onChange={(index) => {
          selectedClipIndex = index
          viewer?.playClip(selectedClipIndex)
        }}
        onPlay={() => viewer?.playClip(selectedClipIndex)}
        onPause={() => viewer?.pause()}
        emptyLabel="애니메이션 없음"
      />
    </div>

    <div
      class="viewer"
      bind:this={viewerHost}
      role="region"
      aria-label="GLB viewer drop target"
      ondragenter={onDragOver}
      ondragover={onDragOver}
      ondragleave={onDragLeave}
      ondrop={onDrop}
    >
      <div class="dropzone" class:active={dropActive}>여기에 GLB 파일을 드래그 앤 드롭</div>
    </div>
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
        </div>

        <div class="small file-name">{gltfBFileName || ''}</div>

        <label class="small"><input type="checkbox" bind:checked={prefixB} /> b_ 접두사 자동 부여</label>
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

      <div
        class="b-preview-wrap"
        role="region"
        aria-label="b glb drop target"
        ondragenter={onBDragOver}
        ondragover={onBDragOver}
        ondragleave={onBDragLeave}
        ondrop={onBDrop}
      >
        <div class="b-preview-overlay">
          <AnimationClipControls
            clips={bClipNames}
            selectedIndex={bSelectedClipIndex}
            info={bClipInfo}
            onChange={(index) => {
              bSelectedClipIndex = index
              bPreviewer?.playClip(bSelectedClipIndex)
            }}
            onPlay={() => bPreviewer?.playClip(bSelectedClipIndex)}
            onPause={() => bPreviewer?.pause()}
            emptyLabel="b 애니메이션 없음"
          />
        </div>
        <div class="dropzone b-dropzone" class:active={bDropActive}>여기에 b.glb 파일을 드래그 앤 드롭</div>

        <div
          class="b-preview"
          bind:this={bPreviewHost}
          role="region"
          aria-label="b glb animation preview"
        ></div>
      </div>
    </div>
  </section>

  <section class="log">
    <pre bind:this={logEl}>{logText}</pre>
  </section>
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

  .viewer {
    width: 100%;
    height: 100%;
  }

  .overlay {
    position: absolute;
    left: 10px;
    top: 10px;
    z-index: 2;
    background: rgb(0 0 0 / 38%);
    padding: 8px;
    border-radius: 10px;
    backdrop-filter: blur(6px);
  }

  .dropzone {
    position: absolute;
    inset: 12px;
    border: 2px dashed #364052;
    border-radius: 10px;
    display: none;
    place-items: center;
    color: #9ca3af;
    background: rgb(0 0 0 / 28%);
    pointer-events: none;
  }

  .dropzone.active {
    display: grid;
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

  .b-preview-overlay {
    position: absolute;
    left: 10px;
    top: 10px;
    z-index: 2;
    background: rgb(0 0 0 / 38%);
    padding: 8px;
    border-radius: 10px;
    backdrop-filter: blur(6px);
    max-width: calc(100% - 20px);
  }

  .b-preview {
    width: 100%;
    height: 100%;
    min-height: 230px;
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

</style>
