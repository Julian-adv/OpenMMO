<script lang="ts">
  import FamilyDailyChart from './FamilyDailyChart.svelte'
  import MetricsError from './MetricsError.svelte'
  import { formatCount, formatDateTime } from './metrics'
  import type { MetricsResource } from './metricsResource.svelte'
  import type { Families } from './families'

  let { resource }: { resource: MetricsResource<Families> } = $props()
  let { history: families, loading, refreshing, error, refresh } = $derived(resource)
  let today = $derived(families?.daily.at(-1))
  let weekFounded = $derived(families?.daily.slice(-7).reduce((sum, day) => sum + day.founded, 0) ?? 0)
</script>

<section class="chart-panel" aria-labelledby="families-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="families-title">가문</h2>
      <p>창설 · 통합 · 해산 횟수와 가문원이 많은 가문</p>
    </div>
  </div>
  <MetricsError {error} until={families?.until} {refreshing} {refresh} />
  {#if families && today}
    <dl class="tiles">
      <div class="primary"><dt>현재 가문</dt><dd>{formatCount(families.active)}<small>개 · 가문원 {formatCount(families.members)}명</small></dd></div>
      <div><dt>오늘 창설</dt><dd>{formatCount(today.founded)}<small>회</small></dd></div>
      <div><dt>최근 7일 창설</dt><dd>{formatCount(weekFounded)}<small>회</small></dd></div>
      <div><dt>누적 창설</dt><dd>{formatCount(families.total.founded)}<small>회</small></dd></div>
      <div><dt>통합된 가문</dt><dd>{formatCount(families.total.merged)}<small>개</small></dd></div>
      <div><dt>해산한 가문</dt><dd>{formatCount(families.total.dissolved)}<small>개</small></dd></div>
    </dl>
    <h3 class="sub-heading">일별 추이 <small>최근 {families.daily.length}일 · KST</small></h3>
    <FamilyDailyChart days={families.daily} />
    <h3 id="families-top-title" class="sub-heading">가문원 상위 10개 가문</h3>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" tabindex="0" role="region" aria-label="가문원 상위 가문 표">
      <table aria-labelledby="families-top-title">
        <thead>
          <tr>
            <th scope="col" class="num">순위</th><th scope="col">가문</th><th scope="col">가주</th>
            <th scope="col" class="fill">원로</th><th scope="col" class="num">가문원</th>
            <th scope="col" class="num">영지</th><th scope="col">창설</th>
          </tr>
        </thead>
        <tbody>
          {#each families.top as family, index (family.name)}
            <tr>
              <td class="num">{index + 1}</td>
              <th scope="row" class="name nowrap">{family.name}</th>
              <td class="name nowrap">{family.head ?? '—'}</td>
              <td>{family.elders.length ? family.elders.join(', ') : '—'}</td>
              <td class="num strong">{formatCount(family.members)}명</td>
              <td class="num">{formatCount(family.estates)}곳</td>
              <td class="nowrap">{formatDateTime(family.created_at)} KST</td>
            </tr>
          {:else}
            <tr><td colspan="7" class="empty-row">아직 창설된 가문이 없어요</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '가문 현황을 불러오고 있어요' : '가문 현황에 연결할 수 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : '연결이 복구되면 표가 자동으로 갱신됩니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{families ? `${formatDateTime(families.collection_started_at)} KST 전에 통합·해산으로 사라진 가문의 창설·해산은 누적에 없음` : '기록 확인 중'}</span>
    <span>1시간 갱신</span>
  </div>
</section>

<style>
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 12px; margin: 18px 0; }
  .tiles > div { padding: 14px; border: 1px solid #e2e9e5; border-radius: 12px; }
  .tiles .primary { background: #eaf3ef; border-color: #cfe3da; color: #166d5e; }
  dt { color: #74857d; font-size: 11px; font-weight: 500; }
  .primary dt { color: inherit; }
  dd { margin: 6px 0 0; font-size: 26px; font-weight: 600; font-variant-numeric: tabular-nums; }
  dd small { margin-left: 5px; color: #87968f; font-size: 11px; font-weight: 400; }
  .table-scroll { overflow: auto; margin: 18px 0; }
  table { width: 100%; min-width: 640px; border-collapse: collapse; font-size: 13px; }
  th, td { padding: 11px 12px; border-bottom: 1px solid #edf1ee; text-align: left; vertical-align: top; }
  thead th { background: #f6f8f7; color: #74857d; font-size: 11px; font-weight: 500; white-space: nowrap; }
  tbody tr:last-child > * { border-bottom: 0; }
  .name { min-width: 120px; font-weight: 500; }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .strong { font-weight: 600; }
  .nowrap { white-space: nowrap; }
  .fill { width: 100%; }
  .sub-heading { margin: 0; font-size: 13px; font-weight: 600; }
  .sub-heading small { margin-left: 6px; color: #74857d; font-size: 11px; font-weight: 400; }
  .empty-row { color: #74857d; text-align: center; }
  .chart-empty { min-height: 140px; }
</style>
