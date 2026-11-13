<script lang="ts">
  import { t, locale } from '../i18n'
  import { impoundSession } from '../stores/impoundStore'
  import { playerGold } from '../stores/inventoryStore'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import { itemDisplayName } from '../data/itemDefs'
  import { formatGold } from '../utils/currency'

  const session = $derived($impoundSession)
  const items = $derived(session?.items ?? [])
  const total = $derived(items.reduce((sum, item) => sum + item.fee, 0))
  const close = () => impoundSession.set(null)

  $effect(() => mountOverlay('impound', close))

  function reclaim() {
    if (!session) return
    impoundSession.set({ ...session, pending: true, error: null })
    networkManager.sendImpoundReclaim(session.npcPlayerId)
  }
</script>

{#if session}
  <div class="impound-backdrop">
    <div class="impound-dialog" role="dialog" aria-labelledby="impound-title">
      <h2 id="impound-title">{$t('impound.title')}</h2>
      <p>{$t('impound.intro')}</p>
      {#if session.reclaimed}
        <p class="done">{$t('impound.reclaimed')}</p>
      {:else if session.items === null}
        <p class="hint">{$t('impound.loading')}</p>
      {:else if items.length === 0}
        <p class="hint">{$t('impound.empty')}</p>
      {:else}
        <ul>
          {#each items as item (item.item_def_id)}
            <li>
              <span
                >{itemDisplayName(item.item_def_id, 0, $locale)} × {item.quantity}</span
              >
              <span>{formatGold(item.fee)}</span>
            </li>
          {/each}
        </ul>
        <div class="total">
          <span>{$t('impound.fee')}</span>
          <span>{formatGold(total)}</span>
        </div>
      {/if}
      {#if session.error}
        <p class="error" role="alert">{session.error}</p>
      {/if}
      <div class="actions">
        <button type="button" class="secondary" onclick={close}
          >{$t('common.cancel')}</button
        >
        <button
          type="button"
          disabled={session.pending ||
            items.length === 0 ||
            $playerGold < total}
          onclick={reclaim}>{$t('impound.reclaim')}</button
        >
      </div>
    </div>
  </div>
{/if}

<style>
  .impound-backdrop {
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
  .impound-dialog {
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
  }
  .done {
    color: #b9d99a;
  }
  .error {
    color: #f0a08c;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li,
  .total {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 0;
  }
  .total {
    border-top: 1px solid #675b4e;
    font-weight: 700;
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
