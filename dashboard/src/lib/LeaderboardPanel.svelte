<script lang="ts" generics="M extends LeaderboardMetric">
  import MetricsError from './MetricsError.svelte'
  import GoldAmount from './GoldAmount.svelte'
  import { leaderboardMetrics, type CharacterLeaderboard, type LeaderboardMetric } from './metrics'

  let { metric, leaderboard, colors, selectedCharacter = $bindable(), loading, refreshing, error, refresh }: {
    metric: M
    leaderboard: CharacterLeaderboard<M> | null
    colors: Record<string, string>
    selectedCharacter: string | null
    loading: boolean
    refreshing: boolean
    error: string
    refresh: () => void
  } = $props()
  let { label, columnLabel, description, enchantPrefix } = $derived(leaderboardMetrics[metric])
  let titleId = $derived(`${metric}-leaderboard-title`)
  let weaponEnchant = $derived(metric === 'weapon_enchant')
</script>

<section class="chart-panel" aria-labelledby={titleId} aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id={titleId}>{label} {weaponEnchant ? '보유 현황' : '상위 10명'}</h2>
      {#if description}<p>{description}</p>{/if}
    </div>
    <span class="leaderboard-tag">{weaponEnchant ? '+7 이상' : 'TOP 10'}</span>
  </div>
  <MetricsError {error} until={leaderboard?.timestamp} {refreshing} {refresh} />
  {#if leaderboard && leaderboard.entries.length > 0}
    <div class:scrollable={weaponEnchant} role="region" aria-labelledby={titleId}>
      <table aria-labelledby={titleId}>
        <thead>
          <tr>{#if !weaponEnchant}<th scope="col" class="rank">순위</th>{/if}<th scope="col">캐릭터</th><th scope="col" class="value" class:gold={metric === 'gold'}>{columnLabel}</th></tr>
        </thead>
        <tbody>
          {#each leaderboard.entries as entry, index (entry.name)}
            <tr class:selected={selectedCharacter === entry.name}>
              {#if !weaponEnchant}<td class="rank"><span class="rank-badge" class:podium={index < 3}>{index + 1}</span></td>{/if}
              <th scope="row" class="character-name">
                <button class="character-button" aria-pressed={selectedCharacter === entry.name} onclick={() => { selectedCharacter = selectedCharacter === entry.name ? null : entry.name }}>
                  <i style:background={colors[entry.name]} aria-hidden="true"></i>{entry.name}
                </button>
                {#if entry.account_first_rank !== index + 1}
                  {@const firstCharacter = leaderboard.entries[entry.account_first_rank - 1]}
                  <span class="account-character">({firstCharacter.name})</span>
                {/if}
              </th>
              <td class="value" class:gold={metric === 'gold'}>{#if metric === 'gold'}<GoldAmount copper={entry[metric]} />{:else if metric === 'land_plots'}{entry[metric].toLocaleString('ko-KR')} <span>필지</span>{:else if enchantPrefix}{enchantPrefix}{entry[metric].toLocaleString('ko-KR')}{:else}<span>Lv.</span> {entry[metric].toLocaleString('ko-KR')}{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if weaponEnchant}<p class="metric-summary">+7 이상 무기 보유 캐릭터 {leaderboard.entries.length.toLocaleString('ko-KR')}명</p>{/if}
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? `${label} 정보를 불러오고 있어요` : error ? '정보에 연결할 수 없어요' : weaponEnchant ? '+7 이상 무기를 보유한 캐릭터가 없어요' : '아직 순위에 표시할 캐릭터가 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 자동으로 갱신됩니다.' : weaponEnchant ? '+7 이상 무기를 보유하면 목록에 표시됩니다.' : metric === 'land_plots' ? '영지를 보유한 캐릭터가 생기면 순위가 표시됩니다.' : `캐릭터가 생성되면 ${label} 순위가 표시됩니다.`}</p>
    </div>
  {/if}
</section>

<style>
  .chart-heading { flex-wrap: nowrap; align-items: flex-start; gap: 12px; }
  .chart-heading > div { min-width: 0; }
  .leaderboard-tag { flex-shrink: 0; color: #167b6c; background: #eaf3ef; border-radius: 6px; padding: 6px 10px; font-size: 10px; font-weight: 600; letter-spacing: 1px; }
  table { width: 100%; border-collapse: collapse; table-layout: fixed; margin-top: 16px; font-size: 13px; }
  .scrollable { max-height: 520px; overflow: auto; margin-top: 16px; }
  .scrollable table { margin-top: 0; }
  .scrollable thead th { position: sticky; top: 0; }
  th, td { padding: 9px 10px; border-bottom: 1px solid #edf1ee; text-align: left; }
  thead th { background: #f6f8f7; color: #74857d; font-size: 11px; font-weight: 500; }
  tbody tr:last-child > * { border-bottom: 0; }
  .rank { width: 48px; text-align: center; font-variant-numeric: tabular-nums; }
  .rank-badge { display: inline-grid; place-items: center; width: 28px; height: 28px; color: #899b90; font-weight: 600; border-radius: 8px; }
  .podium { background: #eaf3ef; color: #166d5e; }
  .character-name { font-weight: 500; overflow-wrap: anywhere; }
  .character-button { display: inline-flex; align-items: center; gap: 7px; max-width: 100%; border: 0; padding: 3px 0; background: none; text-align: left; font-size: inherit; font-weight: inherit; overflow-wrap: anywhere; }
  .character-button i { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .selected { background: #f0f6f3; }
  .account-character { color: #74857d; font-size: 11px; font-weight: 400; }
  .value { width: 76px; text-align: right; font-variant-numeric: tabular-nums; }
  td.value { color: #166d5e; font-weight: 600; }
  .value span { color: #899b90; font-size: 10px; font-weight: 400; }
  .value.gold { width: 124px; }
  @media (max-width: 600px) {
    .chart-heading { flex-direction: row; align-items: center; gap: 12px; }
    th, td { padding: 8px 6px; }
    .rank { width: 44px; }
    .value { width: 72px; }
    .value.gold { width: 112px; }
    .leaderboard-tag { font-size: 9px; padding: 5px 8px; }
  }
</style>
