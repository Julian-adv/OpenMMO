<script lang="ts">
  import { locale, t } from '../i18n'
  import { skillTooltip } from '../actions/skillTooltip'
  import { formatRemaining } from '../data/debuffPresentation'
  import { visibleAbilityBuffs } from '../stores/abilityStore'
  import {
    characterPanelTab,
    characterPanelVisible,
  } from '../stores/debugStore'

  function openStatus() {
    characterPanelTab.set('status')
    characterPanelVisible.set(true)
  }
</script>

{#each $visibleAbilityBuffs as buff (buff.id)}
  {@const name = $t(`ability.${buff.id}.name`)}
  {@const description = $t(`ability.${buff.id}.buffDescription`)}
  {@const time = formatRemaining(buff.remaining, $locale)}
  <button
    class="buff-badge"
    aria-label={$t('status.openBuff', { name, description, time })}
    use:skillTooltip={{ name, description }}
    onclick={openStatus}
  >
    <img src={buff.icon} alt="" width="20" height="20" />
    <span>{time}</span>
  </button>
{/each}

<style>
  .buff-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 25px;
    padding: 1px 8px 1px 2px;
    border: 1px solid rgba(242, 223, 169, 0.35);
    border-radius: 6px;
    background: rgba(20, 16, 10, 0.8);
    color: #f2dfa9;
    font:
      12px/1.4 system-ui,
      sans-serif;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    cursor: pointer;
  }

  .buff-badge img {
    border-radius: 3px;
    object-fit: cover;
  }

  .buff-badge span {
    min-width: 3ch;
    text-align: center;
  }

  .buff-badge:hover {
    border-color: rgba(242, 223, 169, 0.7);
    background: rgba(38, 30, 16, 0.9);
  }

  .buff-badge:focus-visible {
    outline: 2px solid #f2dfa9;
    outline-offset: 2px;
  }
</style>
