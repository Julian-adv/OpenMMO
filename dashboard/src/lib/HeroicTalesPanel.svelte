<script lang="ts">
  import MetricsError from './MetricsError.svelte'
  import { formatCount, formatDateTime } from './metrics'
  import type { MetricsResource } from './metricsResource.svelte'
  import type { HeroicTales } from './heroicTales'

  let { resource }: { resource: MetricsResource<HeroicTales> } = $props()
  let { history: tales, loading, refreshing, error, refresh } = $derived(resource)
  let search = $state('')
  let query = $derived(search.trim().toLocaleLowerCase())
  let entries = $derived.by(() => {
    const allEntries = tales?.entries ?? []
    if (!query) return allEntries
    return allEntries.filter((entry) =>
      `${entry.date} ${entry.hero} ${entry.brief}`.toLocaleLowerCase().includes(query))
  })
</script>

<section class="chart-panel" aria-labelledby="heroic-tales-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="heroic-tales-title">시그네의 영웅담 원장</h2>
      <p>시그네가 노래하는 사건과 공연 방향 · 최근 기록순</p>
    </div>
    <input type="search" bind:value={search} aria-label="영웅담 원장 검색" placeholder="날짜·주인공·내용 검색" />
  </div>
  <MetricsError {error} until={tales?.until} {refreshing} {refresh} />
  {#if tales && tales.skipped_lines > 0}
    <p class="chart-notice">형식이 맞지 않아 표시하지 못한 기록이 {formatCount(tales.skipped_lines)}개 있습니다.</p>
  {/if}
  {#if entries.length > 0}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" tabindex="0" role="region" aria-label="영웅담 원장 표">
      <table aria-labelledby="heroic-tales-title">
        <thead><tr><th scope="col">기록 날짜</th><th scope="col">주인공</th><th scope="col">기록 내용</th></tr></thead>
        <tbody>
          {#each entries as entry (entry.line)}
            <tr>
              <td class="date">{entry.date}</td>
              <th scope="row" class="hero">{entry.hero}</th>
              <td class="brief">{entry.brief}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="chart-empty" role="status">
      {#if loading}
        <strong>영웅담 원장을 불러오고 있어요</strong>
        <p>잠시만 기다려 주세요.</p>
      {:else if error && !tales}
        <strong>원장에 연결할 수 없어요</strong>
        <p>연결이 복구되면 원장이 자동으로 갱신됩니다.</p>
      {:else if tales?.entries.length}
        <strong>검색 결과가 없어요</strong>
        <p>다른 날짜나 주인공, 내용으로 검색해 주세요.</p>
      {:else}
        <strong>{tales?.available ? '아직 표시할 영웅담 기록이 없어요' : '아직 영웅담 원장이 없어요'}</strong>
        <p>원장에 영웅담이 기록되면 이곳에 표시됩니다.</p>
      {/if}
    </div>
  {/if}
  <div class="chart-footer">
    <span>{tales ? `${formatDateTime(tales.until)} KST 확인` : '기록 확인 중'}</span>
    <span>{tales ? `${query ? `${formatCount(entries.length)}개 검색 결과 · ` : ''}전체 ${formatCount(tales.entries.length)}개 기록 · 1시간 갱신` : '전체 영웅담 기록'}</span>
  </div>
</section>

<style>
  input { width: 230px; max-width: 100%; padding: 8px 11px; border: 1px solid #dde5e0; border-radius: 8px; background: #f8faf9; color: #314b3f; font-size: 12px; }
  input::placeholder { color: #84938b; }
  .table-scroll { max-height: 520px; overflow: auto; margin: 18px 0; }
  table { width: 100%; min-width: 600px; border-collapse: collapse; font-size: 13px; }
  th, td { padding: 14px 12px; border-bottom: 1px solid #edf1ee; text-align: left; vertical-align: top; }
  thead th { position: sticky; top: 0; background: #f6f8f7; color: #74857d; font-size: 11px; font-weight: 500; white-space: nowrap; }
  tbody tr:last-child > * { border-bottom: 0; }
  .date { width: 110px; color: #74857d; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .hero { width: 130px; color: #166d5e; font-weight: 600; overflow-wrap: anywhere; }
  .brief { line-height: 1.8; white-space: pre-wrap; overflow-wrap: anywhere; }
  .chart-empty { min-height: 180px; }
</style>
