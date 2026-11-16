<script lang="ts">
  import { locale, t } from '../i18n'
  import { npcDisplayName } from '../i18n/npcs'
  import { itemDisplayName } from '../data/itemDefs'
  import { formatGold } from '../utils/currency'
  import {
    pendingOutfitOffer,
    type PendingOutfitOffer,
  } from '../stores/tradeStore'
  import { networkManager } from '../network/socket'
  import ConsentToast from './ConsentToast.svelte'

  /** Shorter than the server's 60 s, so a late answer is never refused. */
  const OFFER_TTL_MS = 45_000

  const offer = $derived($pendingOutfitOffer)

  function answer(offer: PendingOutfitOffer, accept: boolean) {
    pendingOutfitOffer.set(null)
    networkManager.sendRespondOutfitOffer(offer.npcPlayerId, accept)
  }

  $effect(() => {
    if (!offer) return
    const timer = setTimeout(
      () => answer(offer, false),
      Math.max(0, offer.offeredAt + OFFER_TTL_MS - Date.now())
    )
    return () => clearTimeout(timer)
  })
</script>

{#if offer}
  <ConsentToast
    label={$t('outfit.offerTitle')}
    top="20%"
    accent="#f0c040"
    acceptLabel={$t('outfit.buy')}
    declineLabel={$t('outfit.notNow')}
    gaugeDurationMs={OFFER_TTL_MS}
    gaugeStartAt={offer.offeredAt}
    stacked
    onaccept={() => answer(offer, true)}
    ondecline={() => answer(offer, false)}
  >
    {$t('outfit.offerMessage', {
      name: npcDisplayName(offer.npcName, $locale),
      items: offer.itemDefIds
        .map((id) => itemDisplayName(id, 0, $locale))
        .join(', '),
      price: formatGold(offer.price),
    })}
  </ConsentToast>
{/if}
