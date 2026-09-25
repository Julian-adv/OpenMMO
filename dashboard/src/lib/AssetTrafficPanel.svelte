<script lang="ts">
  import type { MetricsResource } from './metricsResource.svelte'
  import MetricsError from './MetricsError.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { formatDateTime } from './metrics'
  import { categoryLabels, formatBytes, trafficPeriods, type AssetTraffic, type TrafficHours } from './traffic'

  let { hours = $bindable(), resource }: { hours: TrafficHours, resource: MetricsResource<AssetTraffic> } = $props()
  let { history, loading, refreshing, error, refresh } = $derived(resource)
  const percent = (bytes: number) => history?.total_bytes ? bytes / history.total_bytes * 100 : 0
  let stale = $derived(history && (!history.available || history.status && history.until - history.status.updated_at > 1260))
</script>

<section class="chart-panel" aria-labelledby="asset-traffic-title" aria-busy={loading}>
  <div class="chart-heading">
    <div><h2 id="asset-traffic-title">정적 파일 전송 순위</h2><p>Nginx 응답 본문 전송량 · 10분마다 집계 · 최근 30일 보관</p></div>
    <PeriodFilter bind:hours options={trafficPeriods} label="정적 파일 집계 기간" />
  </div>
  <MetricsError {error} until={history?.status?.updated_at} {refreshing} {refresh} />
  {#if history && !history.configured}
    <div class="chart-empty" role="status"><strong>정적 파일 전송량 수집이 설정되지 않았습니다</strong><p>운영 서버의 사이트별 access log 연결 후 기록이 쌓입니다.</p></div>
  {:else}
    {#if stale}<p class="chart-notice">로그 집계가 지연되거나 중단되었습니다. 마지막 집계 결과를 표시합니다.</p>{/if}
    {#if history?.status?.pending_bytes}<p class="chart-notice">추가 로그 {formatBytes(history.status.pending_bytes)}를 다음 집계에서 이어 읽습니다. 순위는 현재까지 처리한 기록 기준입니다.</p>{/if}
    {#if history?.status && (history.status.gaps > 0 || history.status.skipped_lines > 0)}
      <p class="chart-notice">수집 시작 이후 누락 가능 구간 {history.status.gaps}회 · 해석하지 못했거나 수집 범위를 벗어난 로그 {history.status.skipped_lines.toLocaleString('ko-KR')}줄이 있습니다.</p>
    {/if}
    <div class="metric-summary"><span>완료된 조회 구간의 정적 파일 전송량</span><strong>{history ? formatBytes(history.total_bytes) : '—'}</strong><p>비중은 정적 파일 합계 기준입니다. GLB 내부의 텍스처는 모델 전송량에 포함됩니다.</p></div>
    {#if history && history.categories.length > 0}
      <div class="categories" aria-label="종류별 정적 파일 전송량">
        {#each history.categories as category (category.category)}
          <div class="category-row">
            <span>{categoryLabels[category.category]}</span>
            <div class="bar-track" aria-hidden="true"><div style:width={`${percent(category.bytes)}%`}></div></div>
            <strong>{formatBytes(category.bytes)}</strong><small>{percent(category.bytes).toFixed(1)}%</small>
          </div>
        {/each}
      </div>
      <div class="table-scroll">
        <table aria-label="전송량 상위 20개 정적 파일">
          <thead><tr><th scope="col">파일 · 상위 20개</th><th scope="col">전송량</th><th scope="col">비중</th><th scope="col">요청</th><th scope="col">요청당 평균</th><th scope="col">304 응답</th></tr></thead>
          <tbody>
            {#each history.files as file (file.path)}
              <tr><td><span class="file-path">{file.path}</span><small>{categoryLabels[file.category]}</small></td><td>{formatBytes(file.bytes)}</td><td>{percent(file.bytes).toFixed(1)}%</td><td>{file.requests.toLocaleString('ko-KR')}</td><td>{formatBytes(file.requests ? file.bytes / file.requests : 0)}</td><td>{file.revalidations.toLocaleString('ko-KR')}</td></tr>
            {/each}
          </tbody>
        </table>
      </div>
      <p class="detail-note">요청에는 부분 다운로드·캐시 재검증이 포함됩니다. 304는 캐시가 유효해 본문을 다시 보내지 않은 응답입니다. 브라우저 캐시만 사용한 요청은 서버 로그에 남지 않습니다.</p>
    {:else}
      <div class="chart-empty" role="status"><strong>{loading ? '전송 기록을 불러오고 있어요' : '이 기간에 집계된 정적 파일 전송이 없습니다'}</strong><p>수집을 시작한 뒤 완료된 요청부터 기록합니다.</p></div>
    {/if}
    <div class="chart-footer"><span>{history ? `${formatDateTime(history.from)} — ${formatDateTime(history.until)} KST` : '기록 확인 중'}</span><span>{history?.status ? `마지막 로그 확인 ${formatDateTime(history.status.updated_at)} KST` : '첫 집계 대기 중'}</span></div>
    {#if history?.status}<p class="detail-note">수집 시작: {formatDateTime(history.status.started_at)} KST. 이전 로그는 소급 집계하지 않습니다.</p>{/if}
  {/if}
</section>

<style>
  .categories { display: grid; gap: 14px; margin: 24px 0; }
  .category-row { display: grid; grid-template-columns: 125px minmax(30px, 1fr) 100px 50px; align-items: center; gap: 12px; font-size: 12px; }
  .category-row strong, .category-row small { text-align: right; font-variant-numeric: tabular-nums; }
  .bar-track { height: 8px; border-radius: 6px; background: #edf3ef; overflow: hidden; }
  .bar-track div { height: 100%; background: #168878; border-radius: inherit; }
  .table-scroll { overflow-x: auto; }
  table { border-collapse: collapse; width: 100%; min-width: 760px; font-size: 12px; }
  th, td { text-align: right; border-bottom: 1px solid #e6ece9; padding: 14px 10px; font-variant-numeric: tabular-nums; white-space: nowrap; }
  th:first-child, td:first-child { text-align: left; width: 40%; }
  td:first-child { white-space: normal; min-width: 220px; }
  th { color: #75877f; font-weight: 500; }
  .file-path { overflow-wrap: anywhere; font-family: monospace; }
  td small { display: block; color: #75877f; margin-top: 5px; }
  .detail-note { font-size: 12px; color: #75877f; line-height: 1.8; }
  .chart-empty { min-height: 150px; }
  @media (max-width: 650px) { .category-row { grid-template-columns: 105px minmax(10px, 1fr) 85px 40px; gap: 6px; font-size: 11px; } }
</style>
