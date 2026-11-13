<script lang="ts">
  import { tick } from 'svelte'
  import { t } from '../i18n'
  import {
    familyInfo,
    mergerPledgeOpen,
    shownFamilyEmblems,
    type MergeSide,
    type FamilyMergeTerms,
  } from '../stores/familyStore'
  import { gameStore } from '../stores/gameStore'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import type { Emblem } from '../utils/familyEmblem'
  import EmblemChip from './EmblemChip.svelte'

  let dialog = $state<HTMLFormElement>()
  let head = $state('')
  let terms = $state<FamilyMergeTerms>({
    keep_name: 'proposer',
    keep_emblem: 'proposer',
    keep_head: 'proposer',
  })
  const family = $derived($familyInfo)
  const others = $derived(
    [...$gameStore.otherPlayers.values()].filter(
      (p) => !p.isOfficialNpc && p.family && p.family !== family?.name
    )
  )
  const nearby = $derived(others.map((p) => p.name))
  const typed = $derived(head.trim().toLowerCase())
  /** The head typed so far, once it names someone nearby. */
  const them = $derived(others.find((p) => p.name.toLowerCase() === typed))
  const theirName = $derived(them?.name)
  const theirFamily = $derived(them?.family)
  const myName = $derived($gameStore.currentPlayer?.name)
  type Choice = { side: MergeSide; label: string; emblem?: Emblem }
  const rows = $derived<
    { term: keyof FamilyMergeTerms; label: string; choices: Choice[] }[]
  >([
    {
      term: 'keep_name',
      label: $t('family.mergeKeepName'),
      choices: [
        { side: 'proposer', label: family?.name ?? $t('family.mergeOurs') },
        { side: 'target', label: theirFamily ?? $t('family.mergeTheirs') },
      ],
    },
    {
      term: 'keep_emblem',
      label: $t('family.mergeKeepEmblem'),
      choices: [
        {
          side: 'proposer',
          label: $t('family.mergeOurs'),
          emblem: family
            ? { texture: family.emblem_texture, color: family.emblem_color }
            : undefined,
        },
        {
          side: 'target',
          label: theirFamily ?? $t('family.mergeTheirs'),
          emblem: theirName ? $shownFamilyEmblems.get(typed) : undefined,
        },
      ],
    },
    {
      term: 'keep_head',
      label: $t('family.mergeKeepHead'),
      choices: [
        { side: 'proposer', label: myName ?? $t('family.mergeMe') },
        { side: 'target', label: theirName ?? $t('family.mergeThem') },
      ],
    },
  ])
  $effect(() => {
    if (theirName) networkManager.sendInspectFamilyEmblem(theirName)
  })
  const close = () => mergerPledgeOpen.set(false)

  $effect(() => mountOverlay('mergerPledge', close))
  $effect(() => {
    tick().then(() => dialog?.querySelector('input')?.focus())
  })

  function submit(event: Event) {
    event.preventDefault()
    const name = head.trim()
    if (!name) return
    networkManager.sendProposeFamilyMerge(name, { ...terms })
    close()
  }
</script>

<div class="pledge-backdrop">
  <form
    class="pledge-dialog"
    aria-labelledby="pledge-title"
    bind:this={dialog}
    onsubmit={submit}
  >
    <h2 id="pledge-title">{$t('family.pledgeTitle')}</h2>
    <p>{$t('family.pledgeIntro')}</p>
    <input
      type="text"
      list="pledge-heads"
      maxlength="32"
      autocomplete="off"
      placeholder={$t('family.mergeHeadName')}
      aria-label={$t('family.mergeHeadName')}
      bind:value={head}
    />
    <datalist id="pledge-heads">
      {#each nearby as name (name)}
        <option value={name}></option>
      {/each}
    </datalist>
    {#each rows as row (row.term)}
      <div class="row" role="radiogroup" aria-label={row.label}>
        <span>{row.label}</span>
        <span class="choices">
          {#each row.choices as choice (choice.side)}
            <button
              type="button"
              role="radio"
              class="choice"
              class:chosen={terms[row.term] === choice.side}
              aria-checked={terms[row.term] === choice.side}
              onclick={() => (terms[row.term] = choice.side)}
            >
              <EmblemChip emblem={choice.emblem} />
              {choice.label}
            </button>
          {/each}
        </span>
      </div>
    {/each}
    <p class="hint">{$t('family.mergeHint')}</p>
    <div class="actions">
      <button type="button" class="secondary" onclick={close}
        >{$t('common.cancel')}</button
      >
      <button type="submit" disabled={!head.trim()}
        >{$t('family.pledgeSubmit')}</button
      >
    </div>
  </form>
</div>

<style>
  .pledge-backdrop {
    position: absolute;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    padding: 16px;
    box-sizing: border-box;
    background: rgba(8, 5, 4, 0.58);
    backdrop-filter: blur(2px);
  }
  .pledge-dialog {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: min(380px, calc(100vw - 32px));
    box-sizing: border-box;
    padding: 22px;
    border: 1px solid rgba(200, 184, 138, 0.7);
    border-radius: 12px;
    background: #18160f;
    color: #f3e8d2;
    box-shadow: 0 22px 55px rgba(0, 0, 0, 0.55);
    font:
      12px/1.5 'Noto Sans KR',
      sans-serif;
  }
  h2 {
    margin: 0;
    color: #fff4df;
    font-size: 18px;
  }
  p {
    margin: 0;
    color: #dbc98f;
  }
  .hint {
    color: #a99f8c;
    font-size: 11px;
  }
  .choices {
    display: flex;
    gap: 6px;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-color: #675b4e;
    background: #0f0d09;
    font-weight: 400;
  }
  .choice.chosen {
    border-color: #c8b88a;
    background: #3a321c;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  input {
    padding: 6px 8px;
    border: 1px solid #675b4e;
    border-radius: 6px;
    background: #0f0d09;
    color: inherit;
    font: inherit;
  }
  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin-top: 6px;
  }
  button {
    padding: 9px 12px;
    border: 1px solid #c8b88a;
    border-radius: 7px;
    background: #6b5a2e;
    color: #fff4df;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }
  button.secondary {
    border-color: #675b4e;
    background: #2b251f;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
