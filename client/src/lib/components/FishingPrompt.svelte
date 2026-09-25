<script lang="ts">
  import {
    myFishing,
    fishingTargeting,
    cancelFishingTargeting,
  } from '../stores/fishingStore'
  import { FISHING, abilityEquipmentAllowed } from '../data/abilities'
  import { gameStore } from '../stores/gameStore'
  import { inventoryStore } from '../stores/inventoryStore'
  import { skillsStore } from '../stores/skillsStore'
  import { currentDungeonDepth } from '../stores/dungeonStore'
  import { playerVisualFloorLevel } from '../stores/housingStore'
  import {
    teleportLoading,
    mapEditorMode,
    housingEditorMode,
  } from '../stores/debugStore'
  import { landscapingMode } from '../stores/landscapingStore'
  import { estateFurnitureEditorActive } from '../stores/estateFurniturePlacementStore'
  import { mountOverlay } from '../stores/overlayStack'
  import SkillTargetHint from './SkillTargetHint.svelte'
  import type { FishingAction } from '../network/networkTypes'
  import { networkManager } from '../network/socket'
  import { playFishingSound } from '../managers/sfxManager'
  import { isTypingTarget } from '../utils/dom'
  import {
    fishing_trophy_min_tension,
    max_cast_distance_m,
  } from '../wasm/onlinerpg_shared'

  $effect(() => {
    if (!$fishingTargeting) return
    const player = $gameStore.currentPlayer
    if (
      !player ||
      !$gameStore.isConnected ||
      player.health <= 0 ||
      (player.mount != null && player.mount !== 'rowboat') ||
      !$skillsStore.learned.includes(FISHING.id) ||
      !abilityEquipmentAllowed(FISHING.id, $inventoryStore.equipped) ||
      $myFishing.phase !== 'idle' ||
      $currentDungeonDepth > 0 ||
      $playerVisualFloorLevel !== 0 ||
      $teleportLoading ||
      $mapEditorMode ||
      $housingEditorMode ||
      $landscapingMode ||
      $estateFurnitureEditorActive
    )
      cancelFishingTargeting()
  })

  $effect(() => {
    if ($fishingTargeting)
      return mountOverlay('fishingTarget', cancelFishingTargeting)
  })

  type FightStance = Exclude<FishingAction, 'hook'>

  const WHEEL_BURST_MS = 350
  const TROPHY_MIN_TENSION = fishing_trophy_min_tension()

  const STANCE_BUTTONS = [
    { stance: 'reel', label: 'REEL IN', hint: 'hold · SPACE · wheel ↓' },
    { stance: 'giveline', label: 'GIVE LINE', hint: 'hold · S · wheel ↑' },
  ] as const

  let stance = $state<FightStance>('hold')
  /** Which input currently owns the stance ('key-space', 'ptr', 'wheel'…);
   *  releasing an input only drops to hold if it still owns it. */
  let holder: string | null = null
  let wheelTimer: ReturnType<typeof setTimeout> | undefined

  const fighting = $derived($myFishing.phase === 'fight')
  const biting = $derived($myFishing.phase === 'bite')

  function sendStance(next: FightStance) {
    if (stance === next) return
    stance = next
    networkManager.sendFishingRespond(next)
    if (next === 'reel') playFishingSound('reel')
  }

  function press(source: string, next: Exclude<FightStance, 'hold'>) {
    holder = source
    sendStance(next)
  }

  function release(source: string) {
    if (holder !== source) return
    holder = null
    sendStance('hold')
  }

  function wheelBurst(next: Exclude<FightStance, 'hold'>) {
    press('wheel', next)
    clearTimeout(wheelTimer)
    wheelTimer = setTimeout(() => release('wheel'), WHEEL_BURST_MS)
  }

  // The fight ended (or never was): drop any held stance without sending.
  $effect(() => {
    if (!fighting) {
      stance = 'hold'
      holder = null
      clearTimeout(wheelTimer)
    }
  })

  // Wheel control while fighting: wheel down winds the reel (pulling toward
  // you), wheel up pays line out. Non-passive so preventDefault can stop
  // page scroll/zoom gestures.
  $effect(() => {
    if (!fighting) return
    const onWheel = (event: WheelEvent) => {
      event.preventDefault()
      if (event.deltaY > 0) wheelBurst('reel')
      else if (event.deltaY < 0) wheelBurst('giveline')
    }
    window.addEventListener('wheel', onWheel, { passive: false })
    return () => window.removeEventListener('wheel', onWheel)
  })

  // During the bite window a canvas click or a wheel flick means "hook it".
  // The click is captured before the canvas so it can't double as a walk
  // command that aborts the session; UI clicks (buttons, chat) pass through
  // untouched — the HOOK button has its own handler.
  $effect(() => {
    if (!biting) return
    const hook = (event: Event) => {
      event.preventDefault()
      networkManager.sendFishingRespond('hook')
    }
    const onPointerDown = (event: PointerEvent) => {
      if (!(event.target instanceof HTMLCanvasElement)) return
      event.stopPropagation()
      hook(event)
    }
    const onWheel = (event: WheelEvent) => hook(event)
    window.addEventListener('pointerdown', onPointerDown, { capture: true })
    window.addEventListener('wheel', onWheel, { passive: false })
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, {
        capture: true,
      })
      window.removeEventListener('wheel', onWheel)
    }
  })

  function onKeydown(event: KeyboardEvent) {
    // OS key-repeat must not churn the stance.
    if (event.repeat) return
    const phase = $myFishing.phase
    if (phase === 'idle') return
    if (isTypingTarget(event.target)) return
    if (event.code === 'Space') {
      event.preventDefault()
      if (phase === 'bite') networkManager.sendFishingRespond('hook')
      else if (phase === 'fight') press('key-space', 'reel')
    } else if (event.code === 'KeyS') {
      if (phase === 'fight') {
        event.preventDefault()
        press('key-s', 'giveline')
      }
    } else if (event.code === 'Escape') {
      event.preventDefault()
      networkManager.sendFishingStop()
    }
  }

  function onKeyup(event: KeyboardEvent) {
    if (event.code === 'Space') release('key-space')
    else if (event.code === 'KeyS') release('key-s')
  }
</script>

<svelte:window onkeydown={onKeydown} onkeyup={onKeyup} />

{#if $fishingTargeting}
  <SkillTargetHint
    icon={FISHING.icon}
    message={`Left-click water within ${max_cast_distance_m()} m. Esc to cancel.`}
    onCancel={cancelFishingTargeting}
  />
{/if}

{#if $myFishing.phase === 'bite'}
  <button
    class="bite"
    onclick={() => networkManager.sendFishingRespond('hook')}
  >
    HOOK IT — click / SPACE
  </button>
{:else if $myFishing.phase === 'fight'}
  {@const f = $myFishing.fight}
  {@const bold =
    f.trophy && f.fishState === 'running' && f.tension >= TROPHY_MIN_TENSION}
  <div class="fight-panel">
    {#if f.trophy}<div class="trophy-label">TROPHY FISH</div>{/if}
    {#if bold}
      <div class="fish-state bold">Good tension — the trophy is tiring!</div>
    {:else if f.trophy && f.fishState === 'running'}
      <div class="fish-state running">
        Reel back to {TROPHY_MIN_TENSION}+ — a loose line can lose the hook!
      </div>
    {:else if f.fishState === 'running'}
      <div class="fish-state running">The fish runs — watch the tension!</div>
    {:else if f.fishState === 'resting'}
      <div class="fish-state resting">The fish holds steady.</div>
    {:else}
      <div class="fish-state exhausted">It's exhausted — reel it in!</div>
    {/if}

    <div
      class="tension-track"
      role="progressbar"
      aria-label="Line tension"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={f.tension}
    >
      <span
        class="tension-fill"
        class:tension-warn={f.tension >= 60 && f.tension < 85 && !bold}
        class:tension-bold={bold}
        class:tension-high={f.tension >= 85}
        style={`width: ${Math.min(100, f.tension)}%`}
      ></span>
      {#if f.trophy}
        <span
          class="bold-mark"
          class:lit={bold}
          style={`left: ${TROPHY_MIN_TENSION}%`}
          title="Trophy fish only tire above this line while running"
        ></span>
      {/if}
    </div>
    <div class="stamina-label">Stamina: {f.stamina}%</div>
    <div class="stance-row">
      {#each STANCE_BUTTONS as b (b.stance)}
        <button
          class="stance-btn"
          class:reel={b.stance === 'reel'}
          class:give={b.stance === 'giveline'}
          class:held={stance === b.stance}
          onpointerdown={(e) => {
            e.preventDefault()
            press(`ptr-${b.stance}`, b.stance)
          }}
          onpointerup={() => release(`ptr-${b.stance}`)}
          onpointerleave={() => release(`ptr-${b.stance}`)}
          onpointercancel={() => release(`ptr-${b.stance}`)}
        >
          {b.label}<span class="hint">{b.hint}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .bite {
    position: fixed;
    left: 50%;
    bottom: 22%;
    transform: translateX(-50%);
    padding: 8px 18px;
    border-radius: 999px;
    font-size: 15px;
    z-index: 30;
    cursor: pointer;
    background: rgba(213, 73, 60, 0.92);
    color: #fff;
    border: 1px solid #f2ede2;
    font-weight: 700;
    animation: fishing-pulse 0.5s ease-in-out infinite alternate;
  }

  @keyframes fishing-pulse {
    from {
      transform: translateX(-50%) scale(1);
    }
    to {
      transform: translateX(-50%) scale(1.08);
    }
  }

  .fight-panel {
    position: fixed;
    left: 50%;
    bottom: 20%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(360px, 86vw);
    padding: 12px 16px;
    border-radius: 12px;
    background: rgba(6, 10, 14, 0.88);
    border: 1px solid rgba(166, 200, 238, 0.35);
    z-index: 30;
    user-select: none;
  }

  .fish-state {
    text-align: center;
    font-size: 14px;
    font-weight: 700;
    color: #e6edf3;
  }

  .fish-state.running {
    color: #f2a65e;
    animation: state-pulse 0.4s ease-in-out infinite alternate;
  }

  .trophy-label {
    color: #ffd766;
    font-weight: bold;
  }
  .stamina-label {
    color: #b9dfc6;
    font-size: 12px;
  }

  .fish-state.bold {
    color: #ffd766;
    text-shadow: 0 0 8px rgba(255, 215, 102, 0.6);
    animation: state-pulse 0.4s ease-in-out infinite alternate;
  }

  .fish-state.exhausted {
    color: #6fd598;
    animation: state-pulse 0.6s ease-in-out infinite alternate;
  }

  @keyframes state-pulse {
    from {
      opacity: 0.75;
    }
    to {
      opacity: 1;
    }
  }

  .tension-track {
    position: relative;
    height: 10px;
    border-radius: 999px;
    overflow: hidden;
    background: rgba(64, 98, 135, 0.45);
    border: 1px solid rgba(166, 200, 238, 0.25);
  }

  .tension-fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: #6fd598;
    transition:
      width 0.2s linear,
      background 0.2s;
  }

  .bold-mark {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: rgba(255, 255, 255, 0.7);
    transform: translateX(-1px);
    transition: background 0.2s;
  }

  .bold-mark.lit {
    background: #fff3b0;
    box-shadow: 0 0 6px 2px rgba(255, 220, 120, 0.9);
  }

  .tension-fill.tension-warn {
    background: #e8c34f;
  }

  .tension-fill.tension-bold {
    background: linear-gradient(90deg, #e8c34f, #ffd766);
    box-shadow: 0 0 10px 2px rgba(255, 215, 102, 0.75);
    animation: fishing-bold-shimmer 0.5s ease-in-out infinite alternate;
  }

  @keyframes fishing-bold-shimmer {
    from {
      filter: brightness(1);
    }
    to {
      filter: brightness(1.3);
    }
  }

  .tension-fill.tension-high {
    background: #d5493c;
    animation: fishing-pulse-bar 0.4s ease-in-out infinite alternate;
  }

  @keyframes fishing-pulse-bar {
    from {
      filter: brightness(1);
    }
    to {
      filter: brightness(1.35);
    }
  }

  .stance-row {
    display: flex;
    gap: 8px;
  }

  .stance-btn {
    flex: 1;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 10px 8px;
    border-radius: 8px;
    font-weight: 700;
    font-size: 15px;
    color: #fff;
    border: 1px solid #f2ede2;
    touch-action: none;
  }

  .stance-btn .hint {
    font-weight: 400;
    font-size: 10px;
    opacity: 0.8;
  }

  .stance-btn.reel {
    background: rgba(63, 153, 96, 0.55);
  }

  .stance-btn.give {
    background: rgba(213, 73, 60, 0.55);
  }

  .stance-btn.held {
    filter: brightness(1.35);
    border-color: #fff;
    box-shadow: 0 0 8px rgba(255, 255, 255, 0.35) inset;
  }
</style>
