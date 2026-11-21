<script lang="ts">
  import './hudDialog.css'
  import { t } from '../i18n'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import { formatGold } from '../utils/currency'
  import { blessing_price } from '../wasm/onlinerpg_shared'
  import QuantityStepper from './QuantityStepper.svelte'
  import { blessableScrolls, blessSession } from '../stores/bookmarkStore'

  const PRICE = blessing_price()

  const session = $derived($blessSession)
  const owned = $derived($blessableScrolls)
  let quantity = $state(1)
  const count = $derived(Math.min(quantity, owned))

  const close = () => blessSession.set(null)

  $effect(() => mountOverlay('bless', close))

  function bless() {
    if (!session || count < 1) return
    blessSession.set({ ...session, pending: true, error: null })
    networkManager.sendBlessScrolls(session.npcPlayerId, count)
  }
</script>

{#if session}
  <div class="hud-dialog-layer">
    <div
      class="hud-dialog bless-dialog"
      role="dialog"
      aria-labelledby="bless-title"
    >
      <div class="panel-header">
        <h2 class="panel-title" id="bless-title">{$t('bless.menu')}</h2>
        <button
          type="button"
          class="close-btn"
          onclick={close}
          title={$t('common.close')}
          aria-label={$t('common.close')}>&times;</button
        >
      </div>
      <p>{$t('bless.intro', { price: formatGold(PRICE) })}</p>
      <p class="hint">{$t('bless.have', { count: owned })}</p>
      <div class="quantity">
        <span>{$t('bless.quantity')}</span>
        <QuantityStepper
          bind:value={quantity}
          max={Math.max(1, owned)}
          compact
        />
      </div>
      <div class="total">
        <span>{$t('bless.total')}</span>
        <span>{formatGold(PRICE * count)}</span>
      </div>
      {#if session.error}
        <p class="error" role="alert">
          {$t(`bless.error.${session.error}`)}
        </p>
      {:else if session.blessed !== null}
        <p class="done">
          {$t('bless.done', {
            count: session.blessed,
            price: formatGold(PRICE * session.blessed),
          })}
        </p>
      {/if}
      <div class="actions">
        <button
          type="button"
          class="btn primary large"
          disabled={session.pending || owned === 0}
          onclick={bless}>{$t('bless.confirm')}</button
        >
      </div>
    </div>
  </div>
{/if}

<style>
  .bless-dialog {
    width: 320px;
  }
  .done {
    color: #b9d99a;
  }
  .quantity,
  .total {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .total {
    padding-top: 6px;
    border-top: 1px solid rgba(255, 255, 255, 0.15);
    font-weight: 700;
  }
</style>
