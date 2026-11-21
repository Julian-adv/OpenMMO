<script lang="ts">
  import { t } from '../i18n'
  import { tick } from 'svelte'
  import QuantityStepper, { clampQuantity } from './QuantityStepper.svelte'

  interface Props {
    visible: boolean
    itemName: string
    icon: string
    max: number
    /** Pre-selected amount; defaults to `max`. */
    defaultQty?: number
    stepSize?: number
    onConfirm: (qty: number) => void
    onCancel: () => void
  }

  let {
    visible,
    itemName,
    icon,
    max,
    defaultQty,
    stepSize = 1,
    onConfirm,
    onCancel,
  }: Props = $props()

  let qty = $state(1)
  let inputEl = $state<HTMLInputElement | null>(null)

  $effect(() => {
    if (!visible) return
    qty = clampQuantity(defaultQty ?? max, max)
    tick().then(() => {
      if (!visible) return
      inputEl?.focus()
      inputEl?.select()
    })
  })

  function confirm() {
    onConfirm(clampQuantity(qty, max))
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') confirm()
    else if (e.key === 'Escape') onCancel()
  }
</script>

{#if visible}
  <div
    class="quantity-overlay"
    role="dialog"
    aria-label={$t('quantity.title')}
    tabindex="-1"
    onkeydown={onKeydown}
  >
    <div class="quantity-popup">
      <div class="popup-header">
        <img class="item-icon" src="/items/{icon}" alt="" draggable="false" />
        <span class="item-name">{itemName}</span>
      </div>
      <QuantityStepper bind:value={qty} {max} {stepSize} bind:inputEl />
      <div class="max-note">{$t('quantity.maximum', { max })}</div>
      <div class="popup-actions">
        <button class="cancel-btn" onclick={onCancel}
          >{$t('common.cancel')}</button
        >
        <button class="confirm-btn" onclick={confirm}
          >{$t('common.confirm')}</button
        >
      </div>
    </div>
  </div>
{/if}

<style>
  .quantity-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.35);
    pointer-events: auto;
  }

  .quantity-popup {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 8px;
    padding: 14px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.95);
    color: #e6edf3;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 12px;
    min-width: 200px;
  }

  .popup-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .item-icon {
    width: 28px;
    height: 28px;
    image-rendering: pixelated;
    flex-shrink: 0;
  }

  .item-name {
    font-weight: 700;
    color: #f0c040;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .max-note {
    text-align: center;
    color: #6b7d8d;
  }

  .popup-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .cancel-btn,
  .confirm-btn {
    border-radius: 4px;
    padding: 6px 14px;
    font-family: inherit;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }

  .cancel-btn {
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.2);
    color: #9fb2c3;
  }

  .cancel-btn:hover {
    color: #fff;
  }

  .confirm-btn {
    background: rgba(60, 90, 60, 0.85);
    color: #d6f0d6;
    border: 1px solid rgba(140, 220, 140, 0.35);
  }

  .confirm-btn:hover {
    background: rgba(80, 120, 80, 0.95);
    color: #fff;
  }
</style>
