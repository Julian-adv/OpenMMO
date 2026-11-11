<script lang="ts">
  interface Props {
    id: string
    text: string
    anchor: DOMRect
  }

  let { id, text, anchor }: Props = $props()
  const gap = 6
  const viewportWidth = window.innerWidth
  const viewportHeight = window.innerHeight
  let width = $state(0)
  let height = $state(0)

  const left = $derived(
    Math.max(
      gap,
      Math.min(
        anchor.left + anchor.width / 2 - width / 2,
        viewportWidth - width - gap
      )
    )
  )
  /** Below the anchor, or above it when the bottom has no room. */
  const top = $derived(
    anchor.bottom + gap + height <= viewportHeight
      ? anchor.bottom + gap
      : Math.max(gap, anchor.top - gap - height)
  )
</script>

<div
  {id}
  role="tooltip"
  class="hint-tooltip"
  style="left: {left}px; top: {top}px;"
  bind:clientWidth={width}
  bind:clientHeight={height}
>
  {text}
</div>

<style>
  .hint-tooltip {
    position: fixed;
    box-sizing: border-box;
    max-width: 260px;
    padding: 6px 8px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 6px;
    background: rgba(6, 10, 14, 0.92);
    color: #c8d6e0;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 12px;
    line-height: 1.45;
    white-space: pre-line;
    pointer-events: none;
    user-select: none;
    z-index: 1200;
  }
</style>
