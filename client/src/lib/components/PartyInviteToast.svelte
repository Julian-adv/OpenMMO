<script lang="ts">
  import { t } from '../i18n'
  import { pendingPartyInvites, INVITE_TTL_MS } from '../stores/partyStore'
  import { networkManager } from '../network/socket'
  import QueuedConsentToast from './QueuedConsentToast.svelte'
</script>

<QueuedConsentToast
  queue={pendingPartyInvites}
  ttlMs={INVITE_TTL_MS}
  label={$t('party.inviteTitle')}
  top="20%"
  accent="#7ec8ff"
  acceptLabel={$t('party.join')}
  declineLabel={$t('common.decline')}
  respond={(invite, accept) =>
    networkManager.sendPartyRespond(invite.inviterId, accept)}
>
  {#snippet children(invite)}
    {$t('party.inviteMessage', { name: invite.inviterName })}
  {/snippet}
</QueuedConsentToast>
