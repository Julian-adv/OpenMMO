<script lang="ts">
  import {
    pendingTradeOffer,
    acceptTradeOffer,
    declineTradeOffer,
    type PendingTradeOffer,
  } from '../stores/tradeStore'
  import { networkManager } from '../network/socket'

  /** Offers go stale fast (the NPC may wander out of trade range). */
  const OFFER_TTL_MS = 30_000

  const offer = $derived($pendingTradeOffer)

  function accept(offer: PendingTradeOffer) {
    acceptTradeOffer(offer)
    // Register the open window server-side so the NPC holds position while we
    // shop (an NPC-pushed offer doesn't register until the player opts in).
    networkManager.sendOpenShop(offer.session.merchantPlayerId)
  }

  $effect(() => {
    if (!offer) return
    const timer = setTimeout(
      declineTradeOffer,
      Math.max(0, offer.offeredAt + OFFER_TTL_MS - Date.now())
    )
    return () => clearTimeout(timer)
  })
</script>

{#if offer}
  <div class="trade-offer" role="alertdialog" aria-label="Trade offer">
    <span class="offer-text">
      <strong>{offer.session.merchantName}</strong> wants to trade with you
    </span>
    <button class="accept-btn" onclick={() => accept(offer)}>Open</button>
    <button class="decline-btn" onclick={declineTradeOffer}>Not now</button>
  </div>
{/if}

<style>
  .trade-offer {
    position: fixed;
    left: 50%;
    top: 14%;
    transform: translateX(-50%);
    z-index: 44;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    backdrop-filter: blur(4px);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    pointer-events: auto;
  }

  .offer-text strong {
    color: #f0c040;
  }

  .accept-btn,
  .decline-btn {
    border-radius: 4px;
    padding: 4px 10px;
    font-family: inherit;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    transition: background 150ms ease, color 150ms ease;
  }

  .accept-btn {
    background: rgba(60, 90, 60, 0.85);
    color: #d6f0d6;
    border: 1px solid rgba(140, 220, 140, 0.35);
  }

  .accept-btn:hover {
    background: rgba(80, 120, 80, 0.95);
    color: #fff;
  }

  .decline-btn {
    background: none;
    color: #9fb2c3;
    border: 1px solid rgba(255, 255, 255, 0.18);
  }

  .decline-btn:hover {
    color: #fff;
    border-color: rgba(255, 255, 255, 0.4);
  }

  @media (pointer: coarse) {
    .accept-btn,
    .decline-btn {
      min-height: 32px;
    }
  }
</style>
