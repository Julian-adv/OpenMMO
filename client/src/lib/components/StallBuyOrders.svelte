<script lang="ts">
  import { locale, t } from '../i18n'
  import {
    displayName,
    getItemDef,
    getTradeableItemDefs,
    itemDisplayName,
  } from '../data/itemDefs'
  import {
    STALL_MAX_LISTINGS,
    STALL_TAX_PERCENT,
    stallTax,
  } from '../data/stallLimits'
  import { inventoryStore, playerGold } from '../stores/inventoryStore'
  import { networkManager } from '../network/socket'
  import { parseGold, formatGold } from '../utils/currency'
  import { itemTooltip } from '../actions/itemTooltip'
  import type { StallBuyOrder, StallState } from '../network/networkTypes'
  import GoldAmount from './GoldAmount.svelte'

  let { stall }: { stall: StallState } = $props()
  let search = $state('')
  let draftItem = $state<string | null>(null)
  let enchant = $state(0)
  let quantity = $state(1)
  let priceText = $state('')
  let selectedOrderId = $state<number | null>(null)
  let selectedInstanceId = $state<number | null>(null)
  let saleQuantity = $state(1)
  const catalogue = getTradeableItemDefs()
  const localizedCatalogue = $derived(
    catalogue
      .map((def) => {
        const name = displayName(def, 0, $locale)
        return {
          def,
          name,
          searchText: `${def.id} ${def.name} ${name}`.toLocaleLowerCase(),
        }
      })
      .sort((a, b) => a.name.localeCompare(b.name))
  )
  const query = $derived(search.trim().toLocaleLowerCase())
  const searchResults = $derived(
    localizedCatalogue.filter((entry) => entry.searchText.includes(query))
  )
  const order = $derived(
    stall.buy_orders.find((entry) => entry.order_id === selectedOrderId)
  )
  const matchingBag = $derived(
    order
      ? $inventoryStore.bag.filter(
          (item) =>
            !item.locked &&
            item.item_def_id === order.item_def_id &&
            item.enchant === order.enchant
        )
      : []
  )
  const saleItem = $derived(
    matchingBag.find((item) => item.instance_id === selectedInstanceId)
  )
  const saleMax = $derived(
    Math.min(order?.quantity ?? 0, saleItem?.quantity ?? 0)
  )
  const saleTotal = $derived((order?.unit_price ?? 0) * saleQuantity)
  const tax = $derived(stallTax(saleTotal))
  const canSell = $derived(
    !!order &&
      !!saleItem &&
      Number.isSafeInteger(saleQuantity) &&
      saleQuantity > 0 &&
      saleQuantity <= saleMax &&
      Number.isSafeInteger(saleTotal)
  )
  const draftDef = $derived(draftItem ? getItemDef(draftItem) : undefined)
  const enchantable = $derived(
    draftDef?.category === 'weapon' || draftDef?.category === 'armor'
  )
  const unitPrice = $derived(parseGold(priceText))
  const replacing = $derived(
    stall.buy_orders.find(
      (entry) => entry.item_def_id === draftItem && entry.enchant === enchant
    )
  )
  const otherCost = $derived(
    stall.buy_orders
      .filter((entry) => entry !== replacing)
      .reduce((sum, entry) => sum + entry.unit_price * entry.quantity, 0)
  )
  const draftCost = $derived((unitPrice ?? 0) * quantity)
  const canSave = $derived(
    !!draftDef &&
      Number.isSafeInteger(quantity) &&
      quantity > 0 &&
      quantity <= 4294967295 &&
      Number.isSafeInteger(enchant) &&
      enchant >= 0 &&
      enchant <= 2147483647 &&
      (enchantable || enchant === 0) &&
      unitPrice !== null &&
      unitPrice >= 0 &&
      Number.isSafeInteger(draftCost) &&
      draftCost + otherCost <= $playerGold &&
      (!!replacing ||
        stall.listings.length + stall.buy_orders.length < STALL_MAX_LISTINGS)
  )

  function draft(itemDefId: string, existing?: StallBuyOrder) {
    draftItem = itemDefId
    enchant = existing?.enchant ?? 0
    quantity = existing?.quantity ?? 1
    priceText = existing ? formatGold(existing.unit_price) : ''
  }

  function chooseOrder(entry: StallBuyOrder) {
    if (stall.owned) draft(entry.item_def_id, entry)
    else {
      selectedOrderId = entry.order_id
      selectedInstanceId = null
      saleQuantity = 1
    }
  }

  function save() {
    if (!canSave || !draftItem || unitPrice === null) return
    networkManager.sendSetStallBuyOrder(draftItem, quantity, enchant, unitPrice)
    draftItem = null
  }

  function sell() {
    if (!canSell || !order || !saleItem) return
    networkManager.sendSellToStall(
      stall.stall_id,
      order.order_id,
      saleItem.instance_id,
      saleQuantity
    )
    selectedInstanceId = null
  }
</script>

<div class="trade-columns">
  <section class="trade-column">
    <div class="column-title">
      {$t('stall.wanted', { count: stall.buy_orders.length })}
    </div>
    <div class="item-list">
      {#each stall.buy_orders as entry (entry.order_id)}
        {@const def = getItemDef(entry.item_def_id)}
        <div class="order-row">
          <button
            class="item-row"
            class:selected={order?.order_id === entry.order_id}
            onclick={() => chooseOrder(entry)}
            use:itemTooltip={def
              ? { def, enchant: entry.enchant, side: 'right' }
              : null}
          >
            {#if def}<img
                class="item-icon"
                src="/items/{def.icon}"
                alt=""
              />{/if}
            <span class="details">
              <span
                >{itemDisplayName(
                  entry.item_def_id,
                  entry.enchant,
                  $locale
                )}</span
              >
              <span class="figures"
                ><span>×{entry.quantity}</span><GoldAmount
                  copper={entry.unit_price}
                /></span
              >
            </span>
          </button>
          {#if stall.owned}
            <button
              class="close-btn"
              aria-label={$t('stall.removeOrder')}
              onclick={() =>
                networkManager.sendRemoveStallBuyOrder(entry.order_id)}
              >×</button
            >
          {/if}
        </div>
      {:else}
        <div class="empty-note">
          {$t(stall.owned ? 'stall.chooseItem' : 'stall.emptyOrders')}
        </div>
      {/each}
    </div>
  </section>
  <section class="trade-column">
    {#if stall.owned}
      <div class="column-title">{$t('stall.findItem')}</div>
      <input
        class="text-field search"
        aria-label={$t('stall.searchItems')}
        placeholder={$t('stall.searchItems')}
        bind:value={search}
      />
      <div class="item-list">
        {#each searchResults as { def, name } (def.id)}
          <button
            class="item-row"
            class:selected={draftItem === def.id}
            onclick={() => draft(def.id)}
            use:itemTooltip={{ def, side: 'left' }}
          >
            <img class="item-icon" src="/items/{def.icon}" alt="" />
            <span class="details">{name}</span>
          </button>
        {:else}<div class="empty-note">
            {$t('stall.noSearchResults')}
          </div>{/each}
      </div>
    {:else}
      <div class="column-title">{$t('stall.matchingItems')}</div>
      <div class="item-list">
        {#each matchingBag as item (item.instance_id)}
          {@const def = getItemDef(item.item_def_id)}
          <button
            class="item-row"
            class:selected={selectedInstanceId === item.instance_id}
            onclick={() => {
              selectedInstanceId = item.instance_id
              saleQuantity = 1
            }}
            use:itemTooltip={def ? { def, item, side: 'left' } : null}
          >
            {#if def}<img
                class="item-icon"
                src="/items/{def.icon}"
                alt=""
              />{/if}
            <span class="details"
              >{itemDisplayName(item.item_def_id, item.enchant, $locale)} ×{item.quantity}</span
            >
          </button>
        {:else}<div class="empty-note">
            {$t(order ? 'stall.noMatchingItems' : 'stall.chooseOrder')}
          </div>{/each}
      </div>
    {/if}
  </section>
</div>

{#if stall.owned}
  <p class="note">
    {$t('stall.orderHint', { max: STALL_MAX_LISTINGS })}
  </p>
  {#if draftItem}
    <div class="draft">
      <strong>{itemDisplayName(draftItem, enchant, $locale)}</strong>
      <div class="fields">
        <label
          >{$t('stall.buyQuantity')}
          <input
            class="text-field qty"
            type="number"
            min="1"
            max="4294967295"
            bind:value={quantity}
          /></label
        >
        {#if enchantable}<label
            >{$t('stall.enchant')} +<input
              class="text-field qty"
              type="number"
              min="0"
              max="2147483647"
              bind:value={enchant}
            /></label
          >{/if}
        <label
          >{$t('stall.unitPrice')}
          <input
            class="text-field price-input"
            placeholder="1g 20s, 350"
            bind:value={priceText}
          /></label
        >
      </div>
      <div class="figures">
        <span>{$t('stall.total')} <GoldAmount copper={draftCost} /></span>
        <button class="confirm-btn" disabled={!canSave} onclick={save}
          >{$t(replacing ? 'stall.updateOrder' : 'stall.addOrder')}</button
        >
        <button
          class="close-btn"
          aria-label={$t('common.cancel')}
          onclick={() => (draftItem = null)}>×</button
        >
      </div>
      {#if draftCost + otherCost > $playerGold}<p class="error">
          {$t('stall.insufficientBudget')}
        </p>{/if}
    </div>
  {/if}
{:else if order && saleItem}
  <div class="draft">
    <div class="fields">
      <label
        >{$t('stall.sellQuantity')}
        <input
          class="text-field qty"
          type="number"
          min="1"
          max={saleMax}
          bind:value={saleQuantity}
        /></label
      >
      <span>{$t('quantity.maximum', { max: saleMax })}</span>
    </div>
    <div class="figures">
      <span>{$t('stall.total')}</span><GoldAmount copper={saleTotal} />
    </div>
    <div class="figures">
      <span>{$t('stall.tax', { percent: STALL_TAX_PERCENT })}</span><GoldAmount
        copper={tax}
      />
    </div>
    <div class="figures">
      <strong>{$t('stall.proceeds')}</strong><GoldAmount
        copper={saleTotal - tax}
      />
    </div>
    <button class="confirm-btn" disabled={!canSell} onclick={sell}
      >{$t('stall.confirmSale')}</button
    >
  </div>
{/if}

<style>
  .order-row,
  .fields,
  .figures,
  label {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .order-row .item-row {
    flex: 1;
    min-width: 0;
  }
  .details {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .figures {
    justify-content: space-between;
  }
  .details .figures {
    margin-top: 4px;
    color: #9fb2c3;
  }
  .item-row.selected {
    border-color: #f0c040;
    background: rgba(240, 192, 64, 0.12);
  }
  .text-field {
    box-sizing: border-box;
    padding: 3px 6px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.35);
    color: #e6edf3;
    font: inherit;
  }
  .text-field:focus-visible {
    border-color: #f0c040;
    outline: none;
  }
  .search {
    width: 100%;
    margin-bottom: 6px;
  }
  .qty {
    width: 65px;
  }
  .price-input {
    width: 120px;
  }
  .fields {
    flex-wrap: wrap;
  }
  .note {
    max-width: 476px;
    word-break: keep-all;
    color: #9fb2c3;
    margin: 8px 0 0;
  }
  .draft {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 8px;
    margin-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.15);
  }
  .error {
    color: #f0a0a0;
    margin: 0;
  }
</style>
