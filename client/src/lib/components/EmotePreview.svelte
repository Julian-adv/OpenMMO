<script lang="ts">
  import { Canvas } from '@threlte/core'
  import { devicePixelRatio } from 'svelte/reactivity/window'
  import EmotePreviewScene from './EmotePreviewScene.svelte'
  import { createPreviewWebGPURenderer } from '../utils/renderer'
  import {
    getEffectivePreset,
    graphicsQuality,
  } from '../stores/graphicsSettings'
  import type { CharacterClass, Gender } from '../network/networkTypes'

  interface Props {
    anim: string | null
    label: string | null
    characterClass: CharacterClass
    gender: Gender
    /** Which side of the panel the box hangs off. */
    side?: 'left' | 'right'
  }

  let { anim, label, characterClass, gender, side = 'left' }: Props = $props()

  let playing = $state(false)

  // Update resolution when display density or graphics quality changes.
  const dpr = $derived(
    Math.min(
      devicePixelRatio.current ?? window.devicePixelRatio,
      getEffectivePreset($graphicsQuality).pixelRatioCap
    )
  )
</script>

<!-- Warm the renderer before showing the preview. -->
<div
  class="emote-preview"
  class:shown={anim !== null && playing}
  class:right-side={side === 'right'}
  aria-hidden="true"
>
  <div class="preview-canvas">
    <Canvas createRenderer={createPreviewWebGPURenderer} {dpr} shadows={false}>
      <EmotePreviewScene {anim} {characterClass} {gender} bind:playing />
    </Canvas>
  </div>
  {#if label}
    <div class="preview-label">{label}</div>
  {/if}
</div>

<style>
  .emote-preview {
    position: absolute;
    right: calc(100% + 10px);
    top: 0;
    width: 170px;
    display: flex;
    flex-direction: column;
    visibility: hidden;
    backdrop-filter: blur(4px);
    padding: 8px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
  }

  /* Override GameHud's pointer events, including while hidden. */
  .emote-preview,
  .emote-preview :global(*) {
    pointer-events: none !important;
  }

  .emote-preview.right-side {
    right: auto;
    left: calc(100% + 10px);
  }

  .emote-preview.shown {
    visibility: visible;
  }

  .preview-canvas {
    width: 100%;
    height: 210px;
  }

  .preview-label {
    margin-top: 6px;
    color: #8fe08f;
    font-family: inherit;
    font-size: 12px;
    font-weight: 700;
    text-align: center;
  }
</style>
