<script lang="ts">
  import MetricsError from './MetricsError.svelte'
  import { formatCount, formatDateTime } from './metrics'
  import type { MetricsResource } from './metricsResource.svelte'
  import { describeChances, perTenThousandKills, totalHoldings, type RareDrops } from './rareDrops'

  let { resource }: { resource: MetricsResource<RareDrops> } = $props()
  let { history: drops, loading, refreshing, error, refresh } = $derived(resource)
</script>

<section class="chart-panel" aria-labelledby="rare-drops-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="rare-drops-title">희귀 아이템 드롭</h2>
      <p>설정된 드롭 확률, 처치 10,000회당 실측·기대 드롭 수, 누적 드롭과 현재 서버 보유량</p>
    </div>
  </div>
  <MetricsError {error} until={drops?.until} {refreshing} {refresh} />
  {#if drops}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" tabindex="0" role="region" aria-label="희귀 아이템 드롭 표">
      <table aria-labelledby="rare-drops-title">
        <thead>
          <tr>
            <th scope="col">아이템</th><th scope="col">드롭 확률</th><th scope="col" class="num">대상 처치</th>
            <th scope="col" class="num">10,000킬당 실측</th><th scope="col" class="num">10,000킬당 기대</th>
            <th scope="col" class="num">누적 드롭</th><th scope="col" class="num">현재 보유</th>
          </tr>
        </thead>
        <tbody>
          {#each drops.items as item (item.item_def_id)}
            <tr>
              <th scope="row" class="name">{item.name}</th>
              <td class="chances">{#each describeChances(item) as line (line)}<span>{line}</span>{/each}</td>
              <td class="num">{formatCount(item.kills)}</td>
              <td class="num strong">{perTenThousandKills(item.kill_drops, item.kills) ?? '—'}개</td>
              <td class="num">{perTenThousandKills(item.expected_kill_drops, item.kills) ?? '—'}개</td>
              <td class="num">
                <strong>{formatCount(item.kill_drops + item.other_drops)}개</strong>
                <span class="sub">처치 {formatCount(item.kill_drops)} · 상자·통 {formatCount(item.other_drops)}</span>
              </td>
              <td class="num">
                <strong>{formatCount(totalHoldings(item.holdings))}개</strong>
                <span class="sub">소지 {formatCount(item.holdings.inventory)} · 창고 {formatCount(item.holdings.storage)} · 바닥 {formatCount(item.holdings.ground)} · NPC {formatCount(item.holdings.npc_inventory)}</span>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '드롭 현황을 불러오고 있어요' : '드롭 현황에 연결할 수 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : '연결이 복구되면 표가 자동으로 갱신됩니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{drops ? `${formatDateTime(drops.collection_started_at)} KST부터 집계 · 전체 처치 ${formatCount(drops.total_kills)}회` : '기록 확인 중'}</span>
    <span>보유량은 저장 주기(약 30초)만큼 늦을 수 있음 · 1시간 갱신</span>
  </div>
</section>

<style>
  .table-scroll { overflow: auto; margin: 18px 0; }
  table { width: 100%; min-width: 860px; border-collapse: collapse; font-size: 13px; }
  th, td { padding: 13px 12px; border-bottom: 1px solid #edf1ee; text-align: left; vertical-align: top; }
  thead th { background: #f6f8f7; color: #74857d; font-size: 11px; font-weight: 500; white-space: nowrap; }
  tbody tr:last-child > * { border-bottom: 0; }
  .name { font-weight: 500; overflow-wrap: anywhere; }
  .chances span { display: block; color: #4d5d56; white-space: nowrap; }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .strong, .num strong { font-weight: 600; }
  .sub { display: block; margin-top: 3px; color: #74857d; font-size: 11px; }
  .chart-empty { min-height: 180px; }
</style>
