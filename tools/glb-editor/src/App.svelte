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
  import { GlbViewer, type CandidateSummary } from './lib/viewer'

  let viewerHost = $state<HTMLDivElement | null>(null)
  let viewer = $state<GlbViewer | null>(null)
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
  let isLoadingMain = $state(false)

  let gltfB = $state<GLTF | null>(null)
  let gltfBFileName = $state('')
  let isMerging = $state(false)

  let prefixB = $state(true)
  let rotFixEnabled = $state(false)
  let rotFixAxis = $state<RotationFixAxis>('x')
  let rotFixDeg = $state(-90)
  let rotFixScope = $state<RotationFixScope>('root')
  let rotFixOrder = $state<RotationFixOrder>('pre')

  const hasCandidate = $derived(selectedCandidateIndex >= 0)
  const hasCandidates = $derived(candidates.length > 0)
  const canMerge = $derived(Boolean(viewer?.getSourceGLTF() && gltfB))

  function appendLog(message: string): void {
    logText += `${message}\n`
    queueMicrotask(() => {
      if (logEl) {
        logEl.scrollTop = logEl.scrollHeight
      }
    })
  }

  onMount(() => {
    if (!viewerHost) return

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

  function onClipChange(event: Event): void {
    const select = event.currentTarget as HTMLSelectElement
    const next = Number.parseInt(select.value, 10)
    selectedClipIndex = Number.isNaN(next) ? 0 : next
    viewer?.playClip(selectedClipIndex)
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
    gltfB = null
    gltfBFileName = ''
  }

  async function onLoadBFile(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    if (!file) return

    try {
      gltfB = await loadGLTFFromFile(file)
      gltfBFileName = file.name
      appendLog(`b.glb 로드 완료: ${file.name} (animations: ${gltfB.animations?.length ?? 0})`)
    } catch (error) {
      appendLog(`b.glb 로드 실패: ${String(error)}`)
    } finally {
      input.value = ''
    }
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
</script>

<div class="app">
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
      <select value={String(selectedClipIndex)} onchange={onClipChange} disabled={clipNames.length === 0}>
        {#if clipNames.length === 0}
          <option value="0">애니메이션 없음</option>
        {:else}
          {#each clipNames as clip, index}
            <option value={String(index)}>{clip}</option>
          {/each}
        {/if}
      </select>
      <button class="btn" onclick={() => viewer?.playClip(selectedClipIndex)} disabled={clipNames.length === 0}
        >재생</button
      >
      <button class="btn" onclick={() => viewer?.pause()} disabled={clipNames.length === 0}>일시정지</button>
      <span class="small">{clipInfo}</span>
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

  <section class="merge-panel">
    <h2>애니메이션 병합</h2>
    <p class="small">a.glb: 메인 파일, b.glb: 애니메이션 가져올 파일</p>

    <label class="btn file block">
      b.glb 열기
      <input type="file" accept=".glb,.gltf" onchange={onLoadBFile} />
    </label>

    <div class="small file-name">{gltfBFileName || '선택된 b.glb 없음'}</div>

    <label class="small"><input type="checkbox" bind:checked={prefixB} /> b_ 접두사 자동 부여</label>

    <div class="grid-2">
      <label class="small"><input type="checkbox" bind:checked={rotFixEnabled} /> 회전 보정</label>
      <label class="small"
        >축
        <select bind:value={rotFixAxis}>
          <option value="x">X</option>
          <option value="y">Y</option>
          <option value="z">Z</option>
        </select></label
      >
      <label class="small"
        >각도
        <input type="number" bind:value={rotFixDeg} step="1" />
      </label>
      <label class="small"
        >대상
        <select bind:value={rotFixScope}>
          <option value="root">루트만</option>
          <option value="all">모든 본</option>
        </select></label
      >
      <label class="small"
        >순서
        <select bind:value={rotFixOrder}>
          <option value="pre">pre</option>
          <option value="post">post</option>
        </select></label
      >
    </div>

    <button class="btn primary block" onclick={onMerge} disabled={!canMerge || isMerging}>
      {isMerging ? '병합 중...' : '병합 실행 (merged.glb)'}
    </button>

    <p class="small tip">
      매칭 규칙: 대소문자 무시, 공백/하이픈 제거, Blender 숫자 접미사 제거, 좌/우 표준화,
      Levenshtein 퍼지 매칭.
    </p>
  </section>

  <section class="log">
    <pre bind:this={logEl}>{logText}</pre>
  </section>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 300px 1fr 360px;
    grid-template-rows: 56px 1fr 190px;
    height: 100%;
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

  .btn.block {
    width: 100%;
    margin-top: 10px;
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
  }

  .title {
    margin-bottom: 10px;
  }

  .sidebar {
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
    display: flex;
    align-items: center;
    gap: 8px;
    z-index: 2;
    background: rgb(0 0 0 / 38%);
    padding: 8px;
    border-radius: 10px;
    backdrop-filter: blur(6px);
  }

  .overlay select {
    min-width: 180px;
  }

  .overlay button {
    padding-top: 5px;
    padding-bottom: 5px;
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

  .merge-panel {
    background: #151b2a;
    border-left: 1px solid #000;
    padding: 12px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .file-name {
    margin-bottom: 4px;
  }

  .grid-2 {
    display: grid;
    gap: 8px;
    grid-template-columns: 1fr 1fr;
    align-items: end;
  }

  .grid-2 input,
  .grid-2 select {
    width: 100%;
    margin-top: 4px;
  }

  .tip {
    line-height: 1.45;
  }

  .log {
    grid-column: 1 / -1;
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

  @media (width <= 1300px) {
    .app {
      grid-template-columns: 260px 1fr;
      grid-template-rows: 56px minmax(360px, 1fr) minmax(280px, auto) 170px;
    }

    .merge-panel {
      grid-column: 1 / -1;
      border-left: 0;
      border-top: 1px solid #000;
    }
  }

  @media (width <= 900px) {
    .app {
      grid-template-columns: 1fr;
      grid-template-rows: 120px 240px minmax(340px, 1fr) minmax(320px, auto) 180px;
    }

    .sidebar {
      border-right: 0;
      border-bottom: 1px solid #000;
    }

    .overlay {
      max-width: calc(100% - 20px);
      flex-wrap: wrap;
    }
  }
</style>
