<script lang="ts">
  import { serverNotice } from '../stores/gameStore'
</script>

{#if $serverNotice}
  <div class="server-notice" role="alert">
    {$serverNotice}
  </div>
{/if}

<style>
  /* Clear of the top-corner widgets (which end ~40px down). Anchoring to the
     top rather than above TradeOfferToast keeps a wrapped notice on screen on
     short viewports; z-index puts it over the toast if both are up. */
  .server-notice {
    position: fixed;
    top: 40px;
    left: 50%;
    z-index: 1100;
    width: max-content;
    max-width: min(640px, calc(100vw - 32px));
    transform: translateX(-50%);
    padding: 10px 18px;
    border: 1px solid rgba(255, 196, 92, 0.65);
    border-radius: 8px;
    background: rgba(24, 16, 8, 0.94);
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.55);
    color: #ffe3a3;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 14px;
    line-height: 1.4;
    text-align: center;
    pointer-events: none;
  }
</style>
