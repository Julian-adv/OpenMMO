<script lang="ts">
  import { t } from '../i18n'
  import {
    pendingFamilyMerges,
    FAMILY_MERGE_TTL_MS,
    type PendingFamilyMerge,
  } from '../stores/familyStore'
  import { networkManager } from '../network/socket'
  import QueuedConsentToast from './QueuedConsentToast.svelte'

  const pick = (offer: PendingFamilyMerge, side: 'proposer' | 'target') =>
    side === 'proposer' ? offer.proposerFamily : offer.targetFamily
</script>

<QueuedConsentToast
  queue={pendingFamilyMerges}
  ttlMs={FAMILY_MERGE_TTL_MS}
  label={$t('family.mergeTitle')}
  top="38%"
  accent="#c8b88a"
  acceptLabel={$t('family.mergeAccept')}
  declineLabel={$t('common.decline')}
  respond={(offer, accept) =>
    networkManager.sendRespondFamilyMerge(offer.proposerId, accept)}
>
  {#snippet children(offer)}
    <div>
      {$t('family.mergeMessage', {
        name: offer.proposerName,
        family: offer.proposerFamily,
      })}
    </div>
    <div>
      {$t('family.mergeOutcome', {
        name: pick(offer, offer.terms.keep_name),
        emblem: pick(offer, offer.terms.keep_emblem),
        head:
          offer.terms.keep_head === 'proposer'
            ? offer.proposerName
            : $t('family.mergeYou'),
        count: offer.members,
      })}
    </div>
    <div>{$t('family.mergeFinal')}</div>
  {/snippet}
</QueuedConsentToast>
