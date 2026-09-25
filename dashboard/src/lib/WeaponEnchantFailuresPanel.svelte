<script lang="ts">
  import MetricsError from './MetricsError.svelte'
  import { formatCount, formatDateTime } from './metrics'
  import type { MetricsResource } from './metricsResource.svelte'
  import { weaponEnchantFailureKey, type WeaponEnchantFailures } from './weaponEnchantFailures'

  let { resource }: { resource: MetricsResource<WeaponEnchantFailures> } = $props()
  let { history: failures, loading, refreshing, error, refresh } = $derived(resource)
</script>

<section class="chart-panel" aria-labelledby="weapon-enchant-failures-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="weapon-enchant-failures-title">무기 인챈트 실패 내역</h2>
      <p>같은 캐릭터·무기·강화 단계별 누적 횟수 · 최근 실패순 최대 10개 항목</p>
    </div>
  </div>
  <MetricsError {error} until={failures?.until} {refreshing} {refresh} />
  {#if failures && failures.entries.length > 0}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" tabindex="0" role="region" aria-label="무기 인챈트 실패 내역 표">
      <table aria-labelledby="weapon-enchant-failures-title">
        <thead><tr><th scope="col">최근 실패 시각 (KST)</th><th scope="col">캐릭터</th><th scope="col">소멸한 무기</th><th scope="col" class="attempt">실패한 강화 시도</th><th scope="col" class="count">실패 횟수</th></tr></thead>
        <tbody>
          {#each failures.entries as entry (weaponEnchantFailureKey(entry))}
            <tr>
              <td class="timestamp">{formatDateTime(entry.timestamp)}</td>
              <th scope="row" class="name">{entry.name}</th>
              <td class="name"><span class="enchant">+{entry.enchant}</span> {entry.item_name}</td>
              <td class="attempt">+{entry.enchant} → +{entry.enchant + 1}<span class="destroyed">소멸</span></td>
              <td class="count">{formatCount(entry.failure_count)}회</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '무기 인챈트 실패 내역을 불러오고 있어요' : error ? '실패 내역에 연결할 수 없어요' : '아직 기록된 무기 인챈트 실패가 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 표가 자동으로 갱신됩니다.' : '강화 실패로 무기가 소멸하면 이곳에 표시됩니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{failures ? `${formatDateTime(failures.collection_started_at)} KST부터 수집 · 공식 NPC 제외` : '기록 확인 중'}</span>
    <span>{failures ? `${failures.entries.length}개 항목 · 1시간 갱신` : '최근 최대 10개 항목'}</span>
  </div>
</section>

<style>
  .table-scroll { overflow: auto; margin: 18px 0; }
  table { width: 100%; min-width: 760px; border-collapse: collapse; font-size: 13px; }
  th, td { padding: 13px 12px; border-bottom: 1px solid #edf1ee; text-align: left; }
  thead th { background: #f6f8f7; color: #74857d; font-size: 11px; font-weight: 500; white-space: nowrap; }
  tbody tr:last-child > * { border-bottom: 0; }
  .timestamp { color: #74857d; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .name { font-weight: 500; overflow-wrap: anywhere; }
  .enchant { color: #166d5e; font-weight: 600; white-space: nowrap; }
  .attempt { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .count { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; font-weight: 600; }
  .destroyed { display: inline-block; margin-left: 10px; padding: 3px 7px; border-radius: 5px; background: #fbeeea; color: #a54830; font-size: 11px; }
  .chart-empty { min-height: 180px; }
</style>
