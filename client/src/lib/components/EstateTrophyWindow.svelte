<script lang="ts">
  import './hudDialog.css'
  import { t, locale } from '../i18n'
  import { itemDisplayName } from '../data/itemDefs'
  import { hintTooltip } from '../actions/hintTooltip'
  import { shownTrophy, trophyRecords } from '../stores/estateTrophyStore'
  import EstateTrophyRecordLines from './EstateTrophyRecordLines.svelte'

  const record = $derived(
    $shownTrophy ? $trophyRecords.get($shownTrophy.itemId) : null
  )
</script>

{#if $shownTrophy && record}
  <div class="hud-dialog-layer">
    <div class="hud-dialog trophy-dialog" role="dialog">
      <div class="panel-header">
        <h2 class="panel-title">
          {itemDisplayName($shownTrophy.itemDefId, 0, $locale)}
        </h2>
        <button
          class="close-btn"
          use:hintTooltip={$t('common.close')}
          aria-label={$t('common.close')}
          onclick={() => shownTrophy.set(null)}>×</button
        >
      </div>
      <EstateTrophyRecordLines {record} />
    </div>
  </div>
{/if}

<style>
  .trophy-dialog {
    min-width: 240px;
    width: 360px;
  }
</style>
