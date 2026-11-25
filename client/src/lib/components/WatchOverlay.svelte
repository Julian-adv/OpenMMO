<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n'
  import { gameStore } from '../stores/gameStore'
  import { watchWaiting } from '../stores/watchStore'

  const POINTER = [
    'pointerdown',
    'mousedown',
    'click',
    'dblclick',
    'contextmenu',
  ]

  // The socket already refuses to send for the player. This keeps the scene
  // from acting out clicks and game hotkeys locally; panel clicks and the
  // browser's own keys still work.
  function swallowPointer(event: Event) {
    if (!(event.target instanceof HTMLCanvasElement)) return
    event.stopPropagation()
    event.preventDefault()
  }

  function swallowKey(event: Event) {
    event.stopPropagation()
  }

  onMount(() => {
    for (const type of POINTER)
      window.addEventListener(type, swallowPointer, true)
    window.addEventListener('keydown', swallowKey, true)
    return () => {
      for (const type of POINTER)
        window.removeEventListener(type, swallowPointer, true)
      window.removeEventListener('keydown', swallowKey, true)
    }
  })
</script>

<div class="watch-banner">
  <span class="label">{$t('watch.banner')}</span>
  {#if $gameStore.currentPlayer}
    <span class="name">{$gameStore.currentPlayer.name}</span>
  {/if}
  {#if $watchWaiting}
    <div class="waiting">{$t('watch.waiting')}</div>
  {/if}
</div>

<style>
  .watch-banner {
    position: fixed;
    top: 10px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 1000;
    padding: 6px 14px;
    border: 1px solid #63b3ed;
    border-radius: 6px;
    background: rgba(10, 20, 35, 0.8);
    color: #bee3f8;
    font-size: 13px;
    text-align: center;
    pointer-events: none;
  }

  .label {
    font-weight: 600;
    margin-right: 6px;
  }

  .waiting {
    margin-top: 4px;
    color: #ecc94b;
  }
</style>
