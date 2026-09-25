<script lang="ts">
  import type { MetricsResource } from './metricsResource.svelte'
  import HistoryChart from './HistoryChart.svelte'
  import MetricsError from './MetricsError.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { formatDateTime } from './metrics'
  import { formatBytes, formatRate, perAccountRate, trafficPeriods, type NetworkHistory, type NetworkPoint, type TrafficHours } from './traffic'

  let { hours = $bindable(), resource }: { hours: TrafficHours, resource: MetricsResource<NetworkHistory> } = $props()
  let { history, loading, refreshing, error, refresh } = $derived(resource)
  let direction = $state<'total' | 'tx' | 'rx'>('total')
  let latest = $derived(history?.latest)
  let perAccount = $derived(perAccountRate(latest ?? null))
  let stale = $derived(history && (!history.available || latest && history.until - latest.timestamp > history.collection_interval_seconds * 2 + 30))
  let intervalMinutes = $derived((history?.collection_interval_seconds ?? 60) / 60)
  const rate = (sample: NetworkPoint) => (direction === 'tx' ? sample.tx_bytes : direction === 'rx' ? sample.rx_bytes : sample.tx_bytes + sample.rx_bytes) / sample.seconds
  let peak = $derived(history?.samples.length ? Math.max(...history.samples.map(rate)) : null)
  let legend = $derived(direction === 'tx' ? '송신' : direction === 'rx' ? '수신' : '송신 + 수신')
</script>

<section class="chart-panel" aria-labelledby="network-title" aria-busy={loading}>
  <div class="chart-heading">
    <div><h2 id="network-title">네트워크 대역폭</h2><p>외부 인터페이스 전체 · 정적 파일·게임 통신·기타 서버 통신 포함</p></div>
    <PeriodFilter bind:hours options={trafficPeriods} label="네트워크 조회 기간" />
  </div>
  <MetricsError {error} until={history?.until} {refreshing} {refresh} />
  {#if stale}<p class="chart-notice">네트워크 수집이 지연되거나 중단되었습니다. 마지막으로 수집한 값을 표시합니다.</p>{/if}
  <div class="network-summary">
    <div><span>최근 구간 평균 송신</span><strong>{latest ? formatRate(latest.tx_bytes / latest.seconds) : '—'}</strong></div>
    <div><span>최근 구간 평균 수신</span><strong>{latest ? formatRate(latest.rx_bytes / latest.seconds) : '—'}</strong></div>
    <div><span>접속 계정당 평균 · 송신 + 수신</span><strong>{perAccount === null ? '—' : formatRate(perAccount)}</strong><small>{latest ? `구간 종료 시점 ${latest.accounts.toLocaleString('ko-KR')}계정 기준` : '접속 계정이 없으면 표시하지 않습니다'}</small></div>
  </div>
  <p class="sample-time">{latest ? `${formatDateTime(latest.timestamp)} KST · 직전 ${Math.round(latest.seconds)}초 평균 · ${latest.interface}` : `첫 수집 후 ${intervalMinutes}분부터 평균 속도를 표시합니다.`}</p>
  <div class="chart-meta">
    <label>전송 속도 <select bind:value={direction} aria-label="네트워크 그래프 방향"><option value="total">송신 + 수신</option><option value="tx">송신</option><option value="rx">수신</option></select></label>
    <span>{intervalMinutes}분마다 수집</span>
  </div>
  {#if history && history.samples.length > 0}
    <HistoryChart {history} {peak} value={rate} {legend} valueLabel="평균 전송 속도" unit="" peakLabel="구간 최고 평균" formatValue={formatRate} axisWidth={90}>
      {#snippet amount(value: number)}{formatRate(value)}{/snippet}
      {#snippet detail(sample)}<span>송신 {formatRate(sample.tx_bytes / sample.seconds)} · 수신 {formatRate(sample.rx_bytes / sample.seconds)}</span>{/snippet}
    </HistoryChart>
  {:else}
    <div class="chart-empty" role="status"><strong>{loading ? '네트워크 기록을 불러오고 있어요' : '이 기간에 수집된 네트워크 기록이 없습니다'}</strong></div>
  {/if}
  <div class="chart-footer">
    <span>기간 수집량 · 송신 {history ? formatBytes(history.tx_bytes) : '—'} · 수신 {history ? formatBytes(history.rx_bytes) : '—'}</span>
    <span>최근 30일 보관</span>
  </div>
  {#if history?.collection_started_at != null}
    <p class="sample-time">보관 중인 첫 기록: {formatDateTime(history.collection_started_at)} KST. 수집 중단 구간은 합계에서 제외됩니다.</p>
  {/if}
</section>

<style>
  .network-summary { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 20px; padding: 18px 0; }
  .network-summary div { display: flex; flex-direction: column; gap: 8px; }
  .network-summary span, .network-summary small, .sample-time { color: #75877f; font-size: 12px; line-height: 1.7; }
  .network-summary strong { font-size: 23px; font-variant-numeric: tabular-nums; }
  select { margin-left: 8px; background: white; border: 1px solid #dde5e0; border-radius: 6px; padding: 4px; color: inherit; }
  .chart-empty { min-height: 150px; }
  @media (max-width: 650px) { .network-summary { grid-template-columns: 1fr; gap: 18px; } }
</style>
