<script lang="ts">
  import { t } from '../i18n'
  import {
    pendingFamilyMerges,
    FAMILY_MERGE_TTL_MS,
    type PendingFamilyMerge,
  } from '../stores/familyStore'
  import { networkManager } from '../network/socket'
  import { gameStore } from '../stores/gameStore'
  import QueuedConsentToast from './QueuedConsentToast.svelte'
  import { capeTextureUrl } from '../utils/networkUtils'

  const pick = (offer: PendingFamilyMerge, side: 'proposer' | 'target') =>
    side === 'proposer' ? offer.proposerFamily : offer.targetFamily
  const own = (offer: PendingFamilyMerge) =>
    $gameStore.currentPlayer?.id === offer.proposerId
</script>

<QueuedConsentToast
  queue={pendingFamilyMerges}
  ttlMs={FAMILY_MERGE_TTL_MS}
  label={$t('family.mergeTitle')}
  top="38%"
  accent="#c8b88a"
  width="340px"
  gauge={false}
  acceptLabel={$t('family.mergeAccept')}
  declineLabel={$t('family.mergeDecline')}
  respond={(offer, accept) =>
    own(offer)
      ? networkManager.sendConfirmFamilyMerge(accept)
      : networkManager.sendRespondFamilyMerge(offer.proposerId, accept)}
>
  {#snippet children(offer)}
    <div>
      {own(offer)
        ? $t('family.mergeAskOwn', { family: offer.targetFamily })
        : $t('family.mergeMessage', {
            name: offer.proposerName,
            family: offer.proposerFamily,
          })}
    </div>
    <dl class="outcome">
      <dt>{$t('family.mergeKeepName')}</dt>
      <dd>{pick(offer, offer.terms.keep_name)}</dd>
      <dt>{$t('family.mergeKeepEmblem')}</dt>
      <dd class="emblem">
        {#if offer.emblem.color}
          <span class="swatch" style="background:{offer.emblem.color}"></span>
        {/if}
        {#if offer.emblem.texture}
          <img src={capeTextureUrl(offer.emblem.texture)} alt="" />
        {/if}
        {pick(offer, offer.terms.keep_emblem)}
      </dd>
      <dt>{$t('family.mergeKeepHead')}</dt>
      <dd>
        {offer.terms.keep_head === 'proposer'
          ? offer.proposerName
          : offer.targetName}
      </dd>
      <dt>{$t('family.members')}</dt>
      <dd>{offer.members}</dd>
    </dl>
    <div class="final">{$t('family.mergeFinal')}</div>
  {/snippet}
</QueuedConsentToast>

<style>
  .outcome {
    display: grid;
    grid-template-columns: auto 1fr;
    margin: 8px 0;
    border: 1px solid rgba(200, 184, 138, 0.35);
    border-radius: 6px;
    overflow: hidden;
  }
  .outcome dt,
  .outcome dd {
    margin: 0;
    padding: 4px 10px;
    border-top: 1px solid rgba(200, 184, 138, 0.2);
  }
  .outcome dt:first-of-type,
  .outcome dd:first-of-type {
    border-top: none;
  }
  .outcome dt {
    color: #c8b88a;
    background: rgba(200, 184, 138, 0.08);
  }
  .outcome dd {
    font-weight: 700;
  }
  .emblem {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .emblem img,
  .swatch {
    width: 28px;
    height: 28px;
    border-radius: 4px;
    object-fit: cover;
  }
  .final {
    color: #9fb2c3;
  }
</style>
