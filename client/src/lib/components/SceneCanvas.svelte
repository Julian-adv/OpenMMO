<script lang="ts">
  import { Canvas } from '@threlte/core'
  import { PCFShadowMap } from 'three'
  import type { ComponentProps } from 'svelte'
  import type GameSceneComponent from './GameScene.svelte'
  import CharacterSelectScene from './CharacterSelectScene.svelte'
  import CharacterCreateScene from './CharacterCreateScene.svelte'
  import RenderFrameLimiter from './RenderFrameLimiter.svelte'
  import { createWebGPURenderer } from '../utils/renderer'
  import { releaseFemaleCloseup } from '../utils/femaleCharacter'

  interface Props {
    screen: 'login' | 'character-select' | 'character-create' | 'game'
    selection: ComponentProps<typeof CharacterSelectScene>
    creation: ComponentProps<typeof CharacterCreateScene>
    game: Pick<
      ComponentProps<typeof GameSceneComponent>,
      'serverUrl' | 'onCurrentPlayerDyingFinished'
    >
    GameScene?: typeof GameSceneComponent
    isCurrentPlayerLoading: boolean
    isSceneCompiling: boolean
  }

  let {
    screen,
    selection,
    creation,
    game,
    GameScene,
    isCurrentPlayerLoading = $bindable(false),
    isSceneCompiling = $bindable(true),
  }: Props = $props()

  // The selection and creation scenes are gone a frame after the game screen takes over.
  $effect(() => {
    if (screen !== 'game') return
    const frame = requestAnimationFrame(releaseFemaleCloseup)
    return () => cancelAnimationFrame(frame)
  })
</script>

<Canvas
  renderMode="manual"
  shadows={PCFShadowMap}
  createRenderer={createWebGPURenderer}
>
  {#if screen !== 'game'}
    <RenderFrameLimiter />
  {/if}
  {#if screen === 'character-select'}
    <CharacterSelectScene {...selection} />
  {:else if screen === 'character-create'}
    <CharacterCreateScene {...creation} />
  {:else if screen === 'game' && GameScene}
    <GameScene {...game} bind:isCurrentPlayerLoading bind:isSceneCompiling />
  {/if}
</Canvas>
