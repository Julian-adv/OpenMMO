<script lang="ts">
  import { getItemDef } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import { inventoryStore } from '../stores/inventoryStore'
  import {
    houseDemolitionError,
    houseDemolitionMode,
    houseDemolitionPending,
    housePlacementMode,
    rotateHousePlacement,
    startHouseDemolitionSelection,
    stopHouseDemolitionSelection,
    stopHousePlacement,
  } from '../stores/housePlacementStore'

  const scrolls = $derived(
    $inventoryStore.bag.flatMap((item) => {
      const def = getItemDef(item.item_def_id)
      return def?.category === 'house_scroll' ? [{ item, def }] : []
    })
  )

  function place(instanceId: number) {
    networkManager.sendUseItem(instanceId)
  }
</script>

<div class="house-content">
  <section>
    <strong>House Scrolls</strong>
    {#if scrolls.length === 0}
      <small>No house scrolls in your bag.</small>
    {:else}
      <div class="house-list">
        {#each scrolls as { item, def } (item.instance_id)}
          <div class="house-row">
            <img src="/items/{def.icon}" alt="" />
            <span>{def.name}</span>
            <button
              disabled={$housePlacementMode !== null ||
                $houseDemolitionMode !== null ||
                $houseDemolitionPending !== null}
              onclick={() => place(item.instance_id)}>Place</button
            >
          </div>
        {/each}
      </div>
    {/if}
  </section>

  {#if $housePlacementMode}
    <section class="placement-status">
      <strong>Placing {$housePlacementMode.itemName}</strong>
      <span
        class:error={$housePlacementMode.reason !== null ||
          $housePlacementMode.error !== null}
      >
        {$housePlacementMode.pending
          ? 'Building…'
          : ($housePlacementMode.reason ??
            $housePlacementMode.error ??
            'Click to build here')}
      </span>
      <small
        >Move the pointer · R to rotate · Left-click to build · Esc to cancel</small
      >
      <button
        disabled={$housePlacementMode.pending}
        onclick={rotateHousePlacement}
        >Rotate 90° · {$housePlacementMode.quarterTurns * 90}°</button
      >
      <button onclick={stopHousePlacement}>Cancel placement</button>
    </section>
  {/if}

  <section>
    <strong>Demolish a House</strong>
    {#if $houseDemolitionMode}
      <span
        class:error={$houseDemolitionMode.targetHouseId !== null &&
          !$houseDemolitionMode.valid}>{$houseDemolitionMode.reason}</span
      >
      <small>Point at a house · Left-click to select · Esc to cancel</small>
      <button class="danger" onclick={stopHouseDemolitionSelection}
        >Cancel selection</button
      >
    {:else}
      <small
        >Select your house directly in the world. Its construction scroll will
        be returned.</small
      >
      <button
        class="danger"
        disabled={$housePlacementMode !== null ||
          $houseDemolitionPending !== null}
        onclick={startHouseDemolitionSelection}
        >{$houseDemolitionPending ? 'Demolishing…' : 'Demolish House'}</button
      >
    {/if}
  </section>

  {#if $houseDemolitionError}
    <div class="error" role="status">{$houseDemolitionError}</div>
  {/if}
</div>

<style>
  .house-content {
    display: grid;
    gap: 12px;
    min-width: 310px;
    padding: 12px 16px;
    border-block: 1px solid rgba(226, 185, 59, 0.3);
    font-family: 'Courier New', monospace;
    font-size: 12px;
  }
  section,
  .placement-status {
    display: grid;
    gap: 7px;
  }
  .house-list {
    display: grid;
    gap: 5px;
  }
  .house-row {
    display: grid;
    grid-template-columns: 24px 1fr auto;
    align-items: center;
    gap: 8px;
    padding: 6px;
    border-radius: 5px;
    background: rgba(0, 0, 0, 0.22);
  }
  img {
    width: 24px;
    height: 24px;
    object-fit: contain;
  }
  button {
    cursor: pointer;
    color: #f3e8d2;
    background: #3a3024;
    border: 1px solid #766247;
    border-radius: 4px;
    padding: 5px 9px;
    font-family: inherit;
    font-size: 11px;
    font-weight: bold;
  }
  button.danger {
    border-color: #8b4941;
    background: #4a2723;
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
  small {
    color: #aaa;
  }
  .placement-status span,
  section > span {
    color: #9ee69a;
  }
  .error,
  .placement-status span.error {
    color: #ff8989;
  }
</style>
