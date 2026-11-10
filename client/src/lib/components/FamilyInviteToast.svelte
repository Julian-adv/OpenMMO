<script lang="ts">
  import { t } from '../i18n'
  import {
    pendingFamilyInvites,
    FAMILY_INVITE_TTL_MS,
  } from '../stores/familyStore'
  import { networkManager } from '../network/socket'
  import QueuedConsentToast from './QueuedConsentToast.svelte'
</script>

<QueuedConsentToast
  queue={pendingFamilyInvites}
  ttlMs={FAMILY_INVITE_TTL_MS}
  label={$t('family.inviteTitle')}
  top="38%"
  accent="#c8b88a"
  acceptLabel={$t('common.accept')}
  declineLabel={$t('common.decline')}
  respond={(invite, accept) =>
    networkManager.sendRespondFamilyInvite(invite.inviterId, accept)}
>
  {#snippet children(invite)}
    {$t('family.inviteMessage', {
      name: invite.inviterName,
      family: invite.familyName,
    })}
  {/snippet}
</QueuedConsentToast>
