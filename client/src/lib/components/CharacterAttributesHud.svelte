<script lang="ts">
  import type { CharacterAttributes } from '../network/socket'

  interface Props {
    level: number
    currentXp: number
    currentHp: number
    maxHp: number
    attributes: CharacterAttributes
  }

  const MAX_SAFE_XP = Number.MAX_SAFE_INTEGER

  function xpForLevel(level: number) {
    if (level <= 1) {
      return 0
    }

    const shift = level - 2
    if (shift >= 53) {
      return MAX_SAFE_XP
    }

    const threshold = 20 * 2 ** shift
    if (!Number.isFinite(threshold)) {
      return MAX_SAFE_XP
    }

    return Math.min(MAX_SAFE_XP, Math.floor(threshold))
  }

  function clamp(value: number, min: number, max: number) {
    return Math.min(max, Math.max(min, value))
  }

  let { level, currentXp, currentHp, maxHp, attributes }: Props = $props()

  const levelStartXp = $derived(xpForLevel(level))
  const nextLevelXp = $derived(xpForLevel(level + 1))
  const neededXp = $derived(Math.max(1, nextLevelXp - levelStartXp))
  const gainedXp = $derived(clamp(currentXp - levelStartXp, 0, neededXp))
  const expProgress = $derived(gainedXp / neededXp)
  const expPercent = $derived(Math.round(expProgress * 100))
  const guard = $derived(attributes.guard)
</script>

<div class="attribute-hud" aria-label="Character attributes">
  <div class="attribute-main">
    <div class="meta-block">
      <span class="attr-item level-item">
        <span class="attr-label">Lv</span>
        <span class="attr-value level-value">{level}</span>
      </span>
      <span class="attr-item hp-item">
        <span class="attr-label">Hp</span>
        <span class="attr-value hp-value">{currentHp}/{maxHp}</span>
      </span>
    </div>
    <span class="attr-separator"></span>
    <span class="attr-item">
      <span class="attr-label">Str</span>
      <span class="attr-value">{attributes.str}</span>
    </span>
    <span class="attr-item">
      <span class="attr-label">Dex</span>
      <span class="attr-value">{attributes.dex}</span>
    </span>
    <span class="attr-item">
      <span class="attr-label">Con</span>
      <span class="attr-value">{attributes.con}</span>
    </span>
    <span class="attr-item">
      <span class="attr-label">Int</span>
      <span class="attr-value">{attributes.int}</span>
    </span>
    <span class="attr-item">
      <span class="attr-label">Wis</span>
      <span class="attr-value">{attributes.wis}</span>
    </span>
    <span class="attr-item">
      <span class="attr-label">Cha</span>
      <span class="attr-value">{attributes.cha}</span>
    </span>
    <span class="attr-item">
      <span class="attr-label">Guard</span>
      <span class="attr-value">{guard}</span>
    </span>
  </div>
  <div class="exp-block" aria-label="Experience progress">
    <div class="exp-header">
      <span class="attr-label exp-label">Exp</span>
      <span class="exp-text">{gainedXp}/{neededXp} ({expPercent}%)</span>
    </div>
    <div class="exp-track" role="progressbar" aria-valuemin={0} aria-valuemax={neededXp} aria-valuenow={gainedXp}>
      <span class="exp-fill" style={`width: ${Math.min(100, expProgress * 100)}%`}></span>
    </div>
  </div>
</div>

<style>
  .attribute-hud {
    position: fixed;
    left: 50%;
    bottom: 16px;
    transform: translateX(-50%);
    z-index: 30;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 8px;
    pointer-events: none;
    backdrop-filter: blur(2px);
    padding: 8px 12px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.78);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 13px;
    line-height: 1;
  }

  .attribute-main {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .meta-block {
    display: grid;
    grid-template-columns: auto auto;
    column-gap: 10px;
    row-gap: 6px;
    align-items: end;
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
    letter-spacing: 0.3px;
  }

  .attr-value {
    white-space: nowrap;
    font-size: 18px;
    font-weight: 700;
    color: #f5f9fc;
    line-height: 1;
  }

  .level-value {
    color: #f0c040;
  }

  .hp-value {
    color: #6ee7b7;
    font-size: 16px;
  }

  .exp-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
    min-width: 0;
  }

  .exp-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }

  .exp-label {
    color: #9fc5ff;
  }

  .exp-text {
    font-size: 11px;
    color: #d5e5f6;
  }

  .exp-track {
    position: relative;
    height: 7px;
    border-radius: 999px;
    overflow: hidden;
    background: rgba(64, 98, 135, 0.45);
    border: 1px solid rgba(166, 200, 238, 0.25);
  }

  .exp-fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: linear-gradient(90deg, #58a6ff 0%, #7fd0ff 100%);
    box-shadow: 0 0 10px rgba(88, 166, 255, 0.4);
  }

  .attr-separator {
    width: 1px;
    background: rgba(255, 255, 255, 0.15);
    align-self: stretch;
  }
</style>
