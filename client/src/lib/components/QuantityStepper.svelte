<script lang="ts" module>
  export function clampQuantity(n: number, max: number): number {
    if (!Number.isFinite(n)) return 1
    return Math.min(max, Math.max(1, Math.round(n)))
  }
</script>

<script lang="ts">
  import { t } from '../i18n'

  interface Props {
    value: number
    max: number
    stepSize?: number
    /** A shorter row for dense dialogs. */
    compact?: boolean
    inputEl?: HTMLInputElement | null
  }

  let {
    value = $bindable(),
    max,
    stepSize = 1,
    compact = false,
    inputEl = $bindable(null),
  }: Props = $props()

  function onInput(e: Event) {
    value = clampQuantity(Number((e.target as HTMLInputElement).value), max)
  }

  function step(delta: number) {
    if (stepSize > 1) {
      value = clampQuantity(value + delta, max)
      return
    }
    const zeroBased = value - 1 + delta
    value = (((zeroBased % max) + max) % max) + 1
  }

  function onWheel(event: WheelEvent) {
    if (stepSize === 1 || event.ctrlKey || event.deltaY === 0) return
    event.preventDefault()
    event.stopPropagation()
    step(event.deltaY < 0 ? stepSize : -stepSize)
  }
</script>

<div class="qty-row" class:compact onwheel={onWheel}>
  <button
    type="button"
    class="step-btn"
    aria-label={$t('quantity.decrease', { step: stepSize })}
    onclick={() => step(-stepSize)}>−</button
  >
  <input
    bind:this={inputEl}
    class="qty-input"
    type="number"
    min="1"
    step="1"
    {max}
    {value}
    oninput={onInput}
  />
  <button
    type="button"
    class="step-btn"
    aria-label={$t('quantity.increase', { step: stepSize })}
    onclick={() => step(stepSize)}>+</button
  >
</div>

<style>
  .qty-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .step-btn {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    line-height: 1;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    color: #e6edf3;
    font-size: 16px;
    font-weight: 700;
    cursor: pointer;
    flex-shrink: 0;
  }

  .step-btn:hover {
    background: rgba(255, 255, 255, 0.16);
  }

  .qty-input {
    flex: 1;
    min-width: 0;
    text-align: center;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    color: #e6edf3;
    font-family: inherit;
    font-size: 14px;
    padding: 6px 4px;
    appearance: textfield;
  }

  .qty-input::-webkit-inner-spin-button,
  .qty-input::-webkit-outer-spin-button {
    appearance: none;
    margin: 0;
  }

  .compact {
    width: 150px;
  }

  .compact .step-btn {
    width: 24px;
    height: 24px;
    font-size: 14px;
  }

  .compact .qty-input {
    box-sizing: border-box;
    height: 26px;
    padding: 0 4px;
    font-size: 12px;
  }

  @media (max-width: 600px), (pointer: coarse) {
    .step-btn {
      width: 40px;
      height: 40px;
    }
  }
</style>
