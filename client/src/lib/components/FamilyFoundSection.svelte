<script lang="ts">
  import { t } from '../i18n'
  import { familyInfo, FAMILY_CREATE_COST } from '../stores/familyStore'
  import { landAccount } from '../stores/landAccountStore'
  import { playerGold } from '../stores/inventoryStore'
  import { networkManager } from '../network/socket'

  let { merchantPlayerId }: { merchantPlayerId: number } = $props()

  let name = $state('')
  const eligible = $derived(
    $landAccount !== null && $landAccount.plots > 0 && $landAccount.missed === 0
  )

  function submit(event: Event) {
    event.preventDefault()
    const trimmed = name.trim()
    if (!trimmed || !eligible || $playerGold < FAMILY_CREATE_COST) return
    networkManager.sendCreateFamily(merchantPlayerId, trimmed)
  }
</script>

{#if $familyInfo === null}
  <form class="family-found" onsubmit={submit}>
    <strong>{$t('family.found')}</strong>
    <p>{$t('family.foundHint', { cost: FAMILY_CREATE_COST })}</p>
    <div class="row">
      <input
        type="text"
        maxlength="24"
        placeholder={$t('family.namePlaceholder')}
        aria-label={$t('family.namePlaceholder')}
        autocomplete="off"
        bind:value={name}
        onkeydown={(event) => event.stopPropagation()}
      />
      <button
        type="submit"
        disabled={!name.trim() || !eligible || $playerGold < FAMILY_CREATE_COST}
        >{$t('family.found')}</button
      >
    </div>
    {#if $landAccount && !eligible}
      <p class="warn">{$t('family.foundNeedsEstate')}</p>
    {:else if $playerGold < FAMILY_CREATE_COST}
      <p class="warn">{$t('family.foundNeedsGold')}</p>
    {/if}
  </form>
{/if}

<style>
  .family-found {
    border-top: 1px solid #ffffff26;
    margin-top: 12px;
    padding-top: 10px;
    color: #afbfcc;
    font-size: 12px;
    line-height: 1.5;
  }
  strong {
    color: #e7d7ae;
  }
  p {
    margin: 6px 0;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  input {
    flex: 1;
    min-width: 0;
    box-sizing: border-box;
    padding: 6px 8px;
    color: #fff;
    background: #0004;
    border: 1px solid #ffffff44;
    border-radius: 4px;
    font: inherit;
  }
  button {
    padding: 6px 10px;
    border: 1px solid #ffffff44;
    border-radius: 4px;
    color: #dfd9c9;
    background: #38513d;
    font: inherit;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .warn {
    color: #efae87;
  }
</style>
