<script lang="ts">
  import MetricsError from './MetricsError.svelte'
  import { formatCount, formatDateTime, formatPercent } from './metrics'
  import type { MetricsResource } from './metricsResource.svelte'
  import {
    bossHealthLeft, failures, outcomeLabels, raiderTags, raidMinutes, successRate, type FieldRaids,
  } from './fieldRaids'

  let { resource }: { resource: MetricsResource<FieldRaids> } = $props()
  let { history: raids, loading, refreshing, error, refresh } = $derived(resource)
</script>

<section class="chart-panel" aria-labelledby="field-raids-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="field-raids-title">필드 레이드</h2>
      <p>제단 소환 횟수와 결과 · 성공률은 끝난 레이드 기준</p>
    </div>
  </div>
  <MetricsError {error} until={raids?.until} {refreshing} {refresh} />
  {#if raids}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" tabindex="0" role="region" aria-label="필드 레이드 표">
      <table aria-labelledby="field-raids-title">
        <thead>
          <tr>
            <th scope="col">보스</th><th scope="col" class="num">소환</th><th scope="col" class="num">성공</th>
            <th scope="col" class="num">실패</th><th scope="col" class="num">성공률</th><th scope="col">최근 성공</th>
          </tr>
        </thead>
        <tbody>
          {#each raids.bosses as boss (boss.boss)}
            <tr>
              <th scope="row" class="name">{boss.name}</th>
              <td class="num strong">{formatCount(boss.summons)}회</td>
              <td class="num strong">{formatCount(boss.defeated)}회</td>
              <td class="num">
                {formatCount(failures(boss))}회
                <span class="sub">
                  {outcomeLabels.wiped} {formatCount(boss.wiped)} · {outcomeLabels.moon_set} {formatCount(boss.moon_set)}
                  · {outcomeLabels.dawn} {formatCount(boss.dawn)}
                </span>
              </td>
              <td class="num">{formatPercent(successRate(boss))}</td>
              <td>{boss.last_defeated_at === null ? '—' : `${formatDateTime(boss.last_defeated_at)} KST`}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <h3 id="recent-raids-title" class="sub-heading">최근 레이드</h3>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" tabindex="0" role="region" aria-label="최근 레이드 표">
      <table aria-labelledby="recent-raids-title">
        <thead>
          <tr>
            <th scope="col">끝난 시각</th><th scope="col">보스</th><th scope="col">결과</th>
            <th scope="col" class="num">걸린 시간</th><th scope="col" class="num">남은 HP</th><th scope="col">참가자</th>
          </tr>
        </thead>
        <tbody>
          {#each raids.recent as raid (`${raid.boss}-${raid.ended_at}-${raid.summoned_at}`)}
            <tr>
              <td>{formatDateTime(raid.ended_at)} KST</td>
              <th scope="row" class="name">{raid.name}</th>
              <td class="strong">{outcomeLabels[raid.outcome]}</td>
              <td class="num">{formatCount(raidMinutes(raid))}분</td>
              <td class="num">{formatPercent(bossHealthLeft(raid))}</td>
              <td>
                {#if raid.raiders.length === 0}—{/if}
                <ul class="raiders">
                  {#each raid.raiders as raider (raider.character_id)}
                    <li>
                      <span class="name">{raider.name}</span>
                      {#each raiderTags(raid, raider) as tag (tag)}<span class="tag">{tag}</span>{/each}
                      <span class="sub">
                        준 피해 {formatCount(raider.dealt)} · 받은 피해 {formatCount(raider.taken)} · 사망 {formatCount(raider.deaths)}
                      </span>
                    </li>
                  {/each}
                </ul>
              </td>
            </tr>
          {:else}
            <tr><td colspan="6" class="empty-row">아직 끝난 레이드 기록이 없어요</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '레이드 기록을 불러오고 있어요' : '레이드 기록에 연결할 수 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : '연결이 복구되면 표가 자동으로 갱신됩니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{raids ? `${formatDateTime(raids.collection_started_at)} KST부터 집계` : '기록 확인 중'}</span>
    <span>1시간 갱신</span>
  </div>
</section>

<style>
  .table-scroll { overflow: auto; margin: 18px 0; }
  table { width: 100%; min-width: 640px; border-collapse: collapse; font-size: 13px; }
  th, td { padding: 13px 12px; border-bottom: 1px solid #edf1ee; text-align: left; vertical-align: top; }
  thead th { background: #f6f8f7; color: #74857d; font-size: 11px; font-weight: 500; white-space: nowrap; }
  tbody tr:last-child > * { border-bottom: 0; }
  .name { font-weight: 500; overflow-wrap: anywhere; }
  .num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .strong { font-weight: 600; }
  .sub { display: block; margin-top: 3px; color: #74857d; font-size: 11px; }
  .sub-heading { margin: 0; font-size: 13px; font-weight: 600; }
  .raiders { margin: 0; padding: 0; list-style: none; display: grid; gap: 8px; }
  .tag { margin-left: 6px; padding: 1px 6px; border-radius: 999px; background: #edf1ee; color: #4f6158; font-size: 11px; }
  .empty-row { color: #74857d; text-align: center; }
  .chart-empty { min-height: 140px; }
</style>
