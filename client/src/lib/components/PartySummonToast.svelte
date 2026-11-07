<script lang="ts">
  import { t } from '../i18n'
  import { pendingPartySummons, SUMMON_TTL_MS } from '../stores/partyStore'
  import { networkManager } from '../network/socket'
  import QueuedConsentToast from './QueuedConsentToast.svelte'
</script>

<!-- An accept keeps the toast up: the server refuses mid-combat accepts and
     the pending summon survives for a retry, so the gauge must too. A
     successful one clears it via the player's own PlayerTeleported. -->
<QueuedConsentToast
  queue={pendingPartySummons}
  ttlMs={SUMMON_TTL_MS}
  label={$t('party.summonTitle')}
  top="26%"
  accent="#c8a2ff"
  acceptLabel={$t('party.summonAnswer')}
  declineLabel={$t('party.summonIgnore')}
  dismissOnAccept={false}
  respond={(summon, accept) =>
    networkManager.sendPartySummonRespond(summon.casterId, accept)}
>
  {#snippet children(summon)}
    {$t('party.summonMessage', { name: summon.casterName })}
  {/snippet}
</QueuedConsentToast>
