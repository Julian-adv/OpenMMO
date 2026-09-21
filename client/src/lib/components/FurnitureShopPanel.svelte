<script lang="ts">
  import { locale } from '../i18n'
  import {
    furnitureBasket,
    furnitureShopHover,
    furnitureShopError,
    furnitureShop,
  } from '../stores/furnitureShopStore'
  import { shopSession } from '../stores/tradeStore'
  import { itemDisplayName } from '../data/itemDefs'
  import FurnitureBasket from './FurnitureBasket.svelte'
  import GoldAmount from './GoldAmount.svelte'
</script>

{#if $shopSession?.merchantName !== furnitureShop.clerkNpcName && ($furnitureShopHover || $furnitureBasket.length || $furnitureShopError)}
  <div class="shop-panel">
    {#if $furnitureShopHover}
      <p>
        {itemDisplayName($furnitureShopHover.product.itemDefId, 0, $locale)} · <GoldAmount
          copper={$furnitureShopHover.product.price}
        />
      </p>
      <small>Click the display to add one to your unpaid basket.</small>
    {/if}
    {#if $furnitureBasket.length}
      <FurnitureBasket />
    {/if}
    {#if $furnitureShopError}<p role="status">{$furnitureShopError}</p>{/if}
  </div>
{/if}

<style>
  .shop-panel {
    position: fixed;
    z-index: 150;
    right: 20px;
    bottom: 100px;
    width: 330px;
    backdrop-filter: blur(4px);
    padding: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    max-width: calc(100vw - 64px);
  }
  small {
    display: block;
    margin-top: 8px;
    color: #c9bea1;
  }
  .shop-panel p {
    margin: 8px 0;
  }
  .shop-panel small {
    color: #9fb2c3;
    font-size: 11px;
  }
  .shop-panel [role='status'] {
    color: #f0b8b8;
  }
</style>
