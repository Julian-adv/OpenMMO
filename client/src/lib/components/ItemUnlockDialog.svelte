<script lang="ts">
  import { t, locale } from '../i18n'
  import { onMount } from 'svelte'
  import { getItemDef, itemDisplayName } from '../data/itemDefs'
  import type { ItemInstance } from '../stores/inventoryStore'
  import { mountOverlay } from '../stores/overlayStack'

  let {
    item,
    onConfirm,
    onCancel,
  }: {
    item: ItemInstance
    onConfirm: () => void
    onCancel: () => void
  } = $props()

  const confirmationWord = 'UNLOCK'
  const def = $derived(getItemDef(item.item_def_id))
  let confirmation = $state('')
  let dialog: HTMLDialogElement
  let input: HTMLInputElement

  onMount(() => {
    const previousFocus = document.activeElement
    const unregister = mountOverlay('itemUnlock', onCancel)
    dialog.showModal()
    input.focus()
    return () => {
      dialog.close()
      unregister()
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected)
        previousFocus.focus({ preventScroll: true })
    }
  })

  function submit(event: SubmitEvent) {
    event.preventDefault()
    if (confirmation === confirmationWord) onConfirm()
  }
</script>

<dialog
  bind:this={dialog}
  aria-label={$t('inventory.unlockTitle')}
  aria-describedby="item-unlock-description"
  oncancel={(event) => {
    event.preventDefault()
    onCancel()
  }}
  onkeydown={(event) => event.stopPropagation()}
  onpointerdown={(event) => event.stopPropagation()}
  onclick={(event) => event.stopPropagation()}
  ondblclick={(event) => event.stopPropagation()}
>
  <form onsubmit={submit}>
    <h2>{$t('inventory.unlockTitle')}</h2>
    <div class="item-details">
      {#if def}
        <img src="/items/{def.icon}" alt="" draggable="false" />
      {/if}
      <strong>
        {itemDisplayName(
          item.item_def_id,
          item.enchant,
          $locale
        )}{item.quantity > 1 ? ` ×${item.quantity}` : ''}
      </strong>
    </div>
    <p id="item-unlock-description">
      {$t('inventory.unlockDescription')}
    </p>
    <label>
      <span>{$t('inventory.unlockConfirm', { word: confirmationWord })}</span>
      <input
        bind:this={input}
        bind:value={confirmation}
        type="text"
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
      />
    </label>
    <div class="actions">
      <button type="button" class="secondary" onclick={onCancel}
        >{$t('common.cancel')}</button
      >
      <button
        type="submit"
        class="primary"
        disabled={confirmation !== confirmationWord}
        >{$t('common.unlock')}</button
      >
    </div>
  </form>
</dialog>

<style>
  dialog {
    width: min(380px, calc(100vw - 32px));
    box-sizing: border-box;
    padding: 22px;
    border: 1px solid rgba(240, 192, 64, 0.5);
    border-radius: 10px;
    background: #101820;
    color: #e6edf3;
    font:
      13px/1.5 'Noto Sans KR',
      sans-serif;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    pointer-events: auto;
  }

  dialog::backdrop {
    background: rgba(6, 10, 16, 0.7);
  }

  form,
  label {
    display: grid;
    gap: 12px;
  }

  h2,
  p {
    margin: 0;
  }

  h2 {
    font-size: 18px;
  }

  p {
    color: #aebdcb;
  }

  .item-details {
    display: flex;
    align-items: center;
    gap: 10px;
    color: #f0c040;
    overflow-wrap: anywhere;
  }

  .item-details img {
    width: 40px;
    height: 40px;
    object-fit: contain;
    flex-shrink: 0;
  }

  label {
    gap: 6px;
  }

  input {
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
    padding: 9px 10px;
    border: 1px solid #61738a;
    border-radius: 5px;
    background: #080f16;
    color: #e6edf3;
    font: inherit;
    font-size: 16px;
  }

  input:focus-visible {
    outline: 2px solid #f0c040;
    outline-offset: 2px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  button {
    padding: 9px 16px;
    border: 1px solid #61738a;
    border-radius: 5px;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }

  .secondary {
    background: #1c2736;
    color: #dbe6f2;
  }

  .primary {
    border-color: #d2a63c;
    background: #604b20;
    color: #fff0c5;
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  button:enabled:hover {
    filter: brightness(1.2);
  }
</style>
