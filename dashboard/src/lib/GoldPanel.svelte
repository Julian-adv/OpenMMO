<script lang="ts">
  import GoldAmount from './GoldAmount.svelte'
  import HistoryChart from './HistoryChart.svelte'
  import MetricsError from './MetricsError.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { formatGold, formatDateTime, goldPeriods, type GoldHistory, type GoldHours } from './metrics'

  let { hours = $bindable(), history, loading, refreshing, error, refresh }: {
    hours: GoldHours
    history: GoldHistory | null
    loading: boolean
    refreshing: boolean
    error: string
    refresh: () => void
  } = $props()
  let period = $derived(goldPeriods.find((option) => option.hours === hours)!)
  let peak = $derived(history?.samples.length ? Math.max(...history.samples.map((sample) => sample.peak_gold)) : null)
  let sampleCount = $derived(history?.samples.reduce((total, sample) => total + sample.sample_count, 0) ?? 0)
</script>

<section class="chart-panel" aria-labelledby="gold-chart-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="gold-chart-title">서버 총 골드 추이</h2>
      <p>모든 캐릭터가 보유한 골드의 합계 · 오프라인 캐릭터와 NPC 포함</p>
    </div>
    <PeriodFilter bind:hours options={goldPeriods} label="총 골드 조회 기간" />
  </div>
  <MetricsError {error} until={history?.until} {refreshing} {refresh} />
  <div class="metric-summary">
    <span>최근 집계 총 골드{error ? ' · 갱신 중단' : ''}</span>
    <strong>{#if history?.latest}<GoldAmount copper={history.latest.total_gold} />{:else}—{/if}</strong>
    <p>{history?.latest ? `마지막 집계 기준: ${formatDateTime(history.latest.timestamp)} KST` : '첫 시간별 집계를 기다리고 있어요'}</p>
    <p>1시간에 한 번 기록합니다. 6개월·1년 그래프는 구간 평균을 표시합니다.</p>
  </div>
  <div class="chart-meta"><span>총 골드</span><span>{period.intervalLabel} · 한국 시간 (KST)</span></div>
  {#if history && history.samples.length > 0}
    <HistoryChart {history} {peak} value={(sample) => sample.total_gold} legend="서버 총 골드" valueLabel={period.interval > 3600 ? '골드 (평균)' : '골드'} unit="" peakLabel="기간 최고" axisWidth={72} formatValue={formatGold}>
      {#snippet amount(copper, svg)}<GoldAmount {copper} {svg} />{/snippet}
      {#snippet detail(selected)}
        {#if period.interval > 3600}
          <span>구간 최고: <GoldAmount copper={selected.peak_gold} /> · {selected.sample_count}개 시간별 기록</span>
        {:else}
          <span>시간별 집계 기록</span>
        {/if}
      {/snippet}
    </HistoryChart>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '골드 기록을 불러오고 있어요' : error ? '기록에 연결할 수 없어요' : '이 기간의 골드 기록이 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 그래프가 자동으로 갱신됩니다.' : '서버 총 골드는 1시간에 한 번 기록됩니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{history ? `${formatDateTime(history.from)} — ${formatDateTime(history.until)}` : `최근 ${period.label}`} <span class="timezone">KST</span></span>
    <span>{history ? `${sampleCount.toLocaleString('ko-KR')}개 기록` : '기록 확인 중'}</span>
  </div>
</section>
