<script lang="ts">
  import MetricsError from './MetricsError.svelte'
  import { formatCount, formatDateTime } from './metrics'
  import type { MetricsResource } from './metricsResource.svelte'
  import type { CombatAuditTargets } from './combatAuditTargets'

  let { resource }: { resource: MetricsResource<CombatAuditTargets> } = $props()
  let { history: targets, loading, refreshing, error, refresh } = $derived(resource)
</script>

<section class="chart-panel" aria-labelledby="combat-audit-targets-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="combat-audit-targets-title">전투 기록 추적 대상</h2>
      <p>현재 서버에 적용된 대상 · 오프라인 캐릭터 포함</p>
    </div>
    {#if targets}<span class="target-count">{formatCount(targets.entries.length)}명</span>{/if}
  </div>
  <MetricsError {error} until={targets?.until} {refreshing} {refresh} />
  {#if targets && targets.entries.length > 0}
    <ul class="targets" aria-label="전투 기록 추적 캐릭터">
      {#each targets.entries as target (target.character_id)}
        <li>
          <strong class:unknown={target.name === null}>{target.name ?? '이름을 확인할 수 없는 캐릭터'}</strong>
          <span>캐릭터 ID {target.character_id}</span>
        </li>
      {/each}
    </ul>
  {:else}
    <div class="chart-empty" role="status">
      {#if loading}
        <strong>추적 대상을 불러오고 있어요</strong>
        <p>잠시만 기다려 주세요.</p>
      {:else if error}
        <strong>추적 대상에 연결할 수 없어요</strong>
        <p>연결이 복구되면 목록이 자동으로 갱신됩니다.</p>
      {:else}
        <strong>현재 전투 기록을 추적하는 캐릭터가 없어요</strong>
        <p>서버에 추적 대상이 적용되면 이곳에 표시됩니다.</p>
      {/if}
    </div>
  {/if}
  <p class="target-note">대상 변경은 서버의 10분 갱신 주기에 반영됩니다.</p>
  <div class="chart-footer">
    <span>{targets ? `${formatDateTime(targets.until)} KST 확인` : '추적 대상 확인 중'}</span>
    <span>1분 갱신</span>
  </div>
</section>

<style>
  .target-count { padding: 5px 10px; border-radius: 6px; background: #edf4f0; color: #166d5e; font-size: 12px; font-weight: 600; }
  .targets { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 230px), 1fr)); gap: 10px; list-style: none; margin: 20px 0; padding: 0; }
  li { display: flex; flex-direction: column; gap: 6px; padding: 13px 15px; background: #f6f8f7; border-radius: 8px; }
  strong { color: #314b3f; font-size: 13px; font-weight: 600; overflow-wrap: anywhere; }
  .unknown { color: #74857d; font-weight: 400; }
  li span { color: #74857d; font-size: 11px; font-variant-numeric: tabular-nums; }
  .target-note { margin: 16px 0; color: #84938b; font-size: 11px; }
  .chart-empty { min-height: 140px; }
</style>
