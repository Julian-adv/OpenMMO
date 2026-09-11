<script lang="ts">
  import GoldAmount from './GoldAmount.svelte'
  import MetricsError from './MetricsError.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { formatCount, formatDateTime, uniquePeriods, type GoldHours } from './metrics'
  import type { ItemGoldSources } from './itemGoldSources'

  let { hours = $bindable(), sources, loading, refreshing, error, refresh }: {
    hours: GoldHours
    sources: ItemGoldSources | null
    loading: boolean
    refreshing: boolean
    error: string
    refresh: () => void
  } = $props()
  let period = $derived(uniquePeriods.find((option) => option.hours === hours)!)
  const percent = (gold: number) => sources?.total_gold ? gold / sources.total_gold * 100 : 0
</script>

<section class="chart-panel" aria-labelledby="item-gold-sources-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="item-gold-sources-title">아이템 판매 골드 생산 순위</h2>
      <p>상인에게 판매한 아이템 종류별 · 시간별 합계 · 생성 골드 내림차순</p>
    </div>
    <PeriodFilter bind:hours options={uniquePeriods} label="아이템 판매 집계 기간" />
  </div>
  <MetricsError {error} until={sources?.until} {refreshing} {refresh} />
  <div class="metric-summary">
    <span>완료된 최근 {period.label} 판매로 생성된 골드{error ? ' · 갱신 중단' : ''}</span>
    <strong>{#if sources}<GoldAmount copper={sources.total_gold} />{:else}—{/if}</strong>
    <p>흥정을 포함한 실제 판매 지급액입니다. 주민 NPC·플레이어 간 거래는 제외하며, 재매입 비용은 차감하지 않습니다.</p>
    <p>현재 시간대의 판매는 다음 정각 집계 후 반영됩니다.</p>
  </div>
  {#if sources && sources.collection_started_at > sources.from}
    <p class="chart-notice">{formatDateTime(sources.collection_started_at)} KST부터 수집한 판매만 포함합니다.</p>
  {/if}
  {#if sources && sources.entries.length > 0}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" tabindex="0" role="region" aria-label="아이템 판매 골드 순위 표">
      <table aria-labelledby="item-gold-sources-title">
        <thead><tr><th scope="col" class="rank">순위</th><th scope="col">아이템</th><th scope="col" class="number">판매 수량</th><th scope="col" class="number">생성 골드</th><th scope="col" class="share">비중</th></tr></thead>
        <tbody>
          {#each sources.entries as entry, index (entry.item_def_id)}
            <tr>
              <td class="rank"><span class:podium={index < 3}>{index + 1}</span></td>
              <th scope="row" class="item-name" title={entry.item_def_id}>{entry.name}</th>
              <td class="number">{formatCount(entry.quantity)}<small>개</small></td>
              <td class="number amount"><GoldAmount copper={entry.gold} /></td>
              <td class="share"><div class="share-value"><span class="share-track" aria-hidden="true"><span style:width={`${percent(entry.gold)}%`}></span></span><span>{formatCount(percent(entry.gold))}%</span></div></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '아이템 판매 기록을 불러오고 있어요' : error ? '판매 기록에 연결할 수 없어요' : '이 기간에 기록된 상인 판매가 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 표가 자동으로 갱신됩니다.' : '판매 수량과 지급액을 모아 매시간 순위에 반영합니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{sources ? `${formatDateTime(sources.from)} — ${formatDateTime(sources.until)}` : `최근 ${period.label}`} <span class="timezone">KST</span></span>
    <span>{sources ? `${formatCount(sources.entries.length)}종 아이템` : '기록 확인 중'}</span>
  </div>
</section>

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
  .number { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .number small { margin-left: 4px; color: #899b90; font-size: 10px; }
  .amount { font-weight: 600; }
  .share { width: 150px; text-align: right; font-variant-numeric: tabular-nums; }
  .share-value { display: flex; align-items: center; justify-content: flex-end; gap: 10px; color: #74857d; }
  .share-value > span:last-child { min-width: 48px; }
  .share-track { width: 70px; height: 5px; overflow: hidden; background: #edf1ee; border-radius: 4px; }
  .share-track > span { display: block; height: 100%; background: #168878; border-radius: inherit; }
  .chart-empty { min-height: 210px; }
  @media (max-width: 600px) {
    table { min-width: 0; font-size: 11px; }
    th, td { padding: 9px 4px; }
    thead th { font-size: 10px; }
    .rank { width: 28px; }
    .share { width: 48px; }
    .share-track { display: none; }
  }
</style>
