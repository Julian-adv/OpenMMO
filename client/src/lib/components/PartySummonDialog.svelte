<script lang="ts">
  import './hudDialog.css'
  import { t } from '../i18n'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import { gameStore } from '../stores/gameStore'
  import { partyRoster, summonPicker } from '../stores/partyStore'

  const scroll = $derived($summonPicker)
  const selfId = $derived($gameStore.currentPlayer?.id)
  const members = $derived(
    ($partyRoster?.members ?? []).filter((member) => member.id !== selfId)
  )

  const close = () => summonPicker.set(null)

  $effect(() => mountOverlay('party-summon', close))

  $effect(() => {
    if (!$partyRoster) close()
  })

  function call(memberId: number) {
    if (scroll === null) return
    networkManager.sendSummonPartyMember(scroll, memberId)
    close()
  }
</script>

<div class="hud-dialog-layer">
  <div
    class="hud-dialog summon-dialog"
    role="dialog"
    aria-labelledby="summon-title"
  >
    <div class="panel-header">
      <h2 class="panel-title" id="summon-title">
        {$t('party.summonPickTitle')}
      </h2>
      <button
        type="button"
        class="close-btn"
        onclick={close}
        title={$t('common.close')}
        aria-label={$t('common.close')}>&times;</button
      >
    </div>
    <ul>
      {#each members as member (member.id)}
        <li>
          <span class="name">{member.name}</span>
          <button
            type="button"
            class="btn primary"
            onclick={() => call(member.id)}>{$t('party.summonPick')}</button
          >
        </li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .summon-dialog {
    width: 280px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: 45vh;
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
