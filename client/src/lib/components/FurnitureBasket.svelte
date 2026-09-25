<script lang="ts">
  import { locale } from '../i18n'
  import {
    furnitureBasket,
    furnitureBasketTotal,
    displayProduct,
    removeFurnitureFromBasket,
    furniturePurchasePending,
  } from '../stores/furnitureShopStore'
  import { itemDisplayName, getItemDef } from '../data/itemDefs'
  import GoldAmount from './GoldAmount.svelte'
</script>

{#if $furnitureBasket.length}
  <div class="basket">
    <strong>ORKEA · Unpaid furniture</strong>
    {#each $furnitureBasket as line (line.displayId)}
      {@const product = displayProduct(line.displayId)!}
      <div class="line">
        <img src="/items/{getItemDef(product.itemDefId)?.icon}" alt="" />
        <span
          >{itemDisplayName(product.itemDefId, 0, $locale)} ×{line.quantity}</span
        >
        <GoldAmount copper={product.price * line.quantity} />
        <button
          disabled={$furniturePurchasePending}
          onclick={() => removeFurnitureFromBasket(line.displayId)}
          aria-label="Remove one {itemDisplayName(
            product.itemDefId,
            0,
            $locale
          )}"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            aria-hidden="true"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0"
            />
          </svg>
        </button>
      </div>
    {/each}
    <div class="total">Total <GoldAmount copper={$furnitureBasketTotal} /></div>
    <small
      >Click Grida to check out. Unpaid items are returned when you leave.</small
    >
  </div>
{/if}

<style>
  .basket {
    display: grid;
    gap: 8px;
    max-height: 280px;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-width: thin;
    scrollbar-color: rgba(113, 128, 150, 0.5) transparent;
  }
  .basket > strong {
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
    font-size: 14px;
    font-weight: 700;
    color: #f0c040;
  }
  .line {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .line img {
    box-sizing: border-box;
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    padding: 2px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    object-fit: contain;
    image-rendering: pixelated;
  }
  .line span {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .total {
    display: flex;
    justify-content: space-between;
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.15);
    font-weight: 700;
  }
  button {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    padding: 4px;
    background: transparent;
    border: none;
    color: #fff;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  small {
    color: #9fb2c3;
    font-size: 11px;
  }
</style>
