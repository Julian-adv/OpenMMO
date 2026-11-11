<script lang="ts">
  import { t, locale } from '../i18n'
  import { itemDisplayName } from '../data/itemDefs'
  import {
    familyStorageLog,
    familyStorageLogVisible,
  } from '../stores/familyStore'
  import { networkManager } from '../network/socket'
  import { draggablePanel } from '../actions/draggablePanel'
  import { hintTooltip } from '../actions/hintTooltip'

  function refresh() {
    familyStorageLog.set(null)
    networkManager.sendRequestFamilyStorageLog()
  }

  $effect(() => {
    if ($familyStorageLogVisible) refresh()
  })

  const logTime = (at: number) =>
    new Date(at * 1000).toLocaleString($locale, {
      month: 'numeric',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    })
</script>

{#if $familyStorageLogVisible}
  <div
    class="log-panel"
    aria-label={$t('family.storageLog')}
    use:draggablePanel={'familyLog'}
  >
    <div class="panel-header" data-drag-handle>
      <span class="panel-title">{$t('family.storageLog')}</span>
      <button class="text-btn" onclick={refresh}>{$t('family.refresh')}</button>
      <button
        class="close-btn"
        use:hintTooltip={$t('common.close')}
        aria-label={$t('common.close')}
        onclick={() => familyStorageLogVisible.set(false)}>×</button
      >
    </div>
    <div class="log">
      {#if !$familyStorageLog}
        <div class="dim">{$t('family.loading')}</div>
      {:else if $familyStorageLog.length === 0}
        <div class="dim">{$t('family.storageLogEmpty')}</div>
      {:else}
        {#each $familyStorageLog as entry, index (index)}
          <div class="log-row">
            <span class="dim time">{logTime(entry.at)}</span>
            <span class="who">{entry.character_name}</span>
            <span class="qty" class:take={entry.quantity < 0}
              >{entry.quantity > 0 ? '+' : '−'}{Math.abs(entry.quantity)}</span
            >
            <span class="item"
              >{itemDisplayName(entry.item_def_id, 0, $locale)}</span
            >
          </div>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .log-panel {
    position: fixed;
    right: 590px;
    top: 45%;
    transform: translateY(-50%);
    z-index: 40;
    width: 340px;
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    backdrop-filter: blur(4px);
    padding: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    color: #e6edf3;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 12px;
    pointer-events: auto;
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
    margin-bottom: 8px;
  }

  .panel-title {
    flex: 1;
    font-size: 14px;
    font-weight: 700;
    color: #c8b88a;
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

  .text-btn {
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 3px;
    color: #c8b88a;
    font-family: inherit;
    font-size: 11px;
    line-height: 1;
    padding: 3px 6px;
    cursor: pointer;
  }

  .text-btn:hover {
    border-color: rgba(200, 184, 138, 0.6);
  }

  .log {
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: rgba(113, 128, 150, 0.5) transparent;
  }

  .log-row {
    display: flex;
    gap: 8px;
    align-items: baseline;
    padding: 2px 0;
  }

  .dim {
    color: #7f8f9f;
  }

  .time {
    flex: none;
    font-size: 11px;
  }

  .who {
    flex: none;
    max-width: 90px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .qty {
    flex: none;
    color: #9fdc95;
  }

  .qty.take {
    color: #f6ad55;
  }

  .item {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
