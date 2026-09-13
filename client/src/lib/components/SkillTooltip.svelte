<script lang="ts">
  import type { SkillTooltipParams } from '../actions/skillTooltip'

  interface Props extends SkillTooltipParams {
    id: string
    anchor: DOMRect
  }

  let {
    id,
    name,
    description,
    stats = [],
    details = [],
    anchor,
  }: Props = $props()
  const gap = 8
  const viewportWidth = window.innerWidth
  const viewportHeight = window.innerHeight
  const width = $derived(
    Math.min(stats.length ? 240 : 178, viewportWidth - gap * 2)
  )
  let height = $state(0)

  const left = $derived(
    Math.max(
      gap,
      Math.min(
        anchor.right + gap + width <= viewportWidth
          ? anchor.right + gap
          : anchor.left - width - gap,
        viewportWidth - width - gap
      )
    )
  )
  const top = $derived(
    Math.max(gap, Math.min(anchor.top, viewportHeight - height - gap))
  )
</script>

<div
  {id}
  role="tooltip"
  class="skill-tooltip"
  style="left: {left}px; top: {top}px; width: {width}px;"
  bind:clientHeight={height}
>
  <div class="name">{name}</div>
  <div class="description">{description}</div>
  {#if stats.length}
    <dl class="stats">
      {#each stats as stat (stat.label)}
        <div>
          <dt>{stat.label}</dt>
          <dd>{stat.value}</dd>
        </div>
      {/each}
    </dl>
  {/if}
  {#if details.length}
    <div class="details">
      {#each details as detail (detail)}<span>{detail}</span>{/each}
    </div>
  {/if}
</div>

<style>
  .skill-tooltip {
    position: fixed;
    box-sizing: border-box;
    padding: 8px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 6px;
    background: rgba(6, 10, 14, 0.9);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 13px;
    line-height: 1.4;
    pointer-events: none;
    user-select: none;
    z-index: 1200;
  }

  .name {
    margin-bottom: 4px;
    color: #f0c040;
    font-size: 15px;
    font-weight: 700;
  }

  .description {
    margin-bottom: 6px;
    color: #9fb2c3;
  }

  .details {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid rgba(255, 255, 255, 0.15);
    color: #9fb2c3;
  }

  .description:last-child {
    margin-bottom: 0;
  }

  .stats {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin: 0;
    color: #c8d6e0;
  }

  .stats > div {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 12px;
    align-items: baseline;
  }

  dt {
    color: #9fb2c3;
  }

  dd {
    margin: 0;
    text-align: right;
    overflow-wrap: anywhere;
  }
</style>
