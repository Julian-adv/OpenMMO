<script lang="ts">
  type Variant = 'inline' | 'panel'

  interface Props {
    clips: string[]
    selectedIndex: number
    info: string
    onChange?: (index: number) => void
    onPlay?: () => void
    onPause?: () => void
    emptyLabel?: string
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
    emptyLabel = '애니메이션 없음',
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
      <option value="0">{emptyLabel}</option>
    {:else}
      {#each clips as clip, index (index)}
        <option value={String(index)}>{clip}</option>
      {/each}
    {/if}
  </select>

  <button class="btn" onclick={() => onPlay?.()} disabled={!hasClip}>재생</button>
  <button class="btn" onclick={() => onPause?.()} disabled={!hasClip}>일시정지</button>
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
    grid-template-columns: 1fr auto auto;
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
