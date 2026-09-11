<script lang="ts">
  import MetricsError from './MetricsError.svelte'
  import { formatDateTime, type LevelLeaderboard } from './metrics'

  let { leaderboard, loading, refreshing, error, refresh }: {
    leaderboard: LevelLeaderboard | null
    loading: boolean
    refreshing: boolean
    error: string
    refresh: () => void
  } = $props()
</script>

<section class="chart-panel" aria-labelledby="level-leaderboard-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="level-leaderboard-title">레벨 상위 10명</h2>
      <p>전체 캐릭터 기준 · 오프라인 포함 · 공식 NPC 제외</p>
    </div>
    <span class="leaderboard-tag">TOP 10</span>
  </div>
  <MetricsError {error} until={leaderboard?.timestamp} {refreshing} {refresh} />
  {#if leaderboard && leaderboard.entries.length > 0}
    <table aria-labelledby="level-leaderboard-title">
      <thead>
        <tr><th scope="col" class="rank">순위</th><th scope="col">캐릭터</th><th scope="col" class="level">레벨</th></tr>
      </thead>
      <tbody>
        {#each leaderboard.entries as entry, index (entry.name)}
          <tr>
            <td class="rank"><span class="rank-badge" class:podium={index < 3}>{index + 1}</span></td>
            <th scope="row" class="character-name">
              {entry.name}
              {#if entry.account_first_rank !== index + 1}
                {@const firstCharacter = leaderboard.entries[entry.account_first_rank - 1]}
                <span class="account-character">({firstCharacter.name})</span>
              {/if}
            </th>
            <td class="level"><span>Lv.</span> {entry.level.toLocaleString('ko-KR')}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="ranking-note">같은 레벨은 경험치가 높은 순으로 표시합니다. 경험치도 같으면 먼저 생성된 캐릭터가 앞섭니다.</p>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '레벨 순위를 불러오고 있어요' : error ? '순위에 연결할 수 없어요' : '아직 순위에 표시할 캐릭터가 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 순위가 자동으로 갱신됩니다.' : '캐릭터가 생성되면 레벨 순위가 표시됩니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{leaderboard ? `${formatDateTime(leaderboard.timestamp)} KST 조회${error ? ' · 갱신 중단' : ''}` : '순위 확인 중'}</span>
    <span>저장된 레벨 기준 · 30초마다 갱신</span>
  </div>
</section>

<style>
  .leaderboard-tag { color: #167b6c; background: #eaf3ef; border-radius: 6px; padding: 6px 10px; font-size: 10px; font-weight: 600; letter-spacing: 1px; }
  table { width: 100%; border-collapse: collapse; table-layout: fixed; margin-top: 24px; font-size: 13px; }
  th, td { padding: 13px 12px; border-bottom: 1px solid #edf1ee; text-align: left; }
  thead th { background: #f6f8f7; color: #74857d; font-size: 11px; font-weight: 500; }
  tbody tr:last-child > * { border-bottom: 0; }
  .rank { width: 72px; text-align: center; font-variant-numeric: tabular-nums; }
  .rank-badge { display: inline-grid; place-items: center; width: 28px; height: 28px; color: #899b90; font-weight: 600; border-radius: 8px; }
  .podium { background: #eaf3ef; color: #166d5e; }
  .character-name { font-weight: 500; overflow-wrap: anywhere; }
  .account-character { color: #74857d; font-size: 11px; font-weight: 400; }
  .level { width: 100px; text-align: right; font-variant-numeric: tabular-nums; }
  td.level { color: #166d5e; font-weight: 600; }
  .level span { color: #899b90; font-size: 10px; font-weight: 400; }
  .ranking-note { margin: 8px 0 22px; color: #899b90; font-size: 10px; line-height: 1.8; word-break: keep-all; }
  @media (max-width: 600px) {
    .chart-heading { flex-direction: row; align-items: center; gap: 12px; }
    .chart-heading p { font-size: 10px; line-height: 1.8; }
    th, td { padding: 11px 6px; }
    .rank { width: 44px; }
    .level { width: 72px; }
    .leaderboard-tag { font-size: 9px; padding: 5px 8px; }
  }
</style>
