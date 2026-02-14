<script lang="ts">
  import type { CharacterAttributes } from '../network/socket'

  interface Props {
    attributes: CharacterAttributes
  }

  let { attributes }: Props = $props()
</script>

<div class="attribute-hud" aria-label="Character attributes">
  <span class="attr-item">
    <span class="attr-label">str</span>
    <span class="attr-value">{attributes.str}</span>
  </span>
  <span class="attr-item">
    <span class="attr-label">dex</span>
    <span class="attr-value">{attributes.dex}</span>
  </span>
  <span class="attr-item">
    <span class="attr-label">con</span>
    <span class="attr-value">{attributes.con}</span>
  </span>
  <span class="attr-item">
    <span class="attr-label">int</span>
    <span class="attr-value">{attributes.int}</span>
  </span>
  <span class="attr-item">
    <span class="attr-label">wis</span>
    <span class="attr-value">{attributes.wis}</span>
  </span>
  <span class="attr-item">
    <span class="attr-label">cha</span>
    <span class="attr-value">{attributes.cha}</span>
  </span>
</div>

<style>
  .attribute-hud {
    position: fixed;
    left: 50%;
    bottom: 16px;
    transform: translateX(-50%);
    z-index: 30;
    display: flex;
    gap: 10px;
    padding: 8px 12px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.78);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 13px;
    line-height: 1;
    pointer-events: none;
    backdrop-filter: blur(2px);
  }

  .attr-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    min-width: 38px;
  }

  .attr-label {
    white-space: nowrap;
    font-size: 11px;
    color: #9fb2c3;
    text-transform: lowercase;
    letter-spacing: 0.3px;
  }

  .attr-value {
    white-space: nowrap;
    font-size: 18px;
    font-weight: 700;
    color: #f5f9fc;
    line-height: 1;
  }
</style>
