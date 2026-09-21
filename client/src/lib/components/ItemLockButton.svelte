<script lang="ts">
  import { t, locale } from '../i18n'
  import { itemDisplayName } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import type { ItemInstance } from '../stores/inventoryStore'
  import ItemUnlockDialog from './ItemUnlockDialog.svelte'

  let {
    item,
    getInstanceIds,
  }: { item: ItemInstance; getInstanceIds?: () => number[] } = $props()
  const label = $derived(
    $t(item.locked ? 'inventory.unlockItem' : 'inventory.lockItem', {
      item: itemDisplayName(item.item_def_id, item.enchant, $locale),
    })
  )
  let unlockTarget = $state<{
    item: ItemInstance
    instanceIds: number[]
  } | null>(null)

  $effect(() => {
    if (
      unlockTarget &&
      (!item.locked || item.instance_id !== unlockTarget.item.instance_id)
    )
      unlockTarget = null
  })

  function toggle() {
    const targets = getInstanceIds?.() ?? [item.instance_id]
    if (item.locked) {
      unlockTarget = { item: { ...item }, instanceIds: [...targets] }
      return
    }
    for (const instanceId of targets) {
      networkManager.sendSetItemLocked(instanceId, true)
    }
  }

  function confirmUnlock() {
    const target = unlockTarget
    unlockTarget = null
    if (!target || !item.locked || item.instance_id !== target.item.instance_id)
      return
    for (const instanceId of target.instanceIds) {
      networkManager.sendSetItemLocked(instanceId, false)
    }
  }
</script>

<button
  class="item-lock-button"
  class:locked={item.locked}
  aria-label={label}
  aria-pressed={item.locked ?? false}
  title={label}
  onpointerdown={(event) => event.stopPropagation()}
  ondblclick={(event) => event.stopPropagation()}
  onclick={(event) => {
    event.stopPropagation()
    toggle()
  }}
>
  <svg
    width="14"
    height="14"
    viewBox="0 0 16 16"
    fill="none"
    aria-hidden="true"
  >
    <path
      d={item.locked ? 'M5 7V4a3 3 0 0 1 6 0v3' : 'M5 7V4a3 3 0 0 1 6 0'}
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
    />
    <rect
      x="3.5"
      y="7"
      width="9"
      height="7"
      rx="1.5"
      stroke="currentColor"
      stroke-width="1.5"
    />
    <path
      d="M8 10v1"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
    />
  </svg>
</button>

{#if unlockTarget}
  <ItemUnlockDialog
    item={unlockTarget.item}
    onConfirm={confirmUnlock}
    onCancel={() => (unlockTarget = null)}
  />
{/if}

<style>
  .item-lock-button {
    position: absolute;
    top: 2px;
    right: 2px;
    z-index: 2;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 4px;
    background: rgba(6, 10, 14, 0.8);
    color: #9fb2c3;
    opacity: 0.55;
    cursor: pointer;
    pointer-events: auto;
  }

  .item-lock-button.locked {
    color: #f0c040;
    opacity: 1;
  }

  .item-lock-button:hover,
  .item-lock-button:focus-visible {
    border-color: currentColor;
    opacity: 1;
  }
</style>
