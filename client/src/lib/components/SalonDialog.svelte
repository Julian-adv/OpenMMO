<script lang="ts">
  import { Canvas } from '@threlte/core'
  import { devicePixelRatio } from 'svelte/reactivity/window'
  import { t, type MessageKey } from '../i18n'
  import { gameStore } from '../stores/gameStore'
  import { playerGold } from '../stores/inventoryStore'
  import { salonSession, requestRestyle } from '../stores/salonStore'
  import { mountOverlay } from '../stores/overlayStack'
  import { draggablePanel } from '../actions/draggablePanel'
  import { formatGold } from '../utils/currency'
  import { createPreviewWebGPURenderer } from '../utils/renderer'
  import {
    getEffectivePreset,
    graphicsQuality,
  } from '../stores/graphicsSettings'
  import { getCharacterModelPath, isModularModel } from '../utils/modelPaths'
  import { salon_price } from '../wasm/onlinerpg_shared'
  import type { CharacterAppearance } from '../network/networkTypes'
  import AppearanceFields from './AppearanceFields.svelte'
  import SalonPreviewScene from './SalonPreviewScene.svelte'

  const session = $derived($salonSession)
  const player = $derived($gameStore.currentPlayer)
  const current = $derived(player?.appearance)
  let draft = $state<CharacterAppearance | null>(null)
  // The saved look is the starting point, again after each paid restyle.
  $effect(() => {
    if (current) draft = { ...current }
  })

  const styled = $derived(
    !!player &&
      isModularModel(
        getCharacterModelPath(player.characterClass, player.gender)
      )
  )
  const PARTS = [
    ['characterCreate.face', ['face']],
    ['characterCreate.hair', ['hair']],
    ['characterCreate.hairColor', ['hair_color', 'hair_color_original']],
    ['characterCreate.eyeColor', ['eye_color', 'eye_color_original']],
  ] as const satisfies readonly (readonly [
    MessageKey,
    readonly (keyof CharacterAppearance)[],
  ])[]
  // Each part priced on its own, so the lines add up to what the server charges.
  const lines = $derived.by(() => {
    if (!current || !draft) return []
    const wanted = draft
    return PARTS.map(([label, keys]) => {
      const only = { ...current }
      for (const key of keys) Object.assign(only, { [key]: wanted[key] })
      return { label, price: salon_price(current, only) }
    }).filter((line) => line.price > 0)
  })
  const total = $derived(lines.reduce((sum, line) => sum + line.price, 0))
  const short = $derived(total > $playerGold)
  const dpr = $derived(
    Math.min(
      devicePixelRatio.current ?? window.devicePixelRatio,
      getEffectivePreset($graphicsQuality).pixelRatioCap
    )
  )
  const close = () => salonSession.set(null)

  $effect(() => mountOverlay('salon', close))
</script>

{#if session && player && current && draft}
  <div
    class="salon-dialog"
    role="dialog"
    aria-labelledby="salon-title"
    use:draggablePanel={'salon'}
  >
    <header data-drag-handle>
      <h2 id="salon-title">{$t('salon.title')}</h2>
      <button
        type="button"
        class="close-btn"
        onclick={close}
        aria-label={$t('common.close')}>×</button
      >
    </header>
    {#if !styled}
      <p class="hint">{$t('salon.error.unsupported')}</p>
    {:else}
      <div class="salon-body">
        <div class="preview">
          <Canvas
            createRenderer={createPreviewWebGPURenderer}
            {dpr}
            shadows={false}
          >
            <SalonPreviewScene
              characterClass={player.characterClass}
              gender={player.gender}
              appearance={draft}
            />
          </Canvas>
        </div>
        <div class="choices">
          <AppearanceFields
            gender={player.gender}
            appearance={draft}
            disabled={session.pending}
            onChange={(appearance) => (draft = appearance)}
          />
        </div>
      </div>
      {#if lines.length === 0}
        <p class="hint">
          {session.paid === null
            ? $t('salon.intro')
            : $t('salon.done', { price: formatGold(session.paid) })}
        </p>
      {:else}
        <ul>
          {#each lines as line (line.label)}
            <li>
              <span>{$t(line.label)}</span>
              <span>{formatGold(line.price)}</span>
            </li>
          {/each}
        </ul>
        <div class="total">
          <span>{$t('salon.total')}</span>
          <span>{formatGold(total)}</span>
        </div>
      {/if}
      {#if session.error}
        <p class="error" role="alert">
          {$t(`salon.error.${session.error}`)}
        </p>
      {:else if short}
        <p class="error">{$t('salon.error.not_enough_gold')}</p>
      {/if}
    {/if}
    <div class="actions">
      <button type="button" class="secondary" onclick={close}
        >{$t('common.close')}</button
      >
      {#if styled}
        <button
          type="button"
          class="secondary"
          disabled={session.pending || total === 0}
          onclick={() => (draft = { ...current })}>{$t('salon.reset')}</button
        >
        <button
          type="button"
          disabled={session.pending || total === 0 || short}
          onclick={() => draft && requestRestyle(draft)}
          >{$t('salon.confirm')}</button
        >
      {/if}
    </div>
  </div>
{/if}

<style>
  /* Lower right, clear of the mirrors. */
  .salon-dialog {
    position: fixed;
    right: 24px;
    bottom: 96px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    box-sizing: border-box;
    padding: 20px;
    border: 1px solid rgba(200, 184, 138, 0.7);
    border-radius: 12px;
    background: #18160f;
    color: #f3e8d2;
    box-shadow: 0 22px 55px rgba(0, 0, 0, 0.55);
    font:
      12px/1.5 'Noto Sans KR',
      sans-serif;
    pointer-events: auto;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  h2 {
    margin: 0;
    color: #fff4df;
    font-size: 18px;
  }
  button.close-btn {
    padding: 0 8px;
    border-color: transparent;
    background: transparent;
    font-size: 20px;
    line-height: 1.2;
  }
  p {
    margin: 0;
  }
  .salon-body {
    display: flex;
    flex: none;
    gap: 14px;
  }
  .preview {
    flex: 1 1 0;
    min-width: 0;
    height: 300px;
    border: 1px solid rgba(200, 184, 138, 0.3);
    border-radius: 8px;
    background: #10151c;
    pointer-events: none;
  }
  .choices {
    flex: none;
    max-height: 300px;
    overflow-y: auto;
    padding-right: 8px;
    scrollbar-width: thin;
    scrollbar-color: rgba(200, 184, 138, 0.55) transparent;
  }
  @supports selector(::-webkit-scrollbar) {
    .choices {
      scrollbar-width: auto;
      scrollbar-color: auto;
    }
    .choices::-webkit-scrollbar {
      width: 6px;
    }
    .choices::-webkit-scrollbar-track {
      background: transparent;
    }
    .choices::-webkit-scrollbar-thumb {
      border-radius: 999px;
      background: rgba(200, 184, 138, 0.45);
    }
    .choices::-webkit-scrollbar-thumb:hover {
      background: rgba(200, 184, 138, 0.75);
    }
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li,
  .total {
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }
  .total {
    padding-top: 6px;
    border-top: 1px solid rgba(200, 184, 138, 0.3);
    font-weight: 700;
  }
  .hint {
    color: #c9bda3;
  }
  .error {
    color: #ff9c8a;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  button {
    padding: 7px 14px;
    border: 1px solid #c8b88a;
    border-radius: 7px;
    background: #6b4f1d;
    color: #fff4df;
    cursor: pointer;
  }
  button.secondary {
    background: transparent;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  @media (max-width: 520px) {
    .salon-body {
      flex-direction: column;
    }
    .preview {
      flex: none;
      height: 200px;
    }
    .choices {
      max-height: 220px;
    }
  }
</style>
