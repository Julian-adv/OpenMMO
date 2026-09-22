<script lang="ts">
  import { locale, t } from '../i18n'
  import {
    emotePanelVisible,
    emoteStopRequest,
    localEmoteAnim,
    MUSIC_EMOTE_ANIM,
  } from '../stores/emoteStore'
  import {
    EMOTE_LIST,
    emoteClickCommand,
    emoteLabel,
    type EmoteIntent,
    type EmoteMeta,
  } from '../emote-meta'
  import { gameStore } from '../stores/gameStore'
  import { networkManager } from '../network/socket'
  import { innerWidth } from 'svelte/reactivity/window'
  import { draggablePanel } from '../actions/draggablePanel'
  import { draggingPanel, panelPositions } from '../stores/panelLayout'
  import {
    getEffectivePreset,
    graphicsQuality,
  } from '../stores/graphicsSettings'
  import { EMOTE_PANEL_W, previewPlacement } from '../emote-preview-layout'
  import EmotePreview from './EmotePreview.svelte'

  const visible = $derived($emotePanelVisible)
  const active = $derived($localEmoteAnim)
  const previewPlayer = $derived($gameStore.currentPlayer)

  // Keep the last preview playing after the pointer leaves.
  let previewed = $state<EmoteMeta | null>(null)

  // Keep the preview mounted once it fits during this open.
  let everFit = $state(false)
  $effect(() => {
    if (!visible) {
      previewed = null
      everFit = false
      return
    }
    // Clear the preview on death or disconnect.
    if (!usable) previewed = null
    if (placement.fits && affordable) everFit = true
  })

  // Only full render budgets can afford a second WebGPU device.
  const affordable = $derived(
    $graphicsQuality !== 'low' &&
      getEffectivePreset($graphicsQuality).renderBudget === 'full'
  )

  // Find room for the preview beside the panel.
  const placement = $derived(
    previewPlacement(
      $panelPositions.emotes?.x ??
        (innerWidth.current ?? 0) - EMOTE_PANEL_W - 16,
      innerWidth.current ?? 0
    )
  )

  // Hide while dragging until the saved position updates.
  const dragging = $derived($draggingPanel === 'emotes')

  // Disable emotes while dead or disconnected.
  const usable = $derived(
    $gameStore.isConnected && ($gameStore.currentPlayer?.health ?? 0) > 0
  )

  // Retain pending intent until the server confirms it or it expires.
  let intent = $state<EmoteIntent | null>(null)
  $effect(() => {
    if (intent && active === intent.anim) intent = null
  })

  function play(emote: EmoteMeta) {
    // Clicking the running looping emote stops it, like Escape does.
    if (
      emoteClickCommand(emote, active, intent, performance.now()) === 'stop'
    ) {
      emoteStopRequest.set(true)
      intent = { anim: null, at: performance.now() }
      return
    }
    // The server validates the command and broadcasts the animation.
    networkManager.sendChatMessage(`/emote ${emote.anim}`)
    intent = { anim: emote.anim, at: performance.now() }
  }

  // Preview the strum pose for the live instrument panel.
  const INSTRUMENT_ROW: EmoteMeta = {
    anim: MUSIC_EMOTE_ANIM,
    label: 'Play Instrument',
    loops: true,
  }

  function playInstrument() {
    networkManager.sendStartInstrument()
  }
</script>

{#if visible}
  <div
    class="emote-panel"
    aria-label={$t('emotes.title')}
    use:draggablePanel={'emotes'}
  >
    <div class="panel-header" data-drag-handle>
      <span class="panel-title">{$t('emotes.title')}</span>
      <button
        class="close-btn"
        title={$t('common.close')}
        aria-label={$t('common.close')}
        onclick={() => emotePanelVisible.set(false)}>×</button
      >
    </div>

    <div class="emote-rows">
      {#each EMOTE_LIST as emote (emote.anim)}
        <button
          class="emote-row"
          class:active={active === emote.anim}
          disabled={!usable}
          title={emote.loops && active === emote.anim
            ? $t('common.stop')
            : `/emote ${emote.anim}`}
          onclick={() => play(emote)}
          onpointerenter={() => (previewed = emote)}
          onfocus={() => (previewed = emote)}
        >
          <span class="emote-label">{emoteLabel(emote, $locale)}</span>
          {#if emote.loops}
            <span class="loop-badge" title={$t('emotes.loopsHint')}>↻</span>
          {/if}
        </button>
      {/each}

      <button
        class="emote-row instrument-row"
        class:active={active === MUSIC_EMOTE_ANIM}
        disabled={!usable}
        title="/play_instrument"
        onclick={playInstrument}
        onpointerenter={() => (previewed = INSTRUMENT_ROW)}
        onfocus={() => (previewed = INSTRUMENT_ROW)}
      >
        <span class="instrument-mark" aria-hidden="true">♪</span>
        <span class="emote-label">{$t('emotes.playInstrument')}</span>
      </button>
    </div>

    <div class="panel-hint">{$t('emotes.commandHint')}</div>

    {#if previewPlayer && everFit && affordable}
      <EmotePreview
        anim={usable && placement.fits && !dragging
          ? (previewed?.anim ?? null)
          : null}
        label={previewed ? emoteLabel(previewed, $locale) : null}
        characterClass={previewPlayer.characterClass}
        gender={previewPlayer.gender}
        side={placement.side}
      />
    {/if}
  </div>
{/if}

<style>
  .emote-panel {
    position: fixed;
    right: 16px;
    /* Open above the HUD corner buttons. */
    bottom: 64px;
    z-index: 40;
    width: 180px;
    display: flex;
    flex-direction: column;
    backdrop-filter: blur(4px);
    padding: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    color: #e6edf3;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 12px;
    pointer-events: auto;
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
    margin-bottom: 8px;
  }

  .panel-title {
    flex: 1;
    font-size: 14px;
    font-weight: 700;
    color: #8fe08f;
  }

  .close-btn {
    background: none;
    border: none;
    color: #9fb2c3;
    font-size: 18px;
    cursor: pointer;
    padding: 0 2px;
    line-height: 1;
  }

  .close-btn:hover {
    color: #fff;
  }

  .emote-rows {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .emote-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 6px;
    color: #cfd9e3;
    font-family: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    transition:
      background 150ms ease,
      border-color 150ms ease;
  }

  .emote-row:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.35);
    color: #fff;
  }

  .emote-row.active {
    border-color: rgba(88, 255, 88, 0.7);
    box-shadow: 0 0 8px rgba(88, 255, 88, 0.35);
    color: #8fe08f;
  }

  .emote-row:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .emote-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .loop-badge {
    color: #7f8f9f;
    font-size: 11px;
    line-height: 1;
  }

  .emote-row.active .loop-badge {
    color: #8fe08f;
  }

  .panel-hint {
    margin-top: 8px;
    padding-top: 6px;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
    color: #7f8f9f;
    font-size: 10px;
    text-align: center;
  }

  .instrument-row {
    border-color: rgba(77, 238, 220, 0.3);
    color: #a9fff5;
  }

  .instrument-row:hover:not(:disabled) {
    border-color: rgba(77, 238, 220, 0.7);
    background: rgba(38, 168, 156, 0.15);
    color: #d9fffa;
  }

  .instrument-mark {
    font-size: 13px;
    line-height: 1;
  }
</style>
