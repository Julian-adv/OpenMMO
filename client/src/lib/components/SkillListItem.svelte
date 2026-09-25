<script lang="ts">
  import { onDestroy } from 'svelte'
  import { t } from '../i18n'
  import {
    skillTooltip,
    type SkillTooltipParams,
  } from '../actions/skillTooltip'
  import { dragMeta, startDrag } from '../stores/dragStore'

  interface Props extends SkillTooltipParams {
    id: string
    icon: string
    manaCost: number
    onUse: () => void
  }

  let { id, name, description, stats, details, icon, manaCost, onUse }: Props =
    $props()
  let cancelDrag: (() => void) | undefined
  let consecutiveClicks = 0
  onDestroy(() => cancelDrag?.())
</script>

<button
  type="button"
  class="skill-row"
  class:dragging={$dragMeta && 'skill' in $dragMeta && $dragMeta.skill === id}
  aria-label={name}
  draggable="false"
  use:skillTooltip={{ name, description, stats, details }}
  ondblclick={(event) => {
    if (event.button !== 0 || consecutiveClicks < 2) return
    consecutiveClicks = 0
    onUse()
  }}
  onclick={(event) => {
    if (event.detail !== 0) return
    consecutiveClicks = 0
    onUse()
  }}
  onpointerdown={(event) => {
    if (event.button !== 0 || !event.isPrimary) return
    event.preventDefault()
    cancelDrag?.()
    const previousClicks = consecutiveClicks
    consecutiveClicks = 0
    cancelDrag = startDrag(
      event,
      { skill: id, source: { type: 'skill' }, icon },
      undefined,
      () => {
        consecutiveClicks = previousClicks + 1
      }
    )
  }}
>
  <span class="skill-icon">
    <img src={icon} alt="" draggable="false" />
  </span>
  <span class="skill-info">
    <span class="skill-name">{name}</span>
    <span class="skill-description">
      {description}
      {#if manaCost > 0}
        <span class="skill-cost">
          ({$t('skill.manaCost', { amount: manaCost })})
        </span>
      {/if}
    </span>
  </span>
</button>

<style>
  .skill-row {
    display: grid;
    grid-template-columns: 48px minmax(0, 1fr);
    align-items: center;
    gap: 10px;
    box-sizing: border-box;
    width: 100%;
    padding: 6px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.04);
    color: inherit;
    font: inherit;
    text-align: left;
    touch-action: none;
    user-select: none;
    cursor: grab;
  }

  .skill-row:hover,
  .skill-row:focus-visible {
    border-color: rgba(240, 192, 64, 0.7);
    outline: 1px solid rgba(240, 192, 64, 0.3);
    outline-offset: -2px;
  }

  .dragging {
    cursor: grabbing;
  }

  .skill-icon {
    box-sizing: border-box;
    width: 48px;
    height: 48px;
    overflow: hidden;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    background: rgba(6, 10, 14, 0.55);
  }

  .skill-info {
    min-width: 0;
  }

  .skill-name {
    display: block;
    margin-bottom: 4px;
    color: #f0c040;
    font-size: 13px;
    font-weight: 700;
  }

  .skill-description {
    display: block;
    color: #9fb2c3;
    font-size: 11px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }

  .skill-cost {
    color: #60a5fa;
    white-space: nowrap;
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    image-rendering: auto;
    pointer-events: none;
    user-select: none;
  }
</style>
