<script lang="ts">
  import GoldAmount from './GoldAmount.svelte'
  import GoldRankingTable from './GoldRankingTable.svelte'
  import MetricsError from './MetricsError.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { formatCount, formatDateTime, uniquePeriods, type GoldHours } from './metrics'
  import { goldSinkKey, type GoldSinks } from './goldSinks'

  let { hours = $bindable(), sinks, loading, refreshing, error, refresh }: {
    hours: GoldHours
    sinks: GoldSinks | null
    loading: boolean
    refreshing: boolean
    error: string
    refresh: () => void
  } = $props()
  let period = $derived(uniquePeriods.find((option) => option.hours === hours)!)
</script>

<section class="chart-panel" aria-labelledby="gold-sinks-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="gold-sinks-title">골드 소모 순위</h2>
      <p>상인 구매·재매입·가판 수수료·토지세 · 소모 골드 내림차순</p>
    </div>
    <PeriodFilter bind:hours options={uniquePeriods} label="골드 소모 집계 기간" />
  </div>
  <MetricsError {error} until={sinks?.until} {refreshing} {refresh} />
  <div class="metric-summary">
    <span>완료된 최근 {period.label} 소모된 골드{error ? ' · 갱신 중단' : ''}</span>
    <strong>{#if sinks}<GoldAmount copper={sinks.total_gold} />{:else}—{/if}</strong>
    <p>상인 구매·재매입은 아이템별 실제 지불액을, 가판 판매는 수수료만 합산합니다. 토지세와 체납 복구 비용도 포함합니다.</p>
    <p>주민 NPC·플레이어 간에 이동한 골드와 세금 계좌 입출금은 제외합니다. 현재 시간대의 소모는 다음 정각에 반영됩니다.</p>
  </div>
  {#if sinks && sinks.collection_started_at > sinks.from}
    <p class="chart-notice">{formatDateTime(sinks.collection_started_at)} KST부터 수집한 기록만 포함합니다.</p>
  {/if}
  {#if sinks && sinks.entries.length > 0}
    <GoldRankingTable entries={sinks.entries} totalGold={sinks.total_gold}
      titleId="gold-sinks-title" label="골드 소모 순위 표" entryHeading="골드 소모 항목" goldHeading="소모 골드"
      entryKey={goldSinkKey} category={(entry) => entry.sink === 'item_purchase' ? '상인 구매' : entry.sink === 'item_buyback' ? '상인 재매입' : ''}
      quantityUnit={(entry) => entry.item_def_id !== undefined ? '개' : '회'} />
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '골드 소모 기록을 불러오고 있어요' : error ? '골드 소모 기록에 연결할 수 없어요' : '이 기간에 기록된 골드 소모가 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 표가 자동으로 갱신됩니다.' : '구매·수수료·세금으로 소모한 골드를 모아 매시간 순위에 반영합니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{sinks ? `${formatDateTime(sinks.from)} — ${formatDateTime(sinks.until)}` : `최근 ${period.label}`} <span class="timezone">KST</span></span>
    <span>{sinks ? `${formatCount(sinks.entries.length)}개 소모 항목` : '기록 확인 중'}</span>
  </div>
</section>

<style>
  .chart-empty { min-height: 210px; }
</style>
