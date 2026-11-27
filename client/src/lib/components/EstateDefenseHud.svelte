<script lang="ts">
  import { t } from '../i18n'
  import { estateDefense, incomingAngles } from '../stores/estateDefenseStore'
  import { deadlineClock } from '../stores/deadlineClock'

  /** Arrows point the way the next wave comes for its last seconds. */
  const ARROW_LEAD_MS = 20_000

  const now = deadlineClock(estateDefense, (current) =>
    current ? current.receivedAt + current.state.ends_in_ms : 0
  )

  const view = $derived.by(() => {
    const current = $estateDefense
    if (!current) return null
    const { state, receivedAt } = current
    const left = (ms: number) =>
      Math.max(0, Math.ceil((ms - ($now - receivedAt)) / 1000))
    const nextWave =
      state.next_wave_in_ms == null ? null : left(state.next_wave_in_ms)
    const soon = nextWave !== null && nextWave * 1000 <= ARROW_LEAD_MS
    return {
      state,
      nextWave,
      endsIn: left(state.ends_in_ms),
      arrows: soon ? $incomingAngles : [],
    }
  })

  const clock = (seconds: number) =>
    `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
</script>

{#if view}
  <div class="defense">
    <span class="wave">
      {$t('estateDefense.wave', {
        wave: view.state.wave,
        total: view.state.wave_total,
      })}
    </span>
    {#if view.nextWave !== null}
      <span>{$t('estateDefense.nextWave', { seconds: view.nextWave })}</span>
    {/if}
    <span class="banner" title={$t('estateDefense.banner')}>
      {#each { length: view.state.banner_max } as _, i (i)}
        <i class:lost={i >= view.state.banner_hp}></i>
      {/each}
    </span>
    <span class="clock">{clock(view.endsIn)}</span>
  </div>
  {#each view.arrows as angle, i (i)}
    <div
      class="arrow"
      style:left="calc(50% + {Math.cos(angle)} * (50vw - 48px))"
      style:top="calc(50% - {Math.sin(angle)} * (50vh - 130px))"
      style:transform="translate(-50%, -50%) rotate({-angle}rad)"
      aria-hidden="true"
    ></div>
  {/each}
{/if}

<style>
  .defense {
    position: fixed;
    z-index: 45;
    top: 46px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    width: max-content;
    max-width: calc(100vw - 24px);
    box-sizing: border-box;
    padding: 6px 14px;
    border-radius: 10px;
    border: 1px solid rgba(214, 96, 110, 0.55);
    background: rgba(14, 6, 10, 0.88);
    color: #f0e6e8;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 13px;
    pointer-events: none;
  }
  .wave {
    color: #ffb3bd;
    font-weight: 600;
  }
  .banner {
    display: flex;
    gap: 3px;
  }
  .banner i {
    width: 10px;
    height: 12px;
    border-radius: 2px;
    background: #d6606e;
  }
  .banner i.lost {
    background: rgba(255, 255, 255, 0.15);
  }
  .clock {
    color: #c9b8bb;
    font-variant-numeric: tabular-nums;
  }
  .arrow {
    position: fixed;
    z-index: 44;
    width: 0;
    height: 0;
    border-top: 16px solid transparent;
    border-bottom: 16px solid transparent;
    border-left: 22px solid rgba(214, 96, 110, 0.9);
    pointer-events: none;
    animation: pulse 0.9s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
</style>
