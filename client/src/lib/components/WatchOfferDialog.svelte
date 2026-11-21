<script lang="ts">
  import { locale, t } from '../i18n'
  import { npcDisplayName } from '../i18n/npcs'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import { watchOffer } from '../stores/watchDutyStore'
  import { formatGold } from '../utils/currency'

  const TERMS = ['rollCall', 'share', 'gear', 'work', 'return'] as const

  const offer = $derived($watchOffer)
  const close = () => watchOffer.set(null)

  $effect(() => {
    if (offer) return mountOverlay('watchOffer', close)
  })

  function accept() {
    if (!offer) return
    networkManager.sendWatchShiftAccept(offer.npcPlayerId, offer.sergeant)
    close()
  }

  const clock = (minute: number) =>
    `${String(Math.floor(minute / 60)).padStart(2, '0')}:${String(minute % 60).padStart(2, '0')}`
</script>

{#if offer}
  <div class="watch-offer-backdrop">
    <div class="watch-offer" role="dialog" aria-labelledby="watch-offer-title">
      <h2 id="watch-offer-title">{$t('watch.offer.title')}</h2>
      <p>
        {$t('watch.offer.intro', {
          sergeant: npcDisplayName(offer.sergeant, $locale),
          departs: clock(offer.departsMinute),
          wage: formatGold(offer.wage),
        })}
      </p>
      <ul>
        {#each TERMS as term (term)}
          <li>{$t(`watch.offer.${term}`)}</li>
        {/each}
      </ul>
      <p class="ask">{$t('watch.offer.ask')}</p>
      <div class="actions">
        <button type="button" class="secondary" onclick={close}
          >{$t('watch.offer.no')}</button
        >
        <button type="button" onclick={accept}>{$t('watch.offer.yes')}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .watch-offer-backdrop {
    position: absolute;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    padding: 16px;
    box-sizing: border-box;
    background: rgba(8, 5, 4, 0.58);
    backdrop-filter: blur(2px);
  }
  .watch-offer {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: min(440px, calc(100vw - 32px));
    box-sizing: border-box;
    padding: 22px;
    border: 1px solid rgba(200, 184, 138, 0.7);
    border-radius: 12px;
    background: #18160f;
    color: #f3e8d2;
    box-shadow: 0 22px 55px rgba(0, 0, 0, 0.55);
    font:
      13px/1.55 'Noto Sans KR',
      sans-serif;
  }
  h2 {
    margin: 0;
    color: #fff4df;
    font-size: 18px;
  }
  p {
    margin: 0;
    color: #dbc98f;
  }
  .ask {
    color: #fff4df;
  }
  ul {
    margin: 0;
    padding-left: 18px;
  }
  li {
    padding: 2px 0;
  }
  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin-top: 6px;
  }
  button {
    padding: 9px 12px;
    border: 1px solid #c8b88a;
    border-radius: 7px;
    background: #6b5a2e;
    color: #fff4df;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }
  button.secondary {
    border-color: #675b4e;
    background: #2b251f;
  }
</style>
