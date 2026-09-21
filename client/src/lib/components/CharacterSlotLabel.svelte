<script lang="ts">
  import type { AccountCharacter } from '../network/socket'
  import type { CharacterSlotLayout } from '../utils/characterSelectLayout'
  import { t } from '../i18n'
  import CharacterSummary from './CharacterSummary.svelte'

  interface Props {
    character: AccountCharacter | undefined
    selected: boolean
    layout: CharacterSlotLayout
    availableHeight: number
    onclick: () => void
    ondblclick: () => void
    compact?: boolean
    disabled?: boolean
  }

  let {
    character,
    selected,
    layout,
    availableHeight,
    onclick,
    ondblclick,
    compact = false,
    disabled = false,
  }: Props = $props()

  let height = $state(0)
  const top = $derived(
    Math.max(0, Math.min(layout.y - height / 2, availableHeight - height))
  )
  let press: { x: number; y: number } | undefined
  let dragged = false
  let previousDragged = false

  function handlePointerDown(event: PointerEvent) {
    if (event.button !== 0) return
    previousDragged = dragged
    dragged = false
    press = { x: event.clientX, y: event.clientY }
    ;(event.currentTarget as HTMLButtonElement).setPointerCapture(
      event.pointerId
    )
  }

  function handlePointerMove(event: PointerEvent) {
    if (!press) return
    if (Math.hypot(event.clientX - press.x, event.clientY - press.y) > 5) {
      dragged = true
    }
  }
</script>

<button
  type="button"
  class="character-slot"
  class:selected
  class:compact
  class:empty={!character}
  aria-pressed={character ? selected : undefined}
  {disabled}
  bind:clientHeight={height}
  style:left={`${layout.x}px`}
  style:top={`${top}px`}
  style:width={`${layout.width}px`}
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={() => (press = undefined)}
  onpointercancel={() => {
    press = undefined
    dragged = true
  }}
  onclick={(event) => {
    if (event.detail === 0 || !dragged) onclick()
  }}
  ondblclick={() => {
    if (!dragged && !previousDragged) ondblclick()
  }}
>
  {#if character}
    <CharacterSummary {character} {compact} />
  {:else}
    <span>{$t('characterSelect.create')}</span>
  {/if}
</button>

<style>
  .character-slot {
    position: absolute;
    transform: translateX(-50%);
    box-sizing: border-box;
    border: 1px solid #53657b;
    border-radius: 9px;
    padding: 12px;
    background: rgba(20, 30, 44, 0.85);
    color: #a7b7ca;
    font: inherit;
    cursor: pointer;
    pointer-events: auto;
    touch-action: manipulation;
    transition:
      background 120ms,
      border-color 120ms;
  }

  .character-slot.selected {
    border-color: #7cc9ff;
    background: rgba(34, 53, 82, 0.92);
    box-shadow: 0 0 0 1px #7cc9ff;
  }

  .character-slot:hover:not(:disabled) {
    border-color: #a0d8ff;
    background: rgba(34, 53, 82, 0.96);
  }

  .character-slot:focus-visible {
    outline: 2px solid #d6edff;
    outline-offset: 3px;
  }

  .character-slot:disabled {
    cursor: default;
    opacity: 0.6;
  }

  .compact,
  .empty {
    padding: 10px 8px;
    font-size: 14px;
    min-height: 44px;
  }
</style>
