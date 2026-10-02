<script lang="ts">
  import { t, locale } from '../i18n'
  import { landClaimReason } from '../i18n/landClaim'
  import { landClaimDialog } from '../stores/landClaimStore'
  import { mountOverlay } from '../stores/overlayStack'
  import { networkManager } from '../network/socket'

  function close() {
    if ($landClaimDialog?.status !== 'pending') landClaimDialog.set(null)
  }

  function confirm() {
    const claim = $landClaimDialog
    if (!claim || claim.status !== 'confirm' || claim.refreshing) return
    landClaimDialog.set({ ...claim, status: 'pending' })
    networkManager.sendLandClaim(claim)
  }

  $effect(() => mountOverlay('landClaim', close))
</script>

{#if $landClaimDialog}
  <div
    class="land-dialog"
    role="dialog"
    aria-modal="false"
    aria-labelledby="land-claim-title"
    tabindex="-1"
  >
    <h2 id="land-claim-title">
      {$landClaimDialog.status === 'claimed'
        ? $t('landClaim.titleClaimed')
        : $landClaimDialog.status === 'rejected'
          ? $t('landClaim.titleRejected')
          : $t('landClaim.titleConfirm')}
    </h2>
    <p class="plot">32 × 32 m · 1,024 m²</p>
    {#if $landClaimDialog.refreshing}
      <p role="status">{$t('landClaim.checkingPlot')}</p>
    {:else if $landClaimDialog.status === 'claimed'}
      <p>{$t('landClaim.claimed')}</p>
      <p>{$t('landClaim.deedConsumed')}</p>
    {:else if $landClaimDialog.status === 'rejected'}
      <p role="alert">{landClaimReason($landClaimDialog.reason, $locale)}</p>
      <p>{$t('landClaim.deedNotConsumed')}</p>
    {:else}
      <p>{$t('landClaim.confirmHint')}</p>
      <p>
        {$t('landClaim.requirements')}
      </p>
      <p>{$t('landClaim.deedCost')}</p>
    {/if}
    <div class="actions">
      {#if $landClaimDialog.status === 'confirm' || $landClaimDialog.status === 'pending'}
        <button
          class="primary"
          onclick={confirm}
          disabled={$landClaimDialog.status === 'pending' ||
            $landClaimDialog.refreshing}
        >
          {$landClaimDialog.refreshing
            ? $t('landClaim.checking')
            : $landClaimDialog.status === 'pending'
              ? $t('landClaim.claiming')
              : $t('landClaim.claimButton')}
        </button>
        <button onclick={close} disabled={$landClaimDialog.status === 'pending'}
          >{$t('common.cancel')}</button
        >
      {:else}
        <button onclick={close}>{$t('common.close')}</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .land-dialog {
    position: fixed;
    left: 16px;
    top: 45%;
    transform: translateY(-50%);
    z-index: 40;
    width: min(300px, calc(100vw - 32px));
    box-sizing: border-box;
    padding: 16px;
    border-radius: 12px;
    border: 1px solid #aa915b;
    background: rgba(16, 20, 16, 0.95);
    color: #f4f4f4;
    font-family: 'Noto Sans KR', sans-serif;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 20px;
  }
  p {
    font-size: 13px;
    line-height: 1.5;
    color: #d4d4d4;
  }
  .plot {
    color: #ebcb83;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 18px;
  }
  button {
    font-family: inherit;
    padding: 7px 12px;
    font-size: 13px;
    border: 1px solid #666;
    border-radius: 6px;
    background: #333;
    color: white;
    cursor: pointer;
  }
  button.primary {
    background: #456334;
    border-color: #799b59;
  }
  button:disabled {
    opacity: 0.6;
    cursor: wait;
  }
</style>
