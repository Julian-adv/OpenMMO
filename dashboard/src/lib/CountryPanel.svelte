<script lang="ts">
  import type { MetricsResource } from './metricsResource.svelte'
  import MetricsError from './MetricsError.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { formatCount, formatDateTime, uniquePeriods, type UniqueHours } from './metrics'
  import { countryName, share, type CountryStats } from './countries'

  let { hours = $bindable(), resource }: {
    hours: UniqueHours
    resource: MetricsResource<CountryStats>
  } = $props()
  let { history: stats, loading, refreshing, error, refresh } = $derived(resource)
  let period = $derived(uniquePeriods.find((option) => option.hours === hours)!)
  let domestic = $derived(stats?.countries.find((entry) => entry.country === 'KR'))
</script>

<section class="chart-panel" aria-labelledby="country-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="country-title">국가별 접속 계정</h2>
      <p>매일 자정 기준, 직전 {period.label} 동안 접속한 계정의 접속 IP 국가 · 공식 NPC 제외</p>
    </div>
    <PeriodFilter bind:hours options={uniquePeriods} label="국가별 집계 기간" />
  </div>
  <MetricsError {error} until={stats?.until} {refreshing} {refresh} />
  <div class="metric-summary">
    <span>마지막 일별 집계 · 직전 {period.label} 한국 계정 비율{error ? ' · 갱신 중단' : ''}</span>
    <strong>{stats && stats.accounts > 0 ? formatCount(share(domestic?.accounts ?? 0, stats.accounts)) : '—'}<small>%</small></strong>
    <p>{!stats ? '기록 확인 중' : stats.last_aggregated_at === null ? '첫 일별 집계를 기다리고 있어요' : `전체 ${formatCount(stats.accounts)}계정 중 한국 ${formatCount(domestic?.accounts ?? 0)}계정 · 마지막 집계 기준: ${formatDateTime(stats.last_aggregated_at)} KST`}</p>
  </div>
  {#if stats && stats.last_aggregated_at !== null && stats.collection_started_at > stats.last_aggregated_at - (stats.until - stats.from)}
    <p class="chart-notice">{formatDateTime(stats.collection_started_at)} KST부터 수집한 기록만 포함합니다.</p>
  {/if}
  {#if stats && stats.countries.length > 0}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" tabindex="0" role="region" aria-label="국가별 접속 계정 표">
      <table aria-labelledby="country-title">
        <thead>
          <tr><th scope="col">국가</th><th scope="col">계정</th><th scope="col">비율</th><th scope="col">접속 횟수</th><th scope="col">현재 접속</th></tr>
        </thead>
        <tbody>
          {#each stats.countries as entry (entry.country)}
            <tr>
              <th scope="row">{countryName(entry.country)} <span class="code">{entry.country}</span></th>
              <td>{formatCount(entry.accounts)}</td>
              <td>{formatCount(share(entry.accounts, stats.accounts))}%</td>
              <td>{formatCount(entry.sessions)}</td>
              <td>{formatCount(entry.current_accounts)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '국가별 기록을 불러오고 있어요' : error ? '국가별 기록에 연결할 수 없어요' : stats?.last_aggregated_at == null ? '첫 일별 집계를 기다리고 있어요' : '이 기간에 기록된 접속이 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 표가 자동으로 갱신됩니다.' : '유니크 접속 계정과 함께 매일 한국 시간 자정 이후 집계합니다.'}</p>
    </div>
  {/if}
  <p class="country-note">접속 경로의 국가이며 거주 국가와 다를 수 있습니다(VPN·클라우드). 여러 국가에서 접속한 계정은 각 국가에 포함되어 비율 합계가 100%를 넘을 수 있습니다.</p>
  <div class="chart-footer">
    <span>하루 한 번 집계 · 현재 접속 {stats ? formatCount(stats.current_accounts) : '—'}계정은 5분 갱신</span>
    <span><a href="https://db-ip.com" target="_blank" rel="noreferrer">IP Geolocation by DB-IP</a></span>
  </div>
</section>

<style>
  .table-scroll { overflow-x: auto; margin: 20px 0 0; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; font-variant-numeric: tabular-nums; }
  th, td { padding: 9px 12px; border-bottom: 1px solid #e6ece9; text-align: right; white-space: nowrap; }
  th:first-child { text-align: left; }
  thead th { color: #74857d; font-size: 11px; font-weight: 600; }
  tbody th { color: #314b3f; font-weight: 600; }
  td { color: #4c6157; }
  .code { color: #84938b; font-size: 11px; font-weight: 400; }
  .country-note { margin: 16px 0; color: #84938b; font-size: 11px; }
  .chart-empty { min-height: 210px; }
  a { color: inherit; }
</style>
