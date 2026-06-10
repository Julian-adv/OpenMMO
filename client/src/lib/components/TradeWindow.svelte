<script lang="ts">
  import { shopSession } from '../stores/tradeStore'
  import { inventoryStore, playerGold } from '../stores/inventoryStore'
  import type { ItemInstance } from '../stores/inventoryStore'
  import { getItemDef, type ItemDefinition } from '../data/itemDefs'
  import { formatGold } from '../utils/currency'
  import { networkManager } from '../network/socket'

  const session = $derived($shopSession)

  const sellEntries = $derived.by(() => {
    if (!session) return []
    return $inventoryStore.bag
      .map((item) => ({ item, def: getItemDef(item.item_def_id) }))
      .filter(
        (entry): entry is { item: ItemInstance; def: ItemDefinition } =>
          (entry.def?.basePrice ?? 0) > 0
      )
  })

  function sellPrice(def: ItemDefinition): number {
    if (!session) return 0
    return Math.max(
      1,
      Math.floor(((def.basePrice ?? 0) * session.sellRatePercent) / 100)
    )
  }

  function onBuy(itemDefId: string) {
    if (!session) return
    networkManager.sendBuyItem(session.merchantPlayerId, itemDefId)
  }

  function onSell(instanceId: number) {
    if (!session) return
    networkManager.sendSellItem(session.merchantPlayerId, instanceId)
  }
</script>

{#if session}
  <div class="trade-window" role="dialog" aria-label="Trade" data-panel="trade">
    <div class="panel-header">
      <span class="panel-title">{session.merchantName}'s Shop</span>
      <span class="gold-display">{formatGold($playerGold)}</span>
      <button class="close-btn" onclick={() => shopSession.set(null)}>&times;</button>
    </div>

    <div class="trade-columns">
      <div class="trade-column">
        <div class="column-title">Buy</div>
        <div class="item-list">
          {#each session.catalog as itemDefId (itemDefId)}
            {@const def = getItemDef(itemDefId)}
            {#if def}
              {@const price = def.basePrice ?? 0}
              <div class="item-row">
                <img class="item-icon" src="/items/{def.icon}" alt="" draggable="false" />
                <span class="item-name">{def.name}</span>
                <span class="item-price">{formatGold(price)}</span>
                <button
                  class="trade-btn"
                  disabled={$playerGold < price}
                  onclick={() => onBuy(itemDefId)}
                >
                  Buy
                </button>
              </div>
            {/if}
          {/each}
        </div>
      </div>

      <div class="trade-column">
        <div class="column-title">Sell ({session.sellRatePercent}%)</div>
        <div class="item-list">
          {#each sellEntries as { item, def } (item.instance_id)}
            <div class="item-row">
              <img class="item-icon" src="/items/{def.icon}" alt="" draggable="false" />
              <span class="item-name">
                {def.name}{item.quantity > 1 ? ` ×${item.quantity}` : ''}
              </span>
              <span class="item-price">{formatGold(sellPrice(def))}</span>
              <button class="trade-btn" onclick={() => onSell(item.instance_id)}>
                Sell
              </button>
            </div>
          {:else}
            <div class="empty-note">Nothing to sell</div>
          {/each}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .trade-window {
    position: fixed;
    left: 50%;
    top: 45%;
    transform: translate(-50%, -50%);
    z-index: 45;
    display: flex;
    flex-direction: column;
    backdrop-filter: blur(4px);
    padding: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    pointer-events: auto;
    max-width: calc(100vw - 32px);
    max-height: 70vh;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
    margin-bottom: 8px;
  }

  .panel-title {
    font-size: 14px;
    font-weight: 700;
    color: #f0c040;
  }

  .gold-display {
    font-size: 12px;
    font-weight: 700;
    color: #ffd700;
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

  .trade-columns {
    display: flex;
    gap: 12px;
    overflow: hidden;
  }

  .trade-column {
    display: flex;
    flex-direction: column;
    width: 250px;
    min-width: 0;
  }

  .column-title {
    font-size: 12px;
    font-weight: 700;
    color: #9fb2c3;
    padding-bottom: 4px;
  }

  .item-list {
    overflow-y: auto;
    overscroll-behavior: contain;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 50vh;
  }

  .item-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
  }

  .item-icon {
    width: 28px;
    height: 28px;
    image-rendering: pixelated;
    flex-shrink: 0;
  }

  .item-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-price {
    color: #ffd700;
    flex-shrink: 0;
  }

  .trade-btn {
    flex-shrink: 0;
    background: rgba(60, 60, 60, 0.85);
    color: #ccc;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    padding: 2px 8px;
    font-family: inherit;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    transition: background 150ms ease, color 150ms ease;
  }

  .trade-btn:hover:not(:disabled) {
    background: rgba(80, 80, 80, 0.95);
    color: #fff;
  }

  .trade-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .empty-note {
    color: #6b7d8d;
    padding: 6px 4px;
  }

  @media (max-width: 600px), (pointer: coarse) {
    .trade-window {
      top: 40%;
      max-height: 60vh;
    }

    .trade-column {
      width: 180px;
    }

    .trade-btn {
      min-height: 28px;
    }

    .close-btn {
      min-width: 32px;
      min-height: 32px;
      font-size: 22px;
    }
  }
</style>
