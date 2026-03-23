<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { loadGLTFFromFile } from '../gltf-io'
  import {
    type MergeMethod,
    type MergeOptions,
    type RotationFixAxis,
    type RotationFixOrder,
    type RotationFixScope,
  } from '../merge'
  import { ClipPreviewer } from '../clip-previewer'
  import PreviewPanel from './PreviewPanel.svelte'

  interface Props {
    clipNames: string[]
    hasCandidates: boolean
    loop: boolean
    canUndo: boolean
    appendLog: (msg: string) => void
    onMerge: (gltfB: GLTF, options: MergeOptions) => void
    onUndo: () => void
  }

  let {
    clipNames,
    hasCandidates,
    loop,
    canUndo,
    appendLog,
    onMerge,
    onUndo,
  }: Props = $props()

  let bPreviewHost = $state<HTMLDivElement | null>(null)
  let bPreviewer = $state<ClipPreviewer | null>(null)

  let gltfB = $state<GLTF | null>(null)
  let gltfBFileName = $state('')
  let bClipNames = $state<string[]>([])
  let bSelectedClipIndex = $state(0)
  let bClipInfo = $state('애니메이션 없음')
  let bDropActive = $state(false)
  let isMerging = $state(false)

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

  const hasBClip = $derived(bClipNames.length > 0)
  const trimmedMergeName = $derived(mergeAnimName.trim())
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

  function handleMerge(): void {
    if (!gltfB || !canMerge) return

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
      onMerge(gltfB, options)
    } finally {
      isMerging = false
    }
  }

  export function reset(): void {
    bPreviewer?.clear()
    gltfB = null
    gltfBFileName = ''
    bClipNames = []
    bSelectedClipIndex = 0
    bClipInfo = '애니메이션 없음'
  }

  onMount(() => {
    if (bPreviewHost) {
      bPreviewer = new ClipPreviewer(bPreviewHost)
      bPreviewer.setLoop(loop)
    }
  })

  onDestroy(() => {
    bPreviewer?.destroy()
  })

  $effect(() => {
    bPreviewer?.setLoop(loop)
  })
</script>

<section class="merge-panel">
  <div class="merge-top">
    <div class="merge-top-left">
      <div class="merge-header">
        <h2>애니메이션 병합</h2>
        <label class="btn file">
          GLB 열기
          <input type="file" accept=".glb,.gltf" onchange={onLoadBFile} />
        </label>
        <button class="btn primary" onclick={handleMerge} disabled={!canMerge || isMerging}>
          {isMerging ? '병합 중...' : '병합 실행'}
        </button>
        <button class="btn ghost" onclick={onUndo} disabled={!canUndo}>
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

<style>
  h2 {
    margin: 0;
    font-size: 15px;
  }

  .small {
    color: #9ca3af;
    font-size: 12px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
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
</style>
