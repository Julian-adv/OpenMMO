<script lang="ts">
  import { tick } from 'svelte'
  import GoldAmount from './GoldAmount.svelte'
  import { formatGold, parseGold } from '../utils/currency'
  import { playerGold } from '../stores/inventoryStore'
  import { mountOverlay } from '../stores/overlayStack'
  import { BGM_TRACKS } from '../data/bgmTracks'
  import { t } from '../i18n'

  interface Props {
    ownerName: string
    acceptsSongRequests: boolean
    onConfirm: (copper: number, song: string | null) => void
    onCancel: () => void
  }

  let { ownerName, acceptsSongRequests, onConfirm, onCancel }: Props = $props()

  /** Copper presets: a handful of coins, a silver, five silver. */
  const PRESETS = [10, 50, 100, 500]
  const songs = [...BGM_TRACKS].sort((a, b) => a.localeCompare(b))

  let text = $state(formatGold(10))
  let song = $state('')
  let search = $state('')
  let inputEl = $state<HTMLInputElement | null>(null)
  const filteredSongs = $derived(
    songs.filter((title) =>
      title.toLowerCase().includes(search.trim().toLowerCase())
    )
  )

  $effect(() => mountOverlay('tipHat', onCancel))

  $effect(() => {
    tick().then(() => {
      inputEl?.focus()
      inputEl?.select()
    })
  })

  const wallet = $derived($playerGold)
  const amount = $derived(parseGold(text))
  const valid = $derived(amount !== null && amount >= 1 && amount <= wallet)

  function confirm() {
    if (!valid) return
    onConfirm(amount!, acceptsSongRequests && song ? song : null)
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && e.target === inputEl) confirm()
    else if (e.key === 'Escape') onCancel()
  }
</script>

<div class="tip-backdrop">
  <div
    class="tip-dialog"
    class:with-songs={acceptsSongRequests}
    role="dialog"
    aria-modal="true"
    aria-label={$t('tipHat.title', { name: ownerName })}
    tabindex="-1"
    onkeydown={onKeydown}
  >
    <h2>{$t('tipHat.title', { name: ownerName })}</h2>
    <div class="tip-content">
      <div class="tip-form">
        <p>{$t('tipHat.amount')}</p>

        <div class="presets">
          {#each PRESETS as preset (preset)}
            <button class="preset" onclick={() => (text = formatGold(preset))}>
              <GoldAmount copper={preset} />
            </button>
          {/each}
        </div>

        <div class="readout" aria-live="polite">
          {#if amount === null}
            <span class="invalid">{$t('tipHat.invalidAmount')}</span>
          {:else if amount > wallet}
            <span class="invalid"
              >{$t('tipHat.insufficient')} <GoldAmount copper={wallet} /></span
            >
          {:else}
            <GoldAmount copper={amount} />
          {/if}
        </div>

        <input
          bind:this={inputEl}
          type="text"
          bind:value={text}
          aria-label={$t('tipHat.amount')}
        />
        <div class="wallet">
          {$t('tipHat.wallet')}
          <GoldAmount copper={wallet} />
        </div>
      </div>
      {#if acceptsSongRequests}
        <div class="song-picker">
          <label for="tip-song-search">{$t('tipHat.song')}</label>
          <input
            id="tip-song-search"
            type="search"
            bind:value={search}
            placeholder={$t('tipHat.search')}
          />
          <div class="song-list" role="group" aria-label={$t('tipHat.song')}>
            <label class:selected={song === ''}>
              <input type="radio" name="tip-song" value="" bind:group={song} />
              {$t('tipHat.noSong')}
            </label>
            {#each filteredSongs as title (title)}
              <label class:selected={song === title}>
                <input
                  type="radio"
                  name="tip-song"
                  value={title}
                  bind:group={song}
                />
                {title}
              </label>
            {/each}
            {#if filteredSongs.length === 0}
              <p class="empty">{$t('tipHat.noMatches')}</p>
            {/if}
          </div>
          <p class="selected-song" title={song || undefined} aria-live="polite">
            {song ? $t('tipHat.selected', { song }) : ''}
          </p>
          <p class="song-hint">{$t('tipHat.queueHint')}</p>
        </div>
      {/if}
    </div>

    <div class="tip-actions">
      <button class="primary" disabled={!valid} onclick={confirm}
        >{$t(song ? 'tipHat.tipAndRequest' : 'tipHat.tip')}</button
      >
      <button class="secondary" onclick={onCancel}>{$t('common.cancel')}</button
      >
    </div>
  </div>
</div>

<style>
  .tip-backdrop {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.45);
    z-index: 30;
  }

  .tip-dialog {
    width: min(340px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    overflow-y: auto;
    padding: 20px;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: rgba(16, 16, 16, 0.95);
    color: #f4f4f4;
    text-align: center;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 13px;
  }

  button,
  input {
    font-family: inherit;
  }

  .tip-dialog h2 {
    margin: 0 0 8px 0;
    font-size: 18px;
  }

  .tip-dialog p {
    margin: 0 0 14px 0;
    color: #d4d4d4;
  }

  .presets {
    display: flex;
    gap: 6px;
    justify-content: center;
    margin-bottom: 10px;
  }

  .preset {
    flex: 1;
    padding: 6px 4px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    background: rgba(255, 255, 255, 0.06);
    color: inherit;
    cursor: pointer;
  }

  .preset:hover {
    background: rgba(255, 255, 255, 0.14);
  }

  .readout {
    margin-bottom: 6px;
    font-size: 13px;
    min-height: 24px;
  }

  .invalid {
    font-size: 12px;
    color: #d98b8b;
  }

  input:not([type='radio']) {
    width: 100%;
    padding: 8px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: rgba(0, 0, 0, 0.4);
    color: #f4f4f4;
    text-align: center;
    font-size: 13px;
  }

  .wallet {
    margin: 8px 0 16px 0;
    font-size: 12px;
    color: #b9b9b9;
  }

  .tip-actions {
    display: flex;
    gap: 10px;
    justify-content: center;
  }

  .tip-actions button {
    flex: 1;
    padding: 8px 12px;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    cursor: pointer;
  }

  .primary {
    background: #3c6e3c;
    color: #fff;
  }

  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .secondary {
    background: rgba(255, 255, 255, 0.08);
    color: #f4f4f4;
  }

  .with-songs {
    width: min(680px, calc(100vw - 32px));
  }

  .with-songs .tip-content {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
    margin: 16px 0;
  }

  .song-picker {
    min-width: 0;
    text-align: left;
  }

  .song-picker > label {
    display: block;
    margin-bottom: 8px;
  }

  .song-list {
    height: 220px;
    overflow-y: auto;
    scrollbar-gutter: stable;
    scrollbar-width: thin;
    scrollbar-color: rgba(113, 128, 150, 0.5) transparent;
    margin: 8px 0;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 6px;
  }

  @supports selector(::-webkit-scrollbar) {
    .song-list {
      scrollbar-width: auto;
      scrollbar-color: auto;
    }

    .song-list::-webkit-scrollbar {
      width: 8px;
    }

    .song-list::-webkit-scrollbar-track {
      background: transparent;
      margin-block: 4px;
    }

    .song-list::-webkit-scrollbar-thumb {
      background: rgba(113, 128, 150, 0.5);
      border-radius: 999px;
    }

    .song-list::-webkit-scrollbar-thumb:hover {
      background: rgba(160, 174, 192, 0.7);
    }

    .song-list::-webkit-scrollbar-button {
      display: none;
    }
  }

  .song-list label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    font-size: 13px;
    cursor: pointer;
  }

  .song-list label:hover,
  .song-list .selected {
    background: rgba(130, 180, 130, 0.2);
  }

  .song-list input {
    flex-shrink: 0;
    accent-color: #7cb47c;
  }

  .song-picker .song-hint,
  .song-picker .selected-song,
  .song-list .empty {
    margin: 8px 0 0;
    font-size: 12px;
  }

  .selected-song {
    height: 1.5em;
    line-height: 1.5;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  @media (max-width: 600px) {
    .with-songs .tip-content {
      grid-template-columns: 1fr;
      gap: 8px;
    }
  }
</style>
