<script lang="ts">
  import type { MetricsResource } from './metricsResource.svelte'
  import GoldAmount from './GoldAmount.svelte'
  import GoldRankingTable from './GoldRankingTable.svelte'
  import MetricsError from './MetricsError.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { formatCount, formatDateTime, uniquePeriods, type GoldHours } from './metrics'
  import { goldSourceKey, type ItemGoldSources } from './itemGoldSources'

  let { hours = $bindable(), resource }: {
    hours: GoldHours
    resource: MetricsResource<ItemGoldSources>
  } = $props()
  let { history: sources, loading, refreshing, error, refresh } = $derived(resource)
  let period = $derived(uniquePeriods.find((option) => option.hours === hours)!)
</script>

<section class="chart-panel" aria-labelledby="item-gold-sources-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="item-gold-sources-title">골드 생산 순위</h2>
      <p>아이템 판매·상자·동전 획득·NPC 급여 · 생성 골드 내림차순</p>
    </div>
    <PeriodFilter bind:hours options={uniquePeriods} label="골드 생산 집계 기간" />
  </div>
  <MetricsError {error} until={sources?.until} {refreshing} {refresh} />
  <div class="metric-summary">
    <span>완료된 최근 {period.label} 생성된 골드{error ? ' · 갱신 중단' : ''}</span>
    <strong>{#if sources}<GoldAmount copper={sources.total_gold} />{:else}—{/if}</strong>
    <p>흥정 포함 판매 지급액, 던전 보상 상자, 몬스터·상자·파괴물의 동전 더미, 동전 주머니, 주민 NPC 급여를 합산합니다. 동전 더미는 주웠을 때 집계합니다.</p>
  </div>
  {#if sources && sources.collection_started_at > sources.from}
    <p class="chart-notice">아이템 판매는 {formatDateTime(sources.collection_started_at)} KST부터 수집한 기록만 포함합니다.</p>
  {/if}
  {#if sources && sources.rewards_started_at > sources.from}
    <p class="chart-notice">판매 외 골드는 {formatDateTime(sources.rewards_started_at)} KST부터 수집한 기록만 포함합니다.</p>
  {/if}
  {#if sources && sources.entries.length > 0}
    <GoldRankingTable entries={sources.entries} totalGold={sources.total_gold}
      titleId="item-gold-sources-title" label="골드 생산 순위 표" entryHeading="골드 생산원" goldHeading="생성 골드"
      entryKey={goldSourceKey} category={(entry) => entry.source === 'item_sale' ? '상인 판매' : ''}
      quantityUnit={(entry) => entry.source === 'item_sale' ? '개' : '회'} />
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '골드 생산 기록을 불러오고 있어요' : error ? '골드 생산 기록에 연결할 수 없어요' : '이 기간에 기록된 골드 생산이 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 표가 자동으로 갱신됩니다.' : '판매와 보상의 실제 지급액을 모아 매시간 순위에 반영합니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{sources ? `${formatDateTime(sources.from)} — ${formatDateTime(sources.until)}` : `최근 ${period.label}`} <span class="timezone">KST</span></span>
    <span>{sources ? `${formatCount(sources.entries.length)}개 생산원` : '기록 확인 중'}</span>
  </div>
</section>

<style>
  .chart-empty { min-height: 210px; }
</style>
