<script lang="ts">
  import { onDestroy } from 'svelte'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { locale, t } from '../i18n'
  import { EMOTE_LIST, emoteLabel } from '../emote-meta'
  import { combatController } from '../managers/combatController'
  import { remotePlayerManager } from '../managers/remotePlayerManager'
  import { networkManager } from '../network/socket'
  import { chargeTargeting } from '../stores/chargeStrikeStore'
  import { fishingTargeting, myFishing } from '../stores/fishingStore'
  import { inspectionTargeting } from '../stores/inspectionStore'
  import { ownPlayerFloor } from '../network/ownFloor'
  import { gameStore } from '../stores/gameStore'
  import { danceAnchors } from '../stores/danceBeatStore'
  import {
    type DanceCandidate,
    type DanceOffer,
    isJoinableDancer,
    joinableDancers,
  } from '../danceJoin'
  import { isMounted } from '../utils/mounts'

  /** How often the nearest dancer is looked up again, in ms. */
  const SCAN_MS = 250
  /** Join stays disabled this long after a click, the server's join rate. */
  const JOIN_COOLDOWN_MS = 1000
  /** A dancer asked to join is not offered again for this long, so a
   *  refusal does not leave the same prompt up. */
  const RETRY_MS = 10_000

  let dancer = $state<DanceOffer | null>(null)
  let joining = $state(false)
  /** Dancers in reach at the last scan, nearest first. */
  let offers: DanceOffer[] = []
  /** Dancers whose prompt was closed, until they stop dancing or leave view. */
  const dismissed = new SvelteSet<number>()
  /** When each dancer asked to join may be offered again. */
  const asked = new SvelteMap<number, number>()

  /** Dancers to offer, or none while a targeting hint is up, fighting,
   *  fishing, mounted, petrified or down. */
  function dancersInReach(): DanceOffer[] {
    const state = $gameStore
    const me = state.currentPlayer
    if (
      !me ||
      !state.isConnected ||
      me.health <= 0 ||
      me.petrified ||
      isMounted(me)
    ) {
      return []
    }
    if (
      $chargeTargeting ||
      $fishingTargeting ||
      $inspectionTargeting ||
      $myFishing.phase !== 'idle' ||
      combatController.isInCombat
    )
      return []
    const candidates: DanceCandidate[] = []
    for (const [id, remote] of remotePlayerManager.players) {
      const info = state.otherPlayers.get(id)
      if (remote.state !== 'interact' || !info) continue
      candidates.push({
        id,
        name: info.name,
        anim: remote.interactionAnim,
        alive: info.health > 0,
        petrified: !!info.petrified,
        floor: info.floorLevel,
        position: remote.position,
      })
    }
    for (const id of dismissed) {
      if (!candidates.some((c) => c.id === id && isJoinableDancer(c)))
        dismissed.delete(id)
    }
    return joinableDancers(
      me.id,
      $ownPlayerFloor,
      me.position,
      candidates,
      danceAnchors
    )
  }

  function danceLabel(anim: string): string {
    const emote = EMOTE_LIST.find((e) => e.anim === anim)
    return emote ? emoteLabel(emote, $locale) : anim
  }

  function refresh() {
    const now = performance.now()
    for (const [id, until] of asked) if (until <= now) asked.delete(id)
    offers = dancersInReach()
    const next =
      offers.find((o) => !dismissed.has(o.id) && !asked.has(o.id)) ?? null
    if (
      next?.id !== dancer?.id ||
      next?.name !== dancer?.name ||
      next?.dance !== dancer?.dance
    )
      dancer = next
  }

  const timer = setInterval(refresh, SCAN_MS)
  let cooldown: ReturnType<typeof setTimeout> | undefined
  onDestroy(() => {
    clearInterval(timer)
    clearTimeout(cooldown)
  })

  function join() {
    if (!dancer || joining) return
    networkManager.sendJoinDance(dancer.id)
    asked.set(dancer.id, performance.now() + RETRY_MS)
    joining = true
    cooldown = setTimeout(() => (joining = false), JOIN_COOLDOWN_MS)
  }

  /** Quiet every dance in reach, not just the one shown. */
  function dismiss() {
    for (const offer of offers) dismissed.add(offer.id)
    refresh()
  }
</script>

{#if dancer}
  <div class="dance-prompt" role="status">
    <svg class="note" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="7" cy="17.5" r="2.8" />
      <circle cx="17" cy="15.5" r="2.8" />
      <path d="M8.8 17.5V6.2l11-2.4v11.7H18V8.1l-7.4 1.6v7.8z" />
    </svg>
    <span class="text"
      >{$t('dance.dancing', {
        name: dancer.name,
        dance: danceLabel(dancer.dance),
      })}</span
    >
    <button type="button" class="join-btn" disabled={joining} onclick={join}>
      {$t('dance.join')}
    </button>
    <button
      type="button"
      class="dismiss-btn"
      title={$t('common.close')}
      aria-label={$t('common.close')}
      onclick={dismiss}>×</button
    >
  </div>
{/if}

<style>
  /* The HUD's hint panel (SkillTargetHint), in the emote panel's green. */
  .dance-prompt {
    position: fixed;
    z-index: 30;
    bottom: 100px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: calc(100vw - 24px);
    box-sizing: border-box;
    padding: 6px 6px 6px 12px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    backdrop-filter: blur(4px);
    color: #e6edf3;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 12px;
    pointer-events: auto;
    animation: dance-prompt-in 150ms ease-out;
  }

  .note {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    fill: #8fe08f;
  }

  .text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .join-btn {
    flex-shrink: 0;
    padding: 4px 12px;
    border: 1px solid rgba(140, 220, 140, 0.35);
    border-radius: 4px;
    background: rgba(60, 90, 60, 0.85);
    color: #d6f0d6;
    font-family: inherit;
    font-size: 12px;
    font-weight: 700;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background 150ms ease,
      color 150ms ease;
  }

  .join-btn:hover:not(:disabled) {
    background: rgba(80, 120, 80, 0.95);
    color: #fff;
  }

  .join-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .join-btn:focus-visible {
    outline: 2px solid #8fe08f;
    outline-offset: 2px;
  }

  .dismiss-btn {
    flex-shrink: 0;
    padding: 0 4px;
    border: none;
    background: none;
    color: #9fb2c3;
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
  }

  .dismiss-btn:hover {
    color: #fff;
  }

  .dismiss-btn:focus-visible {
    outline: 2px solid #9fb2c3;
    outline-offset: 2px;
  }

  @keyframes dance-prompt-in {
    from {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dance-prompt {
      animation: none;
    }
  }

  @media (pointer: coarse) {
    .join-btn,
    .dismiss-btn {
      min-height: 44px;
    }

    .dismiss-btn {
      min-width: 44px;
    }
  }
</style>
