<script lang="ts" generics="Entry extends GoldRankingEntry">
  import GoldAmount from './GoldAmount.svelte'
  import { formatCount } from './metrics'
  import type { GoldRankingEntry } from './goldRanking'

  let { entries, totalGold, titleId, label, entryHeading, goldHeading, entryKey, category, quantityUnit }: {
    entries: Entry[]
    totalGold: number
    titleId: string
    label: string
    entryHeading: string
    goldHeading: string
    entryKey: (entry: Entry) => string
    category: (entry: Entry) => string
    quantityUnit: (entry: Entry) => string
  } = $props()
  const percent = (gold: number) => totalGold ? gold / totalGold * 100 : 0
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="table-scroll" tabindex="0" role="region" aria-label={label}>
  <table aria-labelledby={titleId}>
    <thead><tr><th scope="col" class="rank">순위</th><th scope="col">{entryHeading}</th><th scope="col" class="number">수량 / 횟수</th><th scope="col" class="number">{goldHeading}</th><th scope="col" class="share">비중</th></tr></thead>
    <tbody>
      {#each entries as entry, index (entryKey(entry))}
        {@const itemCategory = category(entry)}
        <tr>
          <td class="rank"><span class:podium={index < 3}>{index + 1}</span></td>
          <th scope="row" class="item-name">{entry.name}{#if itemCategory}<small>{itemCategory}</small>{/if}</th>
          <td class="number">{formatCount(entry.quantity)}<small>{quantityUnit(entry)}</small></td>
          <td class="number amount"><GoldAmount copper={entry.gold} /></td>
          <td class="share"><div class="share-value"><span class="share-track" aria-hidden="true"><span style:width={`${percent(entry.gold)}%`}></span></span><span>{formatCount(percent(entry.gold))}%</span></div></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .table-scroll { overflow: auto; max-height: 460px; margin: 18px 0; }
  table { width: 100%; min-width: 540px; border-collapse: separate; border-spacing: 0; font-size: 13px; }
  th, td { padding: 11px 12px; border-bottom: 1px solid #edf1ee; text-align: left; }
  thead th { position: sticky; top: 0; z-index: 1; background: #f6f8f7; color: #74857d; font-size: 11px; font-weight: 500; white-space: nowrap; }
  tbody tr:last-child > * { border-bottom: 0; }
  .rank { width: 54px; text-align: center; font-variant-numeric: tabular-nums; }
  .rank span { display: inline-grid; place-items: center; min-width: 28px; height: 28px; color: #899b90; font-weight: 600; border-radius: 8px; }
  .rank .podium { background: #eaf3ef; color: #166d5e; }
  .item-name { font-weight: 500; overflow-wrap: anywhere; }
  .item-name small { display: block; margin-top: 3px; color: #899b90; font-size: 10px; }
  .number { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .number small { margin-left: 4px; color: #899b90; font-size: 10px; }
  .amount { font-weight: 600; }
  .share { width: 150px; text-align: right; font-variant-numeric: tabular-nums; }
  .share-value { display: flex; align-items: center; justify-content: flex-end; gap: 10px; color: #74857d; }
  .share-value > span:last-child { min-width: 48px; }
  .share-track { width: 70px; height: 5px; overflow: hidden; background: #edf1ee; border-radius: 4px; }
  .share-track > span { display: block; height: 100%; background: #168878; border-radius: inherit; }
  @media (max-width: 600px) {
    table { min-width: 0; font-size: 11px; }
    th, td { padding: 9px 4px; }
    thead th { font-size: 10px; }
    .rank { width: 28px; }
    .share { width: 48px; }
    .share-track { display: none; }
  }
</style>
