<script lang="ts">
  import { t } from '../i18n'
  import {
    familyInfo,
    familyStampCount,
    FAMILY_FOUND_STAMPS,
    FAMILY_DISSOLVE_STAMPS,
    FAMILY_RANK_HEAD,
    FAMILY_REJOIN_COOLDOWN_HOURS,
  } from '../stores/familyStore'
  import { landAccount } from '../stores/landAccountStore'
  import { networkManager } from '../network/socket'

  let { merchantPlayerId }: { merchantPlayerId: number } = $props()

  let name = $state('')
  const eligible = $derived(
    $landAccount !== null && $landAccount.plots > 0 && $landAccount.missed === 0
  )

  const canDissolve = $derived(
    $familyInfo !== null &&
      $familyInfo.my_rank === FAMILY_RANK_HEAD &&
      $familyInfo.members.length === 1
  )
  let confirming = $state(false)
  $effect(() => {
    if (!canDissolve) confirming = false
  })

  function submit(event: Event) {
    event.preventDefault()
    const trimmed = name.trim()
    if (!trimmed || !eligible || $familyStampCount < FAMILY_FOUND_STAMPS) return
    networkManager.sendCreateFamily(merchantPlayerId, trimmed)
  }
</script>

{#if $familyInfo === null}
  <form class="family-found" onsubmit={submit}>
    <strong>{$t('family.found')}</strong>
    <p>{$t('family.foundHint', { count: FAMILY_FOUND_STAMPS })}</p>
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
        disabled={!name.trim() ||
          !eligible ||
          $familyStampCount < FAMILY_FOUND_STAMPS}>{$t('family.found')}</button
      >
    </div>
    {#if $landAccount && !eligible}
      <p class="warn">{$t('family.foundNeedsEstate')}</p>
    {:else if $familyStampCount < FAMILY_FOUND_STAMPS}
      <p class="warn">
        {$t('family.foundNeedsStamps', {
          count: FAMILY_FOUND_STAMPS,
          have: $familyStampCount,
        })}
      </p>
    {/if}
  </form>
{:else if canDissolve}
  <div class="family-found">
    <strong>{$t('family.dissolve')}</strong>
    {#if confirming}
      <p class="warn">
        {$t('family.dissolveConfirm', {
          family: $familyInfo.name,
          hours: FAMILY_REJOIN_COOLDOWN_HOURS,
        })}
      </p>
      <div class="row">
        <button
          class="danger"
          onclick={() => networkManager.sendDissolveFamily(merchantPlayerId)}
          >{$t('family.dissolve')}</button
        >
        <button onclick={() => (confirming = false)}
          >{$t('common.cancel')}</button
        >
      </div>
    {:else}
      <p>
        {$t('family.dissolveHint', {
          family: $familyInfo.name,
          count: FAMILY_DISSOLVE_STAMPS,
        })}
      </p>
      <button
        class="danger"
        disabled={$familyStampCount < FAMILY_DISSOLVE_STAMPS}
        onclick={() => (confirming = true)}>{$t('family.dissolve')}</button
      >
      {#if $familyStampCount < FAMILY_DISSOLVE_STAMPS}
        <p class="warn">
          {$t('family.foundNeedsStamps', {
            count: FAMILY_DISSOLVE_STAMPS,
            have: $familyStampCount,
          })}
        </p>
      {/if}
    {/if}
  </div>
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
  button.danger {
    background: #5a2f2f;
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .warn {
    color: #efae87;
  }
</style>
