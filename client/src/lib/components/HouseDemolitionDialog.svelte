<script lang="ts">
  import { tick } from 'svelte'
  import { mountOverlay } from '../stores/overlayStack'
  import type { HouseData } from '../types/housing'

  let {
    house,
    onConfirm,
    onCancel,
  }: {
    house: HouseData
    onConfirm: (houseId: string) => void
    onCancel: () => void
  } = $props()

  let dialog = $state<HTMLDivElement>()

  $effect(() => mountOverlay('houseDemolition', onCancel))
  $effect(() => {
    tick().then(() => dialog?.focus())
  })
</script>

<div class="demolition-backdrop">
  <div
    class="demolition-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="demolition-title"
    tabindex="-1"
    bind:this={dialog}
  >
    <div class="eyebrow">HOUSE DEMOLITION</div>
    <h2 id="demolition-title">Demolish this house?</h2>
    <div class="house-details">
      <span>{house.rooms.length} room{house.rooms.length === 1 ? '' : 's'}</span
      >
      <span>{house.origin.x.toFixed(0)}, {house.origin.z.toFixed(0)}</span>
    </div>
    <div class="warning">
      {#if house.sourceScrollId}
        <span>The construction scroll will be returned to your bag.</span>
      {:else}
        <strong>This cannot be undone.</strong>
        <span>The house will be removed permanently.</span>
      {/if}
    </div>
    <div class="actions">
      <button class="secondary" onclick={onCancel}>Keep house</button>
      <button class="danger" onclick={() => onConfirm(house.id)}
        >Demolish</button
      >
    </div>
  </div>
</div>

<style>
  .demolition-backdrop {
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
  .demolition-dialog {
    width: min(380px, calc(100vw - 32px));
    box-sizing: border-box;
    padding: 22px;
    border: 1px solid rgba(207, 112, 91, 0.7);
    border-radius: 12px;
    outline: none;
    background:
      linear-gradient(145deg, rgba(77, 38, 31, 0.28), transparent 45%), #18130f;
    color: #f3e8d2;
    box-shadow:
      0 22px 55px rgba(0, 0, 0, 0.55),
      inset 0 1px rgba(255, 255, 255, 0.05);
    font-family: 'Courier New', monospace;
  }
  .eyebrow {
    margin-bottom: 7px;
    color: #cf705b;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.18em;
  }
  h2 {
    margin: 0;
    color: #fff4df;
    font: 700 22px/1.25 sans-serif;
  }
  .house-details {
    display: flex;
    gap: 8px;
    margin: 15px 0;
  }
  .house-details span {
    padding: 5px 8px;
    border: 1px solid rgba(226, 185, 59, 0.24);
    border-radius: 999px;
    background: rgba(226, 185, 59, 0.08);
    color: #dbc98f;
    font-size: 11px;
  }
  .warning {
    display: grid;
    gap: 5px;
    padding: 12px;
    border-left: 3px solid #b95749;
    border-radius: 4px;
    background: rgba(145, 55, 44, 0.16);
    font: 12px/1.45 sans-serif;
  }
  .warning strong {
    color: #f0a08f;
  }
  .warning span {
    color: #c9bbb0;
  }
  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin-top: 20px;
  }
  button {
    padding: 9px 12px;
    border-radius: 7px;
    font-family: inherit;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }
  button.secondary {
    border: 1px solid #675b4e;
    background: #2b251f;
    color: #ddd1bf;
  }
  button.danger {
    border: 1px solid #d57966;
    background: #7e342b;
    color: #fff1eb;
  }
  button:hover {
    filter: brightness(1.12);
  }
</style>
