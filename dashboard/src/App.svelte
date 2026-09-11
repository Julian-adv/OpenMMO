<script lang="ts">
  import ConcurrentChart from './lib/ConcurrentChart.svelte'
  import ConnectionBreakdown from './lib/ConnectionBreakdown.svelte'
  import { formatDateTime, formatTime, parseHistory, periods, summarize, type ConcurrentHistory, type Hours } from './lib/metrics'

  let hours = $state<Hours>(24)
  let period = $derived(periods.find((period) => period.hours === hours)!)
  let history = $state<ConcurrentHistory | null>(null)
  let refreshing = $state(false)
  let error = $state('')
  let loading = $derived(history === null && !error)
  let refresh = () => {}
  let summary = $derived(summarize(history?.samples ?? []))
  const count = (value: number | null | undefined) => value == null ? '—' : value.toLocaleString('ko-KR')

  $effect(() => {
    const requestedHours = hours
    let stopped = false
    let controller: AbortController | null = null
    history = null
    error = ''

    async function update() {
      if (controller) return
      const request = new AbortController()
      controller = request
      refreshing = true
      const timeout = window.setTimeout(() => request.abort(), 10000)
      try {
        const response = await fetch(`/api/metrics/concurrent?hours=${requestedHours}`, { signal: request.signal, cache: 'no-store' })
        if (!response.ok) throw new Error(`HTTP ${response.status}`)
        const data = parseHistory(await response.json(), requestedHours)
        if (!stopped) {
          history = data
          error = ''
        }
      } catch {
        if (!stopped) error = '접속 현황을 불러오지 못했어요. 잠시 후 다시 시도해 주세요.'
      } finally {
        window.clearTimeout(timeout)
        controller = null
        if (!stopped) {
          refreshing = false
        }
      }
    }

    refresh = () => { void update() }
    void update()
    const timer = window.setInterval(() => { void update() }, 30000)
    return () => {
      stopped = true
      window.clearInterval(timer)
      controller?.abort()
    }
  })
</script>

<svelte:head>
  <title>접속 현황 · OpenMMO Pulse</title>
</svelte:head>

<header class="site-header">
  <div class="header-inner">
    <a class="brand" href={import.meta.env.BASE_URL} aria-label="OpenMMO Pulse 홈">
      <span class="brand-mark"><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M3 12h5l3-7 3 14 3-7h4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg></span>
      <span>OpenMMO <span class="brand-sub">Pulse</span></span>
    </a>
    <nav aria-label="대시보드"><a class="nav-current" href={import.meta.env.BASE_URL} aria-current="page">접속 현황</a></nav>
    <span class="header-caption">월드의 오늘을 기록합니다</span>
  </div>
</header>

<main>
  <div class="page-heading">
    <div>
      <div class="eyebrow"><span></span> WORLD ACTIVITY</div>
      <h1>월드 접속 현황<span>.</span></h1>
      <p class="page-description">지금 함께하는 플레이어와 시간에 따른 월드의 변화를 살펴보세요.</p>
    </div>
    <div class="update-controls">
      <div class:unavailable={!!error} class="update-status" role="status">
        <span class="status-dot"></span>
        {#if error}연결 확인 필요{:else if loading}연결 중{:else}30초마다 업데이트{/if}
      </div>
      <button class="refresh-button" onclick={() => refresh()} disabled={refreshing} aria-label="접속 현황 새로고침" title="새로고침">
        <svg viewBox="0 0 24 24" fill="none" class:spinning={refreshing} aria-hidden="true"><path d="M20 11a8 8 0 1 0-2 6M20 4v7h-7" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" /></svg>
      </button>
    </div>
  </div>

  {#if error}
    <div class="error-banner" role="alert">
      <span>{error} {history ? `마지막 수신: ${formatDateTime(history.until)} KST. 아래는 마지막으로 받은 기록입니다.` : ''}</span>
      <button onclick={() => refresh()} disabled={refreshing}>다시 시도</button>
    </div>
  {/if}

  <section class="stat-grid" aria-label="접속 요약" aria-busy={loading}>
    <article class="stat-card current-card">
      <div class="stat-label">{error && history ? '마지막 확인 접속' : '현재 접속'}<span class="live-tag">{error ? '갱신 중단' : loading ? '연결 중' : 'LIVE'}</span></div>
      <div class="stat-value">{count(history?.current.accounts)}<span>계정</span></div>
      <div class="stat-detail"><span class="tiny-dot"></span>{history ? `${formatTime(history.until)} KST 기준` : '월드에 입장한 계정 기준'}</div>
      {#if history}<ConnectionBreakdown sample={history.current} />{/if}
    </article>
    <article class="stat-card">
      <div class="stat-label">기간 최고 접속<span class="stat-icon" aria-hidden="true">↗</span></div>
      <div class="stat-value">{count(summary.peak)}<span>계정</span></div>
      <div class="stat-detail">{summary.peakAt !== null ? `${formatDateTime(summary.peakAt)} KST` : `최근 ${period.label} · 기록 대기 중`}</div>
    </article>
    <article class="stat-card">
      <div class="stat-label">기간 평균 접속<span class="stat-icon average-icon" aria-hidden="true">≈</span></div>
      <div class="stat-value">{summary.average === null ? '—' : summary.average.toLocaleString('ko-KR', { minimumFractionDigits: 1, maximumFractionDigits: 1 })}<span>계정</span></div>
      <div class="stat-detail">최근 {period.label} · 수집된 기록 기준</div>
    </article>
  </section>

  <section class="chart-panel" aria-labelledby="chart-title" aria-busy={loading}>
    <div class="chart-heading">
      <div>
        <h2 id="chart-title">동시 접속 추이</h2>
        <p>월드에 머물고 있는 계정 수의 변화</p>
      </div>
      <div class="period-filter" role="group" aria-label="조회 기간">
        {#each periods as option (option.hours)}
          <button class:active={hours === option.hours} aria-pressed={hours === option.hours} onclick={() => { hours = option.hours }}>{option.label}</button>
        {/each}
      </div>
    </div>
    <div class="chart-meta"><span>접속 계정 수</span><span>{period.intervalLabel} · 한국 시간 (KST)</span></div>
    {#if history && history.samples.length > 0}
      <ConcurrentChart {history} peak={summary.peak} />
    {:else}
      <div class="chart-empty" role="status">
        <div class="empty-illustration" aria-hidden="true"><svg viewBox="0 0 64 48" fill="none"><path d="M4 42h56M4 24h56M4 6h56" stroke="currentColor" stroke-opacity=".18" /><path d="M6 34h13l9-16 10 11 10-19 10 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" /></svg></div>
        <strong>{loading ? '월드의 기록을 불러오고 있어요' : error ? '기록에 연결할 수 없어요' : '첫 번째 기록을 기다리고 있어요'}</strong>
        <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 그래프가 자동으로 갱신됩니다.' : '이 기간에 수집된 기록이 아직 없습니다. 새 기록은 1분마다 쌓입니다.'}</p>
      </div>
    {/if}
    <div class="chart-footer">
      <span>{history ? `${formatDateTime(history.from)} — ${formatDateTime(history.until)}` : `최근 ${period.label}`} <span class="timezone">KST</span></span>
      <span>{history ? `${summary.sampleCount.toLocaleString('ko-KR')}개 기록` : '기록 확인 중'}</span>
    </div>
  </section>

  <section class="notes-grid" aria-label="지표 안내">
    <div class="metric-note">
      <span class="note-icon" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none"><circle cx="9" cy="8" r="3" stroke="currentColor" stroke-width="1.5" /><path d="M3 20v-2a6 6 0 0 1 12 0v2M16 5a3 3 0 0 1 0 6m2 3a5 5 0 0 1 3 4v2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg></span>
      <div><h3>게임에 입장한 계정 기준</h3><p>게임에 입장한 계정을 한 번씩 세며, 공식 NPC는 제외합니다. 접속 프로그램에 따라 웹 접속과 외부 에이전트를 구분합니다. 구분 정보가 없는 과거 기록과 기타 클라이언트는 기타·미분류로 표시합니다.</p></div>
    </div>
    <div class="metric-note">
      <span class="note-icon" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="8" stroke="currentColor" stroke-width="1.5" /><path d="M12 7v5l3 2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg></span>
      <div><h3>기록된 순간을 연결합니다</h3><p>최고·평균은 1분 간격의 기록으로 계산합니다. 긴 기간의 그래프는 구간 평균으로 표시하며, 기록이 없는 구간은 평균에서 제외합니다. 1개월·6개월·1년은 최근 30일·180일·365일 기준입니다.</p></div>
    </div>
  </section>
  <footer class="site-footer"><span>OpenMMO <strong>Pulse</strong></span><span>작은 순간들이 모여, 하나의 월드가 됩니다.</span></footer>
</main>
