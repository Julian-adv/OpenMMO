<script lang="ts">
  import { t } from '../i18n'
  import { networkManager } from '../network/socket'
  import { watchDuty } from '../stores/watchDutyStore'

  let confirming = $state(false)
</script>

{#if $watchDuty}
  {@const duty = $watchDuty}
  <div class="watch-duty" class:marked={duty.answered < duty.calls}>
    <span>
      {$t(`watch.stage.${duty.stage}`)}{duty.calls > 0
        ? ` · ${$t('watch.rollCalls', {
            answered: duty.answered,
            calls: duty.calls,
          })}`
        : ''}
    </span>
    {#if duty.gear === 'unissued' && duty.stage === 'waiting'}
      <span class="hint">{$t('watch.gear.take')}</span>
    {:else if duty.gear === 'issued' && duty.stage === 'done'}
      <span class="hint">{$t('watch.gear.return')}</span>
    {/if}
    {#if duty.stage !== 'done'}
      {#if confirming}
        <span class="hint">{$t('watch.quitAsk')}</span>
        <button
          class="quit"
          onclick={() => {
            confirming = false
            networkManager.sendWatchShiftQuit()
          }}>{$t('watch.quitYes')}</button
        >
        <button onclick={() => (confirming = false)}
          >{$t('watch.quitNo')}</button
        >
      {:else}
        <button class="quit" onclick={() => (confirming = true)}
          >{$t('watch.quit')}</button
        >
      {/if}
    {/if}
    {#if duty.stage === 'on_watch'}
      <button onclick={() => networkManager.sendWatchToolUse('spear')}>
        {$t('watch.clearRoad')}
      </button>
      {#if duty.shovel}
        <button onclick={() => networkManager.sendWatchToolUse('shovel')}>
          {$t('watch.shovelSnow')}
        </button>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .watch-duty {
    position: fixed;
    z-index: 45;
    top: 84px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 10px;
    width: max-content;
    max-width: calc(100vw - 24px);
    box-sizing: border-box;
    padding: 6px 12px;
    border-radius: 10px;
    border: 1px solid rgba(143, 196, 150, 0.5);
    background: rgba(6, 10, 14, 0.88);
    color: #e6edf3;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 13px;
    pointer-events: auto;
  }
  .watch-duty .hint {
    color: #ffe3a3;
  }
  .watch-duty button.quit {
    border-color: rgba(240, 160, 140, 0.55);
    background: rgba(240, 160, 140, 0.1);
    color: #f0b8a8;
  }
  .watch-duty.marked {
    border-color: rgba(255, 196, 92, 0.6);
  }
  .watch-duty button {
    cursor: pointer;
    background: rgba(143, 196, 150, 0.1);
    border: 1px solid rgba(143, 196, 150, 0.5);
    border-radius: 4px;
    padding: 4px 8px;
    font-family: inherit;
    font-size: inherit;
    color: #bfe3c4;
  }
</style>
