<script lang="ts">
  import { get } from 'svelte/store'
  import { shopSession } from '../stores/tradeStore'
  import { gameStore } from '../stores/gameStore'
  import { remotePlayerManager } from '../managers/remotePlayerManager'
  import { inventoryStore, playerGold } from '../stores/inventoryStore'
  import type { ItemInstance } from '../stores/inventoryStore'
  import { getItemDef, type ItemDefinition } from '../data/itemDefs'
  import { getMerchantByNpcName } from '../data/merchantDefs'
  import { MAX_TRADE_DISTANCE_METERS } from '../data/tradeConstants'
  import GoldAmount from './GoldAmount.svelte'
  import { networkManager } from '../network/socket'

  const session = $derived($shopSession)

  interface CartEntry {
    kind: 'buy' | 'sell'
    itemDefId: string
    /** Bag instance backing a sell entry; absent for buy entries. */
    instanceId?: number
    qty: number
    /** Per-unit price, fixed when the entry is added (prices cannot change
     *  within a shop session). */
    unitPrice: number
  }

  let cart = $state<CartEntry[]>([])
  let portraitFailed = $state(false)

  // Reset the cart whenever the shop session changes (open/close/refresh).
  $effect(() => {
    void session?.merchantPlayerId
    cart = []
    portraitFailed = false
  })

  const portraitSrc = $derived.by(() => {
    if (!session) return null
    const def = getMerchantByNpcName(session.merchantName)
    return def ? `/portraits/${def.id}.png` : null
  })

  // The server rejects trades beyond MAX_TRADE_DISTANCE_METERS; close the
  // window at the same range so the player isn't left with a shop that only
  // errors.
  $effect(() => {
    if (!session) return
    const merchantId = session.merchantPlayerId
    const timer = setInterval(() => {
      const me = get(gameStore).currentPlayer
      const merchant = remotePlayerManager.players.get(merchantId)
      if (!me || !merchant) {
        shopSession.set(null)
        return
      }
      const dx = me.position.x - merchant.position.x
      const dz = me.position.z - merchant.position.z
      if (dx * dx + dz * dz > MAX_TRADE_DISTANCE_METERS ** 2) {
        shopSession.set(null)
      }
    }, 300)
    return () => clearInterval(timer)
  })

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

  const buyTotal = $derived(
    cart.reduce((sum, e) => (e.kind === 'buy' ? sum + e.unitPrice * e.qty : sum), 0)
  )
  const sellTotal = $derived(
    cart.reduce((sum, e) => (e.kind === 'sell' ? sum + e.unitPrice * e.qty : sum), 0)
  )
  /** Net gold the player must pay; negative means the player earns gold. */
  const netCost = $derived(buyTotal - sellTotal)
  const canConfirm = $derived(cart.length > 0 && netCost <= $playerGold)

  const findSellEntry = (instanceId: number) =>
    cart.find((e) => e.kind === 'sell' && e.instanceId === instanceId)

  function addBuy(itemDefId: string, def: ItemDefinition) {
    const existing = cart.find((e) => e.kind === 'buy' && e.itemDefId === itemDefId)
    if (existing) {
      existing.qty += 1
    } else {
      cart.push({ kind: 'buy', itemDefId, qty: 1, unitPrice: def.basePrice ?? 0 })
    }
  }

  function addSell(item: ItemInstance, def: ItemDefinition) {
    const existing = findSellEntry(item.instance_id)
    if (existing) {
      if (existing.qty < item.quantity) existing.qty += 1
    } else {
      cart.push({
        kind: 'sell',
        itemDefId: item.item_def_id,
        instanceId: item.instance_id,
        qty: 1,
        unitPrice: sellPrice(def),
      })
    }
  }

  function removeOne(entry: CartEntry) {
    entry.qty -= 1
    if (entry.qty <= 0) {
      cart = cart.filter((e) => e !== entry)
    }
  }

  /** Units of this bag item already reserved in the cart. */
  function reservedQty(instanceId: number): number {
    return findSellEntry(instanceId)?.qty ?? 0
  }

  function onConfirm() {
    if (!session || !canConfirm) return
    // Sells go first so their proceeds can fund the buys.
    for (const entry of cart) {
      if (entry.kind !== 'sell' || entry.instanceId === undefined) continue
      const owned = $inventoryStore.bag.find(
        (i) => i.instance_id === entry.instanceId
      )
      const qty = Math.min(entry.qty, owned?.quantity ?? 0)
      for (let i = 0; i < qty; i++) {
        networkManager.sendSellItem(session.merchantPlayerId, entry.instanceId)
      }
    }
    for (const entry of cart) {
      if (entry.kind !== 'buy') continue
      for (let i = 0; i < entry.qty; i++) {
        networkManager.sendBuyItem(session.merchantPlayerId, entry.itemDefId)
      }
    }
    cart = []
  }
</script>

{#if session}
  <div class="trade-window" role="dialog" aria-label="Trade" data-panel="trade">
    {#if portraitSrc && !portraitFailed}
      <img
        class="merchant-portrait"
        src={portraitSrc}
        alt={session.merchantName}
        draggable="false"
        onerror={() => (portraitFailed = true)}
      />
    {/if}
    <div class="panel-header">
      <span class="panel-title">{session.merchantName}'s Shop</span>
      <button class="close-btn" onclick={() => shopSession.set(null)}>&times;</button>
    </div>

    <div class="trade-columns">
      <div class="trade-column">
        <div class="column-title">Buy</div>
        <div class="item-list">
          {#each session.catalog as itemDefId (itemDefId)}
            {@const def = getItemDef(itemDefId)}
            {#if def}
              <button class="item-row" onclick={() => addBuy(itemDefId, def)}>
                <img class="item-icon" src="/items/{def.icon}" alt="" draggable="false" />
                <span class="item-name">{def.name}</span>
                <span class="item-price"><GoldAmount copper={def.basePrice ?? 0} /></span>
              </button>
            {/if}
          {/each}
        </div>
      </div>

      <div class="trade-column cart-column">
        <div class="cart-line cart-current">
          <span class="cart-label">Current</span>
          <GoldAmount copper={$playerGold} />
        </div>
        <div class="column-title">Cart</div>
        <div class="item-list">
          {#each cart as entry (entry.kind + ':' + (entry.instanceId ?? entry.itemDefId))}
            {@const def = getItemDef(entry.itemDefId)}
            {#if def}
              <button class="item-row" onclick={() => removeOne(entry)}>
                <span class="cart-kind {entry.kind}">
                  {entry.kind === 'buy' ? 'B' : 'S'}
                </span>
                <img class="item-icon" src="/items/{def.icon}" alt="" draggable="false" />
                <span class="item-name">
                  {def.name}{entry.qty > 1 ? ` ×${entry.qty}` : ''}
                </span>
                <span class="item-price {entry.kind}">
                  {entry.kind === 'buy' ? '−' : '+'}<GoldAmount
                    copper={entry.unitPrice * entry.qty}
                  />
                </span>
              </button>
            {/if}
          {:else}
            <div class="empty-note">Click items to add</div>
          {/each}
        </div>
        <div class="cart-footer">
          <div class="cart-line">
            <span class="cart-label">Total</span>
            <span class="cart-total" class:earn={netCost < 0}>
              {netCost === 0 ? '' : netCost < 0 ? '+' : '−'}<GoldAmount
                copper={Math.abs(netCost)}
              />
            </span>
          </div>
          <div class="cart-line">
            <span class="cart-label">After</span>
            <GoldAmount copper={$playerGold - netCost} />
          </div>
          <button class="confirm-btn" disabled={!canConfirm} onclick={onConfirm}>
            Confirm
          </button>
        </div>
      </div>

      <div class="trade-column">
        <div class="column-title">Sell ({session.sellRatePercent}%)</div>
        <div class="item-list">
          {#each sellEntries as { item, def } (item.instance_id)}
            {@const reserved = reservedQty(item.instance_id)}
            <button
              class="item-row"
              disabled={reserved >= item.quantity}
              onclick={() => addSell(item, def)}
            >
              <img class="item-icon" src="/items/{def.icon}" alt="" draggable="false" />
              <span class="item-name">
                {def.name}{item.quantity > 1 ? ` ×${item.quantity}` : ''}
              </span>
              <span class="item-price"><GoldAmount copper={sellPrice(def)} /></span>
            </button>
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

  .merchant-portrait {
    position: absolute;
    left: 0;
    bottom: 100%;
    width: 160px;
    pointer-events: none;
    user-select: none;
    filter: drop-shadow(0 4px 8px rgba(0, 0, 0, 0.5));
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
    gap: 16px;
    overflow: hidden;
  }

  .trade-column {
    display: flex;
    flex-direction: column;
    width: 230px;
    min-width: 0;
  }

  .cart-column {
    width: 210px;
    padding: 0 10px;
    border-left: 1px solid rgba(255, 255, 255, 0.12);
    border-right: 1px solid rgba(255, 255, 255, 0.12);
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
    scrollbar-width: none;
  }

  .item-list::-webkit-scrollbar {
    display: none;
  }

  .item-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    background: none;
    color: inherit;
    font-family: inherit;
    font-size: inherit;
    text-align: left;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 150ms ease, border-color 150ms ease;
  }

  .item-row:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.3);
  }

  .item-row:disabled {
    opacity: 0.4;
    cursor: default;
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

  .item-price.buy {
    color: #ff9a8a;
  }

  .item-price.sell {
    color: #8ae29a;
  }

  .cart-kind {
    flex-shrink: 0;
    width: 14px;
    font-weight: 700;
    text-align: center;
  }

  .cart-kind.buy {
    color: #ff9a8a;
  }

  .cart-kind.sell {
    color: #8ae29a;
  }

  .cart-current {
    padding-bottom: 4px;
    margin-bottom: 4px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
  }

  .cart-footer {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.15);
  }

  .cart-line {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .cart-label {
    color: #9fb2c3;
    font-weight: 700;
  }

  .cart-total {
    font-weight: 700;
    color: #ff9a8a;
  }

  .cart-total.earn {
    color: #8ae29a;
  }

  .confirm-btn {
    margin-top: 4px;
    background: rgba(60, 90, 60, 0.85);
    color: #d6f0d6;
    border: 1px solid rgba(140, 220, 140, 0.35);
    border-radius: 4px;
    padding: 4px 14px;
    font-family: inherit;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    transition: background 150ms ease, color 150ms ease;
  }

  .confirm-btn:hover:not(:disabled) {
    background: rgba(80, 120, 80, 0.95);
    color: #fff;
  }

  .confirm-btn:disabled {
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

    .merchant-portrait {
      display: none;
    }

    .trade-columns {
      gap: 10px;
    }

    .trade-column {
      width: 170px;
    }

    .cart-column {
      width: 165px;
      padding: 0 6px;
    }

    .item-row {
      min-height: 36px;
    }

    .confirm-btn {
      min-height: 30px;
    }

    .close-btn {
      min-width: 32px;
      min-height: 32px;
      font-size: 22px;
    }
  }
</style>
