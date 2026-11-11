<script lang="ts">
  import { t, locale } from '../i18n'
  import {
    familyInfo,
    familyStampCount,
    FAMILY_FOUND_STAMPS,
    FAMILY_RENAME_STAMPS,
    FAMILY_DISSOLVE_STAMPS,
    FAMILY_RANK_HEAD,
    FAMILY_REJOIN_COOLDOWN_HOURS,
  } from '../stores/familyStore'
  import { landAccount } from '../stores/landAccountStore'
  import { networkManager } from '../network/socket'

  let { merchantPlayerId }: { merchantPlayerId: number } = $props()

  const STAMPS = {
    found: FAMILY_FOUND_STAMPS,
    rename: FAMILY_RENAME_STAMPS,
    dissolve: FAMILY_DISSOLVE_STAMPS,
  }
  type Action = keyof typeof STAMPS
  let action = $state<Action | null>(null)
  let name = $state('')
  let confirming = $state(false)

  const family = $derived($familyInfo)
  const isHead = $derived(family?.my_rank === FAMILY_RANK_HEAD)
  const actions = $derived<Action[]>(
    family === null ? ['found'] : isHead ? ['rename', 'dissolve'] : []
  )
  const eligible = $derived(
    $landAccount !== null && $landAccount.plots > 0 && $landAccount.missed === 0
  )
  const renameOpen = $derived(
    family !== null && family.rename_at * 1000 <= Date.now()
  )
  const cost = $derived(action ? STAMPS[action] : 0)
  const canPay = $derived($familyStampCount >= cost)
  const canName = $derived(
    name.trim() !== '' && canPay && (action !== 'found' || eligible)
  )

  // Founding, renaming and dissolving each change the name and end the filing.
  const filedName = $derived(family?.name ?? null)
  $effect(() => {
    void filedName
    action = null
    name = ''
    confirming = false
  })

  function pick(next: Action) {
    action = action === next ? null : next
    name = ''
    confirming = false
  }

  function submitName(event: Event) {
    event.preventDefault()
    if (!canName) return
    if (action === 'found') {
      networkManager.sendCreateFamily(merchantPlayerId, name.trim())
    } else {
      confirming = true
    }
  }
</script>

{#snippet nameForm(filing: 'found' | 'rename', hint: string)}
  <form onsubmit={submitName}>
    <p>{hint}</p>
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
      <button type="submit" disabled={!canName}>{$t(`family.${filing}`)}</button
      >
    </div>
  </form>
{/snippet}

{#if actions.length > 0}
  <div class="family-found">
    <strong>{$t('family.registrarTitle')}</strong>
    <div class="menu">
      {#each actions as item (item)}
        <button class:active={action === item} onclick={() => pick(item)}
          >{$t(`family.${item}`)}</button
        >
      {/each}
    </div>

    {#if action === 'found'}
      {@render nameForm('found', $t('family.foundHint', { count: cost }))}
      {#if $landAccount && !eligible}
        <p class="warn">{$t('family.foundNeedsEstate')}</p>
      {/if}
    {:else if action === 'rename' && family}
      {#if !renameOpen}
        <p>
          {$t('family.renameAt', {
            date: new Date(family.rename_at * 1000).toLocaleDateString($locale),
          })}
        </p>
      {:else if confirming}
        <p class="warn">
          {$t('family.renameConfirm', { name: name.trim(), count: cost })}
        </p>
        <div class="row">
          <button
            onclick={() =>
              networkManager.sendRenameFamily(merchantPlayerId, name.trim())}
            >{$t('family.rename')}</button
          >
          <button onclick={() => (confirming = false)}
            >{$t('common.cancel')}</button
          >
        </div>
      {:else}
        {@render nameForm('rename', $t('family.renameCost', { count: cost }))}
      {/if}
    {:else if action === 'dissolve' && family}
      {#if family.members.length > 1}
        <p class="warn">{$t('family.dissolveNeedsAlone')}</p>
      {:else if confirming}
        <p class="warn">
          {$t('family.dissolveConfirm', {
            family: family.name,
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
          {$t('family.dissolveHint', { family: family.name, count: cost })}
        </p>
        <button
          class="danger"
          disabled={!canPay}
          onclick={() => (confirming = true)}>{$t('family.dissolve')}</button
        >
      {/if}
    {/if}
    {#if action && !canPay}
      <p class="warn">
        {$t('family.needsStamps', { count: cost, have: $familyStampCount })}
      </p>
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
  input:focus,
  button:focus,
  button:focus-visible {
    outline: none;
    border-color: #e7d7ae;
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
  .menu {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  .menu button {
    flex: 1;
    background: #26323c;
  }
  .menu button.active {
    border-color: #e7d7ae;
    color: #e7d7ae;
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
