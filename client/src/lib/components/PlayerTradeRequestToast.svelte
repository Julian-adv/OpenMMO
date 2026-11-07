<script lang="ts">
  import { t } from '../i18n'
  import {
    pendingTradeRequests,
    TRADE_REQUEST_TTL_MS,
  } from '../stores/playerTradeStore'
  import { networkManager } from '../network/socket'
  import QueuedConsentToast from './QueuedConsentToast.svelte'
</script>

<QueuedConsentToast
  queue={pendingTradeRequests}
  ttlMs={TRADE_REQUEST_TTL_MS}
  label={$t('trade.requestTitle')}
  top="28%"
  accent="#e0c070"
  acceptLabel={$t('trade.requestAccept')}
  declineLabel={$t('common.decline')}
  respond={(request, accept) =>
    networkManager.sendPlayerTradeRespond(request.requesterId, accept)}
>
  {#snippet children(request)}
    {$t('trade.requestMessage', { name: request.requesterName })}
  {/snippet}
</QueuedConsentToast>
