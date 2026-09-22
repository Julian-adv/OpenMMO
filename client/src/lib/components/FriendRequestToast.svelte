<script lang="ts">
  import { t } from '../i18n'
  import {
    pendingFriendRequests,
    FRIEND_REQUEST_TTL_MS,
  } from '../stores/friendStore'
  import { networkManager } from '../network/socket'
  import QueuedConsentToast from './QueuedConsentToast.svelte'
</script>

<QueuedConsentToast
  queue={pendingFriendRequests}
  ttlMs={FRIEND_REQUEST_TTL_MS}
  label={$t('friends.requestTitle')}
  top="32%"
  accent="#8fe08f"
  acceptLabel={$t('common.accept')}
  declineLabel={$t('common.decline')}
  respond={(request, accept) =>
    networkManager.sendFriendRespond(request.requesterId, accept)}
>
  {#snippet children(request)}
    {$t('friends.requestMessage', { name: request.requesterName })}
  {/snippet}
</QueuedConsentToast>
