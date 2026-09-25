<script lang="ts">
  import type { MetricsResource } from './metricsResource.svelte'
  import MetricsError from './MetricsError.svelte'
  import { formatDateTime } from './metrics'
  import { formatBytes } from './traffic'
  import { formatPercent, usagePercent, type HardwareStatus } from './hardware'

  let { resource }: { resource: MetricsResource<HardwareStatus> } = $props()
  let { history, loading, refreshing, error, refresh } = $derived(resource)
  let latest = $derived(history?.latest)
  let agent = $derived(latest && latest.agent.instances > 0 ? latest.agent : null)
  let memoryPercent = $derived(latest ? usagePercent(latest.memory_used_bytes, latest.memory_total_bytes) : null)
  const load = (value: number) => value.toLocaleString('ko-KR', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
</script>

<section class="chart-panel" aria-labelledby="hardware-title" aria-busy={loading}>
  <div class="chart-heading">
    <div><h2 id="hardware-title">서버 하드웨어 상태</h2><p>서버 실행 환경 기준 · 1분마다 수집 및 갱신</p></div>
    <span class="live-tag">{error || history && !history.available ? '수집 확인 필요' : loading ? '연결 중' : '1분 갱신'}</span>
  </div>
  <MetricsError {error} until={latest?.timestamp} {refreshing} {refresh} />
  {#if history && !history.available}
    <p class="chart-notice">{latest ? '하드웨어 수집이 지연되거나 중단되었습니다. 마지막으로 수집한 값을 표시합니다.' : '하드웨어 정보를 아직 수집하지 못했습니다.'}</p>
  {/if}
  <div class="hardware-summary">
    <div>
      <span>서버 CPU 사용률</span>
      <strong>{formatPercent(latest?.cpu_percent)}</strong>
      <small>{latest ? `${latest.cpu_count}개 논리 코어 전체 기준` : '수집 대기 중'}</small>
      {#if latest?.cpu_percent != null}<meter min="0" max="100" value={latest.cpu_percent} aria-label="서버 CPU 사용률"></meter>{/if}
    </div>
    <div>
      <span>서버 메모리 사용률</span>
      <strong>{formatPercent(memoryPercent)}</strong>
      <small>{latest ? `${formatBytes(latest.memory_used_bytes)} / ${formatBytes(latest.memory_total_bytes)}` : '수집 대기 중'}</small>
      {#if memoryPercent != null}<meter min="0" max="100" value={memoryPercent} aria-label="서버 메모리 사용률"></meter>{/if}
    </div>
    <div>
      <span>agent-client CPU</span>
      <strong>{formatPercent(agent?.cpu_percent)}</strong>
      <small>자식 프로세스 포함 · 코어 1개 = 100%</small>
    </div>
    <div>
      <span>agent-client 메모리</span>
      <strong>{agent ? formatBytes(agent.memory_bytes) : '—'}</strong>
      <small>{agent ? `${agent.instances}개 agent-client · 자식 포함 ${agent.process_count}개 프로세스` : latest ? '실행 중인 agent-client가 감지되지 않았습니다' : '수집 대기 중'}</small>
    </div>
  </div>
  {#if latest?.load_average}
    <div class="load-average" aria-label="CPU 평균 부하">
      <span>평균 부하 (Load average)</span>
      {#each latest.load_average as value, index (index)}<span>{[1, 5, 15][index]}분 <b>{load(value)}</b></span>{/each}
    </div>
  {/if}
  <p class="hardware-note">{latest && latest.cpu_percent === null ? 'CPU 사용률은 첫 수집 후 1분부터 표시합니다.' : 'CPU 사용률은 직전 수집 구간의 평균입니다.'} 메모리는 재사용 가능한 캐시를 제외하며, agent-client는 자식 프로세스를 포함한 상주 메모리(RSS) 합계로 공유 메모리가 중복될 수 있습니다.</p>
  <div class="disk-heading"><h3>디스크 여유 공간</h3><span>일반 사용자에게 남은 용량 기준</span></div>
  {#if latest?.disks.length}
    <div class="disk-table-wrap">
      <table>
        <thead><tr><th scope="col">마운트 경로</th><th scope="col">남은 용량</th><th scope="col">전체 용량</th><th scope="col">여유 비율</th></tr></thead>
        <tbody>
          {#each latest.disks as disk, index (index)}
            {@const freePercent = usagePercent(disk.available_bytes, disk.total_bytes)}
            <tr>
              <th scope="row">{disk.mount}</th>
              <td class:low-space={freePercent < 10}>{formatBytes(disk.available_bytes)}</td>
              <td>{formatBytes(disk.total_bytes)}</td>
              <td><div class="disk-capacity"><meter min="0" max="100" low="10" high="20" optimum="100" value={freePercent} aria-label={`${disk.mount} 디스크 여유 비율`}></meter><span>{formatPercent(freePercent)}</span></div></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <p class="disk-empty">{loading ? '디스크 정보를 불러오고 있어요' : '조회 가능한 디스크 정보가 없습니다'}</p>
  {/if}
  <div class="chart-footer">
    <span>{latest ? `${formatDateTime(latest.timestamp)} KST 기준` : '첫 수집 대기 중'}</span>
    <span>최근 수집값</span>
  </div>
</section>

<style>
  .hardware-summary { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 20px; padding: 22px 0 16px; }
  .hardware-summary > div { display: flex; flex-direction: column; gap: 8px; }
  .hardware-summary span, .hardware-summary small, .hardware-note, .load-average, .disk-heading span, .disk-empty { color: #75877f; font-size: 11px; line-height: 1.8; }
  .hardware-summary strong { color: #166d5e; font-size: 27px; font-weight: 600; font-variant-numeric: tabular-nums; }
  meter { width: 100%; height: 10px; }
  meter::-webkit-meter-bar { background: #edf1ee; border: 0; border-radius: 5px; }
  meter::-webkit-meter-optimum-value { background: #298f79; }
  meter::-webkit-meter-suboptimum-value { background: #b87936; }
  meter::-webkit-meter-even-less-good-value { background: #bb5a43; }
  meter::-moz-meter-bar { background: #298f79; }
  .load-average { display: flex; flex-wrap: wrap; gap: 10px 20px; padding-top: 4px; }
  .load-average b { margin-left: 5px; color: #243c37; font-variant-numeric: tabular-nums; }
  .hardware-note { margin: 12px 0 20px; }
  .disk-heading { display: flex; flex-wrap: wrap; align-items: baseline; justify-content: space-between; gap: 8px; }
  .disk-heading h3 { font-size: 13px; color: #243c37; }
  .disk-table-wrap { overflow-x: auto; margin: 6px 0 18px; }
  table { width: 100%; border-collapse: collapse; font-size: 12px; font-variant-numeric: tabular-nums; }
  th, td { padding: 12px 10px; text-align: right; border-bottom: 1px solid #edf1ee; white-space: nowrap; }
  th:first-child { text-align: left; padding-left: 0; }
  thead th { color: #75877f; font-size: 11px; font-weight: 500; }
  tbody th { font-weight: 500; white-space: normal; overflow-wrap: anywhere; }
  .disk-capacity { display: flex; align-items: center; gap: 12px; justify-content: flex-end; }
  .disk-capacity meter { width: 80px; }
  .disk-capacity span { min-width: 45px; }
  .low-space { color: #a44d35; font-weight: 600; }
  .disk-empty { padding: 10px 0 20px; }
  @media (max-width: 950px) { .hardware-summary { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  @media (max-width: 480px) {
    .hardware-summary { gap: 20px 12px; }
    .hardware-summary strong { font-size: 22px; }
    .disk-capacity meter { display: none; }
    th, td { padding: 10px 6px; font-size: 11px; }
  }
</style>
