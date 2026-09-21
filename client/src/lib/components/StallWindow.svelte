<script lang="ts">
  import { locale } from '../i18n'
  import './tradePanel.css'
  import { closeStallPanel, openStall } from '../stores/stallStore'
  import { inventoryStore, playerGold } from '../stores/inventoryStore'
  import { getItemDef, itemDisplayName } from '../data/itemDefs'
  import { STALL_MAX_LISTINGS } from '../data/stallLimits'
  import { parseGold } from '../utils/currency'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import { draggablePanel } from '../actions/draggablePanel'
  import { itemTooltip } from '../actions/itemTooltip'
  import { sortBag } from './inventorySort'
  import GoldAmount from './GoldAmount.svelte'
  import QuantityPopup from './QuantityPopup.svelte'
  import type { ItemInstance, StallListing } from '../network/networkTypes'

  type CartLine = { listing: StallListing; qty: number }

  const stall = $derived($openStall)
  const listings = $derived(stall?.listings ?? [])
  const listed = $derived(new Set(listings.map((l) => l.instance_id)))
  const bag = $derived(
    sortBag($inventoryStore.bag).filter(
      (item) => !item.locked && !listed.has(item.instance_id)
    )
  )

  let draft = $state<{ item: ItemInstance; quantity: number } | null>(null)
  let priceText = $state('')
  let cart = $state<CartLine[]>([])
  let pendingAdd = $state<StallListing | null>(null)
  let signText = $state('')
  let error = $state<string | null>(null)

  // Adopt the server's sign whenever the panel lands on a different stall.
  let signedStall = $state<number | null>(null)
  $effect(() => {
    if (!stall || stall.stall_id === signedStall) return
    signedStall = stall.stall_id
    signText = stall.sign
    draft = null
    cart = []
    pendingAdd = null
    error = null
  })

  $effect(() => {
    if (!stall) return
    return mountOverlay('stall', closeStallPanel)
  })

  // Remove sold-out or withdrawn listings from the cart.
  $effect(() => {
    if (pendingAdd && !listed.has(pendingAdd.instance_id)) pendingAdd = null
    if (cart.some((line) => !listed.has(line.listing.instance_id))) {
      cart = cart.filter((line) => listed.has(line.listing.instance_id))
    }
  })

  const draftPrice = $derived(parseGold(priceText))
  $effect(() => {
    if (
      draft &&
      !bag.some((item) => item.instance_id === draft?.item.instance_id)
    )
      draft = null
  })
  const draftValid = $derived(
    draft !== null &&
      draft.quantity > 0 &&
      draft.quantity <= draft.item.quantity &&
      draftPrice !== null &&
      draftPrice >= 0
  )
  const cartTotal = $derived(
    cart.reduce((sum, line) => sum + line.listing.unit_price * line.qty, 0)
  )
  const canConfirm = $derived(cart.length > 0 && cartTotal <= $playerGold)

  function inCart(instanceId: number): number {
    return (
      cart.find((line) => line.listing.instance_id === instanceId)?.qty ?? 0
    )
  }

  const addMax = $derived(
    pendingAdd === null
      ? 1
      : Math.max(
          1,
          Math.min(
            pendingAdd.quantity - inCart(pendingAdd.instance_id),
            pendingAdd.unit_price > 0
              ? Math.floor(($playerGold - cartTotal) / pendingAdd.unit_price)
              : pendingAdd.quantity
          )
        )
  )

  function startDraft(item: ItemInstance) {
    if (listings.length >= STALL_MAX_LISTINGS) {
      error = `A stall holds ${STALL_MAX_LISTINGS} kinds of goods.`
      return
    }
    error = null
    priceText = ''
    draft = { item, quantity: item.quantity }
  }

  function submitDraft() {
    if (!draft || !draftValid || draftPrice === null) return
    networkManager.sendListStallItem(
      draft.item.instance_id,
      draft.quantity,
      draftPrice
    )
    draft = null
  }

  function chooseListing(listing: StallListing) {
    if (!stall) return
    if (stall.owned) {
      networkManager.sendUnlistStallItem(listing.instance_id)
      return
    }
    const left = listing.quantity - inCart(listing.instance_id)
    if (left < 1) {
      error = "That's all of them."
      return
    }
    if (listing.unit_price > $playerGold - cartTotal) {
      error = "That's more than you're carrying."
      return
    }
    error = null
    if (left === 1) addToCart(listing, 1)
    else pendingAdd = listing
  }

  function addToCart(listing: StallListing, qty: number) {
    const existing = cart.find(
      (line) => line.listing.instance_id === listing.instance_id
    )
    if (existing) existing.qty += qty
    else cart.push({ listing, qty })
  }

  function confirmAdd(qty: number) {
    if (!pendingAdd) return
    addToCart(pendingAdd, Math.min(qty, addMax))
    pendingAdd = null
  }

  function removeOne(line: CartLine) {
    if (line.qty > 1) line.qty -= 1
    else cart = cart.filter((entry) => entry !== line)
    error = null
  }

  function buy() {
    if (!stall || !canConfirm) return
    networkManager.sendBuyFromStall(
      stall.stall_id,
      cart.map((line) => ({
        instance_id: line.listing.instance_id,
        quantity: line.qty,
      }))
    )
    cart = []
  }

  function saveSign() {
    networkManager.sendSetStallSign(signText.trim())
  }

  const title = $derived(
    !stall ? '' : stall.sign || `${stall.owner_name}'s stall`
  )
</script>

{#if stall}
  <div
    class="trade-window stall-window"
    role="dialog"
    aria-label="Stall"
    use:draggablePanel={'stall'}
  >
    <header class="panel-header" data-drag-handle>
      <span class="panel-title">{title}</span>
      <button class="close-btn" onclick={closeStallPanel} aria-label="Close">
        ×
      </button>
    </header>

    {#if stall.owned}
      <div class="field-row">
        <input
          class="text-field"
          maxlength="32"
          placeholder="Sign board (optional)"
          bind:value={signText}
          onblur={saveSign}
          onkeydown={(e) => e.key === 'Enter' && saveSign()}
        />
      </div>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}

    <div class="trade-columns">
      <section class="trade-column">
        {#if stall.owned}
          <div class="column-title">
            On the stall ({listings.length}/{STALL_MAX_LISTINGS})
          </div>
        {/if}
        <div class="item-list">
          {#each listings as listing (listing.instance_id)}
            {@const def = getItemDef(listing.item_def_id)}
            <button
              class="item-row listing-row"
              title={stall.owned ? 'Take it back off the stall' : 'Buy'}
              onclick={() => chooseListing(listing)}
              use:itemTooltip={def
                ? { def, enchant: listing.enchant, side: 'right' }
                : null}
            >
              <span class="icon-cell">
                {#if def}
                  <img
                    class="item-icon"
                    src="/items/{def.icon}"
                    alt=""
                    draggable="false"
                  />
                {/if}
              </span>
              <span class="item-name">
                {itemDisplayName(listing.item_def_id, listing.enchant, $locale)}
              </span>
              <span class="figures">
                <span class="stock">×{listing.quantity}</span>
                <span class="price"
                  ><GoldAmount copper={listing.unit_price} /></span
                >
              </span>
            </button>
          {:else}
            <div class="empty-note">
              {stall.owned ? 'Put something out to sell' : 'Nothing for sale'}
            </div>
          {/each}
        </div>
      </section>

      {#if !stall.owned}
        <section class="trade-column cart-column">
          <div class="cart-line cart-current">
            <span class="cart-label">Current</span>
            <GoldAmount copper={$playerGold} />
          </div>
          <div class="column-title">Cart</div>
          <div class="item-list">
            {#each cart as line (line.listing.instance_id)}
              {@const def = getItemDef(line.listing.item_def_id)}
              <button
                class="item-row listing-row"
                title="Take one back off"
                onclick={() => removeOne(line)}
                use:itemTooltip={def
                  ? { def, enchant: line.listing.enchant, side: 'left' }
                  : null}
              >
                <span class="icon-cell">
                  {#if def}
                    <img
                      class="item-icon"
                      src="/items/{def.icon}"
                      alt=""
                      draggable="false"
                    />
                  {/if}
                </span>
                <span class="item-name">
                  {itemDisplayName(
                    line.listing.item_def_id,
                    line.listing.enchant,
                    $locale
                  )}
                </span>
                <span class="figures">
                  <span class="stock">×{line.qty}</span>
                  <span class="price cost"
                    >−<GoldAmount
                      copper={line.listing.unit_price * line.qty}
                    /></span
                  >
                </span>
              </button>
            {:else}
              <div class="empty-note">Click items to add</div>
            {/each}
          </div>
          <div class="cart-footer">
            <div class="cart-line">
              <span class="cart-label">Total</span>
              <span class="price cost"
                >{cartTotal === 0 ? '' : '−'}<GoldAmount
                  copper={cartTotal}
                /></span
              >
            </div>
            <div class="cart-line">
              <span class="cart-label">After</span>
              <GoldAmount copper={$playerGold - cartTotal} />
            </div>
            <button class="confirm-btn" disabled={!canConfirm} onclick={buy}>
              Confirm
            </button>
          </div>
        </section>
      {:else}
        <section class="trade-column bag-column">
          <div class="column-title">Your bag</div>
          <div class="item-list">
            {#each bag as item (item.instance_id)}
              {@const def = getItemDef(item.item_def_id)}
              <button
                class="item-row bag-row"
                class:selected={draft?.item.instance_id === item.instance_id}
                title="Put it on the stall"
                onclick={() => startDraft(item)}
                use:itemTooltip={def ? { def, item, side: 'left' } : null}
              >
                <span class="icon-cell">
                  {#if def}
                    <img
                      class="item-icon"
                      src="/items/{def.icon}"
                      alt=""
                      draggable="false"
                    />
                  {/if}
                </span>
                <span class="item-name">
                  {itemDisplayName(item.item_def_id, item.enchant, $locale)}
                </span>
                <span class="figures">
                  <span class="stock">×{item.quantity}</span>
                </span>
              </button>
            {:else}
              <div class="empty-note">Your bag is empty</div>
            {/each}
          </div>
        </section>
      {/if}
    </div>

    {#if draft}
      {@const draftDef = getItemDef(draft.item.item_def_id)}
      <div class="draft">
        <span class="icon-cell">
          {#if draftDef}
            <img
              class="item-icon"
              src="/items/{draftDef.icon}"
              alt=""
              draggable="false"
            />
          {/if}
        </span>
        <span class="draft-name">
          {itemDisplayName(draft.item.item_def_id, draft.item.enchant, $locale)}
        </span>
        <label class="draft-field">
          <span class="draft-label">Sell</span>
          <input
            class="text-field qty"
            type="number"
            min="1"
            max={draft.item.quantity}
            bind:value={draft.quantity}
          />
        </label>
        <label class="draft-field draft-price">
          <span class="draft-label">at</span>
          <input
            class="text-field"
            placeholder="1g 20s, 350"
            bind:value={priceText}
          />
        </label>
        <button
          class="confirm-btn"
          disabled={!draftValid}
          onclick={submitDraft}
        >
          List
        </button>
        <button
          class="close-btn"
          onclick={() => (draft = null)}
          aria-label="Cancel">×</button
        >
      </div>
    {/if}

    {#if stall.owned}
      <footer>
        <span class="purse-label">Purse</span>
        <span class="price"><GoldAmount copper={$playerGold} /></span>
      </footer>
    {/if}
  </div>
{/if}

<QuantityPopup
  visible={pendingAdd !== null}
  itemName={pendingAdd
    ? itemDisplayName(pendingAdd.item_def_id, pendingAdd.enchant, $locale)
    : ''}
  icon={pendingAdd ? (getItemDef(pendingAdd.item_def_id)?.icon ?? '') : ''}
  max={addMax}
  defaultQty={1}
  onConfirm={confirmAdd}
  onCancel={() => (pendingAdd = null)}
/>

<style>
  .stall-window {
    z-index: 45;
  }

  .field-row {
    padding-bottom: 8px;
  }

  .text-field {
    width: 100%;
    padding: 3px 6px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.35);
    color: #e6edf3;
    font-family: inherit;
    font-size: inherit;
  }

  .text-field:focus-visible {
    border-color: #f0c040;
    outline: none;
  }

  /* Name, stock and price cannot share one 230px line without something
     truncating, so the row stacks: the name takes the full width and the
     figures keep their own line under it. Scoped under .trade-window so it
     outranks tradePanel.css's `.item-row` flex. */
  .trade-window .listing-row,
  .trade-window .bag-row {
    display: grid;
    grid-template-columns: 28px 1fr;
    gap: 1px 8px;
    align-items: center;
  }

  .icon-cell {
    width: 28px;
    height: 28px;
    grid-row: span 2;
  }

  /* tradePanel.css clips this to one ellipsised line; here a name must never
     be cut, so it wraps instead. */
  .trade-window .item-row .item-name {
    flex: none;
    overflow: visible;
    white-space: normal;
    overflow-wrap: anywhere;
    line-height: 1.25;
  }

  /* Same width on every row, so the price lands on one right edge down the
     list — which is the whole point of a price list. */
  .figures {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }

  .trade-window .item-row.selected {
    border-color: #f0c040;
    background: rgba(240, 192, 64, 0.12);
  }

  .stock {
    color: #9fb2c3;
    white-space: nowrap;
  }

  .price {
    font-size: 13px;
    font-weight: 700;
    text-align: right;
    white-space: nowrap;
  }

  .error {
    margin: 0 0 6px;
    color: #f0a0a0;
  }

  .draft {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 8px;
    margin-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.15);
  }

  .draft-field {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .draft-price {
    flex: 1;
  }

  .draft-label {
    color: #9fb2c3;
  }

  /* An echo of the highlighted bag row, not list data: this one may shorten
     so the price field keeps a usable width. */
  .draft-name {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .qty {
    width: 58px;
  }

  footer {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding-top: 8px;
    margin-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.15);
  }

  /* The purse lands on the same right edge as the prices, so what you have
     and what things cost read as one column. */
  .purse-label {
    color: #9fb2c3;
  }

  footer .price {
    flex: 1;
  }

  /* Borrowed wholesale from the merchant shop's cart so the two purchases
     read as the same act (TradeWindow.svelte). */
  .cart-column {
    display: flex;
    flex-direction: column;
  }

  .cart-line {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .cart-current {
    padding-bottom: 4px;
  }

  .cart-label {
    color: #9fb2c3;
    font-weight: 700;
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

  .price.cost {
    color: #ff9a8a;
  }
</style>
