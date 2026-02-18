<script lang="ts">
  type Variant = 'inline' | 'panel'

  const EMPTY_LABEL = '애니메이션 없음'

  interface Props {
    clips: string[]
    selectedIndex: number
    info: string
    onChange?: (index: number) => void
    onPlay?: () => void
    onPause?: () => void
    onDelete?: () => void
    className?: string
    variant?: Variant
  }

  let {
    clips,
    selectedIndex,
    info,
    onChange,
    onPlay,
    onPause,
    onDelete,
    className = '',
    variant = 'inline',
  }: Props = $props()

  const hasClip = $derived(clips.length > 0)

  function handleChange(event: Event): void {
    const select = event.currentTarget as HTMLSelectElement
    const next = Number.parseInt(select.value, 10)
    onChange?.(Number.isNaN(next) ? 0 : next)
  }
</script>

<div class={`clip-controls ${variant} ${className}`.trim()}>
  <select value={String(selectedIndex)} onchange={handleChange} disabled={!hasClip}>
    {#if !hasClip}
      <option value="0">{EMPTY_LABEL}</option>
    {:else}
      {#each clips as clip, index (index)}
        <option value={String(index)}>{clip}</option>
      {/each}
    {/if}
  </select>

  <button class="btn" onclick={() => onPlay?.()} disabled={!hasClip}>재생</button>
  <button class="btn" onclick={() => onPause?.()} disabled={!hasClip}>일시정지</button>
  {#if onDelete}
    <button class="btn danger" onclick={() => onDelete?.()} disabled={!hasClip}>삭제</button>
  {/if}
  <span class="info">{info}</span>
</div>

<style>
  .clip-controls {
    gap: 8px;
    align-items: center;
  }

  .clip-controls.inline {
    display: flex;
    flex-wrap: wrap;
  }

  .clip-controls.panel {
    display: grid;
    grid-template-columns: 1fr auto auto auto;
  }

  select {
    min-width: 180px;
    min-height: 31px;
  }

  .btn {
    background: #1f2635;
    border: 1px solid #0a0d14;
    color: #e5e7eb;
    border-radius: 8px;
    padding: 7px 10px;
    cursor: pointer;
    min-height: 31px;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn.danger {
    background: #7a1e1e;
    border-color: #4a0f0f;
  }

  .btn.danger:hover:not(:disabled) {
    background: #9a2525;
  }

  .info {
    color: #9ca3af;
    font-size: 12px;
  }

  .panel .info {
    grid-column: 1 / -1;
  }

  @media (width <= 900px) {
    .clip-controls.panel {
      grid-template-columns: 1fr;
    }

    .clip-controls.inline {
      max-width: calc(100% - 20px);
    }
  }
</style>
