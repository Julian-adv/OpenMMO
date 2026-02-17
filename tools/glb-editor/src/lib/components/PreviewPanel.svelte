<script lang="ts">
  import AnimationClipControls from './AnimationClipControls.svelte'

  interface Props {
    clips: string[]
    selectedClipIndex: number
    clipInfo: string
    emptyLabel?: string
    dropzoneText?: string
    ariaLabel?: string
    dropActive?: boolean
    onClipChange?: (index: number) => void
    onPlay?: () => void
    onPause?: () => void
    onDragEnter?: (event: DragEvent) => void
    onDragOver?: (event: DragEvent) => void
    onDragLeave?: (event: DragEvent) => void
    onDrop?: (event: DragEvent) => void
    bindHost?: (el: HTMLDivElement) => void
  }

  let {
    clips,
    selectedClipIndex,
    clipInfo,
    emptyLabel = '애니메이션 없음',
    dropzoneText = '여기에 GLB 파일을 드래그 앤 드롭',
    ariaLabel = 'GLB preview',
    dropActive = false,
    onClipChange,
    onPlay,
    onPause,
    onDragEnter,
    onDragOver,
    onDragLeave,
    onDrop,
    bindHost,
  }: Props = $props()

  let hostEl = $state<HTMLDivElement | null>(null)

  $effect(() => {
    if (hostEl) bindHost?.(hostEl)
  })
</script>

<div
  class="preview-wrap"
  role="region"
  aria-label={ariaLabel}
  ondragenter={onDragEnter ?? onDragOver}
  ondragover={onDragOver}
  ondragleave={onDragLeave}
  ondrop={onDrop}
>
  <div class="preview-overlay">
    <AnimationClipControls
      {clips}
      selectedIndex={selectedClipIndex}
      info={clipInfo}
      onChange={onClipChange}
      {onPlay}
      {onPause}
      {emptyLabel}
    />
  </div>
  <div class="dropzone" class:active={dropActive}>{dropzoneText}</div>
  <div class="preview-host" bind:this={hostEl} role="region" aria-label="{ariaLabel} canvas"></div>
</div>

<style>
  .preview-wrap {
    position: relative;
    background: #090b12;
    overflow: hidden;
    width: 100%;
    height: 100%;
    min-height: 0;
  }

  .preview-overlay {
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

  .preview-host {
    width: 100%;
    height: 100%;
  }
</style>
