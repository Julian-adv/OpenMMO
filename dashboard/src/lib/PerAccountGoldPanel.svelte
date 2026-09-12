<script lang="ts">
  import type { MetricsResource } from './metricsResource.svelte'
  import GoldAmount from './GoldAmount.svelte'
  import HistoryChart from './HistoryChart.svelte'
  import MetricsError from './MetricsError.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { formatCount, formatGold, formatDateTime, goldPeriods, kstDayStart, uniquePeriods, type ChartMarker, type GoldHours, type PerAccountGoldHistory, type UniqueHours } from './metrics'

  let { hours = $bindable(), activeHours = $bindable(), resource, markers = [] }: {
    hours: GoldHours
    activeHours: UniqueHours
    markers?: ChartMarker[]
    resource: MetricsResource<PerAccountGoldHistory>
  } = $props()
  let { history, loading, refreshing, error, refresh } = $derived(resource)
  let period = $derived(goldPeriods.find((option) => option.hours === hours)!)
  let activePeriod = $derived(uniquePeriods.find((option) => option.hours === activeHours)!)
  let latest = $derived(history?.latest)
  let peak = $derived(history?.samples.length ? Math.max(...history.samples.map((sample) => sample.peak_gold_per_account)) : null)
  let sampleCount = $derived(history?.samples.reduce((total, sample) => total + sample.sample_count, 0) ?? 0)
</script>

<section class="chart-panel" aria-labelledby="per-account-gold-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="per-account-gold-title">활성 유저 1인당 골드 추이</h2>
      <p>서버 총 골드 ÷ 직전 {activePeriod.label} 유니크 접속 계정 수</p>
    </div>
    <PeriodFilter bind:hours options={goldPeriods} label="1인당 골드 조회 기간" />
  </div>
  <div class="active-period-filter">
    <span>나눌 활성 유저 기준</span>
    <PeriodFilter bind:hours={activeHours} options={uniquePeriods} label="골드를 나눌 활성 유저 집계 기간" />
  </div>
  <MetricsError {error} until={history?.until} {refreshing} {refresh} />
  <div class="metric-summary">
    <span>최근 집계 · 활성 유저 1인당 골드{error ? ' · 갱신 중단' : ''}</span>
    <strong>{#if latest}<GoldAmount copper={latest.gold_per_account} />{:else}—{/if}<small>/계정</small></strong>
    {#if latest}
      <p><GoldAmount copper={latest.total_gold} /> ÷ {formatCount(latest.accounts)}계정 · 골드 집계: {formatDateTime(latest.timestamp)} KST</p>
      <p>활성 유저 집계: {formatDateTime(kstDayStart(latest.timestamp))} KST · 직전 {activePeriod.label}</p>
    {:else}
      <p>최근 골드 기록에 대응하는 활성 계정 집계가 없거나 계정 수가 0이면 계산하지 않습니다.</p>
    {/if}
    <p>각 시점의 골드를 그날 자정의 유니크 계정 수로 나눕니다. 총 골드는 오프라인 캐릭터와 NPC를 포함하며, 활성 계정은 공식 NPC를 제외합니다.</p>
  </div>
  {#if history && history.samples.length > 0 && history.collection_started_at > kstDayStart(history.from) - history.window_seconds}
    <p class="chart-notice">{formatDateTime(history.collection_started_at)} KST부터 수집한 접속 기록입니다. 일부 시점의 활성 계정 수는 선택한 기간 전체를 포함하지 못합니다.</p>
  {/if}
  <div class="chart-meta"><span>골드/계정 · 직전 {activePeriod.label} 활성 유저</span><span>{period.intervalLabel}</span></div>
  {#if history && history.samples.length > 0}
    <HistoryChart {history} {peak} value={(sample) => sample.gold_per_account} legend={`직전 ${activePeriod.label} 활성 유저 1인당 골드`} valueLabel={period.interval > 3600 ? '골드/계정 (평균)' : '골드/계정'} unit="/계정" peakLabel="기간 최고" axisWidth={72} formatValue={formatGold} fitAxis {markers}>
      {#snippet amount(copper, svg)}<GoldAmount {copper} {svg} />{/snippet}
      {#snippet detail(selected)}
        {#if period.interval > 3600}
          <span>구간 최고: <GoldAmount copper={selected.peak_gold_per_account} />/계정 · {selected.sample_count}개 시간별 기록</span>
          <span>시간별 1인당 골드를 계산한 뒤 평균합니다.</span>
        {:else}
          <span>활성 유저 집계: {formatDateTime(kstDayStart(selected.timestamp))} KST</span>
        {/if}
        <span>직전 {activePeriod.label} 유니크 접속 계정 기준</span>
      {/snippet}
    </HistoryChart>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '1인당 골드 기록을 불러오고 있어요' : error ? '기록에 연결할 수 없어요' : '이 기간에 계산할 수 있는 기록이 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 그래프가 자동으로 갱신됩니다.' : '시간별 골드와 해당 날짜의 활성 계정 집계가 필요합니다. 활성 계정이 0인 구간은 제외합니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{history ? `${formatDateTime(history.from)} — ${formatDateTime(history.until)}` : `최근 ${period.label}`} <span class="timezone">KST</span></span>
    <span>{history ? `${sampleCount.toLocaleString('ko-KR')}개 기록` : '기록 확인 중'}</span>
  </div>
</section>

<style>
  .active-period-filter { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; margin-top: 16px; }
  .active-period-filter > span { color: #60796c; font-size: 11px; }
</style>
