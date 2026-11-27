<script lang="ts">
  import { t, locale } from '../i18n'
  import { itemDisplayName } from '../data/itemDefs'
  import { mountOverlay } from '../stores/overlayStack'
  import { reviveItem } from '../stores/inventoryStore'
  import { networkManager } from '../network/socket'
  import { respawnOptions, type RespawnPlace } from '../stores/respawnStore'

  interface Props {
    onRespawn: (place: RespawnPlace) => void
    onLater: () => void
  }

  let { onRespawn, onLater }: Props = $props()

  $effect(() => {
    respawnOptions.set(null)
    networkManager.requestRespawnOptions()
  })

  function useReviveItem(instanceId: number) {
    networkManager.sendUseItem(instanceId)
    onLater()
  }

  // Escape defers the dialog exactly like the Later button.
  $effect(() => mountOverlay('respawn', onLater))
</script>

<div class="respawn-backdrop">
  <div class="respawn-dialog" role="dialog" aria-modal="true">
    <h2>{$t('respawn.title')}</h2>
    <p>{$t('respawn.prompt')}</p>
    <div class="respawn-actions">
      {#if $reviveItem}
        <button
          class="talisman"
          onclick={() => useReviveItem($reviveItem.item.instance_id)}
        >
          {$t('respawn.useItem', {
            item: itemDisplayName($reviveItem.def.id, 0, $locale),
            health: $reviveItem.def.reviveHpPercent ?? 0,
            count: $reviveItem.item.quantity,
          })}
        </button>
      {/if}
      <button class="primary" onclick={() => onRespawn('Town')}
        >{$t('respawn.town')}</button
      >
      {#if $respawnOptions?.estate}
        <button class="shelter" onclick={() => onRespawn('Estate')}
          >{$t('respawn.estate')}</button
        >
      {/if}
      {#if $respawnOptions?.family_seat}
        <button class="shelter" onclick={() => onRespawn('FamilySeat')}
          >{$t('respawn.familySeat')}</button
        >
      {/if}
      {#if $respawnOptions?.nearby}
        {@const nearby = $respawnOptions.nearby}
        <button
          class="shelter"
          onclick={() => onRespawn({ Nearby: { estate_id: nearby.estate_id } })}
          >{$t('respawn.nearby', {
            name: nearby.name,
            distance: Math.round(nearby.distance_m),
          })}</button
        >
      {/if}
      <button class="secondary" onclick={onLater}>{$t('respawn.later')}</button>
    </div>
  </div>
</div>

<style>
  .respawn-backdrop {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.45);
    z-index: 30;
  }

  .respawn-dialog {
    width: min(380px, calc(100vw - 32px));
    padding: 20px;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: rgba(16, 16, 16, 0.95);
    color: #f4f4f4;
    text-align: center;
  }

  .respawn-dialog h2 {
    margin: 0 0 8px 0;
    font-size: 22px;
  }

  .respawn-dialog p {
    margin: 0 0 16px 0;
    color: #d4d4d4;
  }

  .respawn-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    justify-content: center;
  }

  .respawn-actions button {
    border: none;
    border-radius: 8px;
    padding: 10px 14px;
    font-size: 14px;
    cursor: pointer;
  }

  .respawn-actions .primary {
    background: #e2b93b;
    color: #1a1a1a;
    font-weight: 700;
  }

  .respawn-actions .secondary {
    background: #3d3d3d;
    color: #f0f0f0;
  }

  .respawn-actions .shelter {
    background: #8a2b26;
    color: #fff4ee;
    font-weight: 700;
  }

  .respawn-actions .talisman {
    background: #c9632a;
    color: #fff4e6;
    font-weight: 700;
  }
</style>
