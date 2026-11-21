<script lang="ts">
  import { locale, t } from '../i18n'
  import { getItemDef, itemDisplayName } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import { inventoryStore } from '../stores/inventoryStore'
  import {
    FALLBACK_ICON,
    dragMeta,
    pointInRect,
    startDrag,
  } from '../stores/dragStore'
  import { watchRack } from '../stores/watchDutyStore'
  import { itemTooltip } from '../actions/itemTooltip'

  const SLOTS = 4

  const rack = $derived($watchRack)
  /** A lent piece is being dragged from the bag or the body: it can land here. */
  const returning = $derived(
    $dragMeta !== null &&
      !('skill' in $dragMeta) &&
      $dragMeta.source.type !== 'rack' &&
      !!rack?.owed.includes($dragMeta.defId)
  )
  const close = () => watchRack.set(null)

  $effect(() => {
    if (rack) return mountOverlay('watchRack', close)
  })

  /** Lent pieces the player holds, in the bag or worn. */
  const held = $derived(
    new Set(
      [...$inventoryStore.bag, ...Object.values($inventoryStore.equipped)].map(
        (item) => item?.item_def_id
      )
    )
  )
  const missing = $derived(rack ? rack.owed.filter((id) => !held.has(id)) : [])

  function move(itemDefId: string, take: boolean, equip: boolean) {
    if (rack)
      networkManager.sendWatchRackMove(rack.objectId, itemDefId, take, equip)
  }

  /** Drag a piece off the rack: onto the character to wear it, into the bag to carry it. */
  function dragOff(e: PointerEvent, itemDefId: string) {
    if (e.button !== 0) return
    e.preventDefault()
    const def = getItemDef(itemDefId)
    startDrag(
      e,
      {
        instanceId: 0,
        defId: itemDefId,
        enchant: 0,
        equipSlot: def?.equipSlot ?? null,
        source: { type: 'rack' },
        icon: def?.icon ?? FALLBACK_ICON,
      },
      (x, y) => {
        const over = (selector: string) =>
          [...document.querySelectorAll<HTMLElement>(selector)].some((el) =>
            pointInRect(x, y, el.getBoundingClientRect())
          )
        if (over('.character-panel')) move(itemDefId, true, true)
        else if (over('[data-panel="inventory"]')) move(itemDefId, true, false)
      }
    )
  }
</script>

{#if rack}
  {@const cells = [
    ...rack.onRack,
    ...Array<null>(Math.max(0, SLOTS - rack.onRack.length)).fill(null),
  ]}
  <div
    class="watch-rack"
    class:drop-target={returning}
    role="dialog"
    aria-label={$t('watch.rack.title')}
    data-watch-rack
  >
    <header class="panel-header">
      <span class="panel-title">{$t('watch.rack.title')}</span>
      <button class="close-btn" onclick={close} aria-label="Close">×</button>
    </header>

    <div class="rack-grid">
      {#each cells as id, i (i)}
        {@const def = id ? getItemDef(id) : null}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="grid-cell"
          use:itemTooltip={def ? { def, side: 'right' } : null}
          ondblclick={() => {
            if (id) move(id, true, true)
          }}
          onpointerdown={(e: PointerEvent) => {
            if (id) dragOff(e, id)
          }}
        >
          {#if def}
            <img
              class="item-icon"
              src="/items/{def.icon}"
              alt={itemDisplayName(def.id, 0, $locale)}
              draggable="false"
            />
          {/if}
        </div>
      {/each}
    </div>

    <p class="hint">
      {$t(
        rack.owed.length === 0
          ? 'watch.rack.takeHint'
          : rack.returnable
            ? 'watch.rack.putHint'
            : 'watch.rack.keepHint'
      )}
    </p>
    {#if rack.returnable && missing.length > 0}
      <button
        class="settle"
        onclick={() => networkManager.sendWatchRackSettle(rack.objectId)}
      >
        {$t('watch.rack.settleLost')}
      </button>
    {/if}
  </div>
{/if}

<style>
  .watch-rack {
    --slot: 64px;
    position: fixed;
    top: 22%;
    left: 50%;
    transform: translateX(-50%);
    z-index: 45;
    display: flex;
    flex-direction: column;
    width: max-content;
    max-width: calc(100vw - 32px);
    box-sizing: border-box;
    padding: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    backdrop-filter: blur(4px);
    color: #e6edf3;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 12px;
    pointer-events: auto;
    user-select: none;
  }
  .watch-rack.drop-target {
    border-color: rgba(88, 255, 88, 0.5);
    box-shadow: inset 0 0 12px rgba(88, 255, 88, 0.15);
  }
  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
    margin-bottom: 8px;
  }
  .panel-title {
    font-size: 14px;
    font-weight: 700;
    color: #f0c040;
  }
  .close-btn {
    background: none;
    border: none;
    color: #9fb2c3;
    font-size: 18px;
    cursor: pointer;
    padding: 0 2px;
    line-height: 1;
  }
  .close-btn:hover {
    color: #fff;
  }
  .rack-grid {
    display: grid;
    grid-template-columns: repeat(4, var(--slot));
    grid-auto-rows: var(--slot);
    gap: 6px;
  }
  .grid-cell {
    position: relative;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    touch-action: none;
  }
  .item-icon {
    position: absolute;
    inset: 0;
    margin: auto;
    width: 90%;
    height: 90%;
    object-fit: contain;
    image-rendering: pixelated;
  }
  .hint {
    width: 0;
    min-width: 100%;
    margin: 8px 0 0;
    color: #9fb2c3;
    line-height: 1.45;
  }
  .settle {
    margin-top: 8px;
    padding: 7px 10px;
    border: 1px solid rgba(240, 192, 64, 0.6);
    border-radius: 6px;
    background: rgba(240, 192, 64, 0.12);
    color: #f0c040;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }
</style>
