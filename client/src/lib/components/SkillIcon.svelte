<script lang="ts">
  import { onDestroy } from 'svelte'
  import {
    skillTooltip,
    type SkillTooltipParams,
  } from '../actions/skillTooltip'
  import { dragMeta, startDrag, type SkillDragMeta } from '../stores/dragStore'

  interface Props extends SkillTooltipParams {
    icon: string
    quickslotSkill?: SkillDragMeta['skill']
  }

  let { name, description, stats, details, icon, quickslotSkill }: Props =
    $props()
  let cancelDrag: (() => void) | undefined
  onDestroy(() => cancelDrag?.())
</script>

<button
  type="button"
  class="skill-icon"
  class:assignable={quickslotSkill !== undefined}
  class:dragging={$dragMeta &&
    'skill' in $dragMeta &&
    $dragMeta.skill === quickslotSkill}
  aria-label={name}
  draggable="false"
  use:skillTooltip={{ name, description, stats, details }}
  onpointerdown={(event) => {
    if (event.button !== 0 || !event.isPrimary || !quickslotSkill) return
    event.preventDefault()
    cancelDrag?.()
    cancelDrag = startDrag(event, {
      skill: quickslotSkill,
      source: { type: 'skill' },
      icon,
    })
  }}
>
  <img src={icon} alt="" draggable="false" />
</button>

<style>
  .skill-icon {
    position: relative;
    box-sizing: border-box;
    width: 100%;
    aspect-ratio: 1;
    padding: 0;
    overflow: hidden;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    background: rgba(6, 10, 14, 0.55);
    cursor: default;
  }

  .skill-icon:hover,
  .skill-icon:focus-visible {
    border-color: rgba(240, 192, 64, 0.7);
    outline: 1px solid rgba(240, 192, 64, 0.3);
    outline-offset: -2px;
  }

  .assignable {
    touch-action: none;
    cursor: grab;
  }

  .dragging {
    cursor: grabbing;
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
