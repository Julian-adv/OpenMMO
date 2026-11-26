<script lang="ts">
  import { onMount } from 'svelte'
  import { t, type MessageKey } from '../i18n'
  import { networkManager } from '../network/socket'
  import { draggablePanel } from '../actions/draggablePanel'
  import { mountOverlay } from '../stores/overlayStack'
  import './hudDialog.css'
  import {
    closeSeasonalEvents,
    markSeasonalEventsPending,
    seasonalEventsPanel,
    type SeasonalEvent,
  } from '../stores/seasonalEventStore'
  import {
    buildSchedule,
    clampDay,
    daysInMonth,
    formatMonthDay,
    sameSchedule,
    scheduleMode,
    scheduledSeason,
    seasonContains,
    type Season,
    type SeasonMode,
    type SeasonSchedule,
  } from '../utils/seasonDates'

  interface Draft {
    mode: SeasonMode
    period: Season
  }
  type Edge = 'start' | 'end'

  const MONTHS = Array.from({ length: 12 }, (_, i) => i + 1)
  const MODES: SeasonMode[] = ['default', 'period', 'off']

  const panel = $derived($seasonalEventsPanel)
  // Kept apart from `panel` so a pending flag does not reset the drafts.
  const events = $derived(panel?.events ?? [])
  let drafts = $state<Record<string, Draft>>({})

  $effect(() => mountOverlay('seasonalEvents', closeSeasonalEvents))
  onMount(() => networkManager.sendQuerySeasonalEvents())

  $effect(() => {
    const next: Record<string, Draft> = {}
    for (const event of events) {
      const season =
        scheduledSeason(event.schedule, event.default_season) ??
        event.default_season
      next[event.id] = {
        mode: scheduleMode(event.schedule),
        period: { start: { ...season.start }, end: { ...season.end } },
      }
    }
    drafts = next
  })

  function eventName(id: string): string {
    const key = `season.event.${id}`
    const name = $t(key as MessageKey)
    return name === key ? id : name
  }

  function drafted(event: SeasonalEvent): SeasonSchedule | null {
    const draft = drafts[event.id]
    return draft ? buildSchedule(draft.mode, draft.period) : null
  }

  function changed(event: SeasonalEvent): boolean {
    const schedule = drafted(event)
    return schedule !== null && !sameSchedule(schedule, event.schedule)
  }

  /** Whether the drafted schedule would have the props up today. */
  function upOnceApplied(event: SeasonalEvent): boolean {
    const schedule = drafted(event)
    if (!schedule || !panel?.today) return event.active
    const season = scheduledSeason(schedule, event.default_season)
    return season !== null && seasonContains(season, panel.today)
  }

  function setMonth(draft: Draft, edge: Edge, month: number) {
    draft.period[edge] = clampDay({ month, day: draft.period[edge].day })
  }

  function apply(event: SeasonalEvent) {
    const schedule = drafted(event)
    if (!schedule) return
    markSeasonalEventsPending()
    networkManager.sendSetSeasonalEvent(event.id, $state.snapshot(schedule))
  }
</script>

{#snippet dayPicker(draft: Draft, edge: Edge)}
  <span class="day-picker">
    <span class="hint">{$t(`season.${edge}`)}</span>
    <select
      aria-label={`${$t(`season.${edge}`)} ${$t('season.month')}`}
      value={draft.period[edge].month}
      onchange={(e) => setMonth(draft, edge, Number(e.currentTarget.value))}
    >
      {#each MONTHS as month (month)}
        <option value={month}>{month}</option>
      {/each}
    </select>
    <span class="hint">/</span>
    <select
      aria-label={`${$t(`season.${edge}`)} ${$t('season.day')}`}
      bind:value={draft.period[edge].day}
    >
      {#each { length: daysInMonth(draft.period[edge].month) }, i (i)}
        <option value={i + 1}>{i + 1}</option>
      {/each}
    </select>
  </span>
{/snippet}

<div
  class="hud-dialog season-panel"
  role="dialog"
  aria-labelledby="season-title"
  use:draggablePanel={'seasonalEvents'}
>
  <div class="panel-header" data-drag-handle>
    <h2 class="panel-title" id="season-title">{$t('season.title')}</h2>
    {#if panel?.today}
      <span class="hint"
        >{$t('season.today', { date: formatMonthDay(panel.today) })}</span
      >
    {/if}
    <button
      type="button"
      class="close-btn"
      onclick={closeSeasonalEvents}
      title={$t('common.close')}
      aria-label={$t('common.close')}>&times;</button
    >
  </div>
  <p class="hint">{$t('season.hint')}</p>
  {#if !panel?.today}
    <p class="hint">{$t('season.loading')}</p>
  {:else if events.length === 0}
    <p class="hint">{$t('season.empty')}</p>
  {:else}
    <ul>
      {#each events as event (event.id)}
        {@const draft = drafts[event.id]}
        <li>
          <div class="event-head">
            <span class="name">{eventName(event.id)}</span>
            <span class="hint">{event.id}</span>
            <span class="badge" class:up={event.active}
              >{$t(event.active ? 'season.up' : 'season.down')}</span
            >
          </div>
          <div class="hint">
            {$t('season.defaultRun', {
              start: formatMonthDay(event.default_season.start),
              end: formatMonthDay(event.default_season.end),
            })} · {$t('season.props', { count: event.props })}
          </div>
          {#if draft}
            <div class="modes" role="radiogroup" aria-label={$t('season.run')}>
              {#each MODES as mode (mode)}
                <label>
                  <input
                    type="radio"
                    name={`season-mode-${event.id}`}
                    value={mode}
                    bind:group={draft.mode}
                  />
                  {$t(`season.mode.${mode}`)}
                </label>
              {/each}
            </div>
            {#if draft.mode === 'period'}
              <div class="period">
                {@render dayPicker(draft, 'start')}
                <span class="hint">~</span>
                {@render dayPicker(draft, 'end')}
              </div>
            {/if}
            <div class="apply-row">
              {#if changed(event)}
                <span class="preview"
                  >{$t(
                    upOnceApplied(event)
                      ? 'season.preview.up'
                      : 'season.preview.down'
                  )}</span
                >
              {/if}
              <button
                type="button"
                class="btn primary"
                disabled={panel.pending || !changed(event)}
                onclick={() => apply(event)}>{$t('season.apply')}</button
              >
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if panel?.error}
    <p class="error" role="alert">{panel.error}</p>
  {/if}
</div>

<style>
  .season-panel {
    position: fixed;
    right: 24px;
    top: 96px;
    z-index: 40;
    width: 340px;
    max-height: 70vh;
  }
  .panel-title {
    flex: 1;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-y: auto;
  }
  li {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }
  li:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }
  .event-head,
  .modes,
  .period,
  .day-picker,
  .apply-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .name {
    font-size: 13px;
    font-weight: 700;
  }
  .badge {
    margin-left: auto;
    padding: 1px 8px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 999px;
    color: #9fb2c3;
    font-size: 11px;
    font-weight: 700;
    white-space: nowrap;
  }
  .badge.up {
    border-color: rgba(126, 211, 138, 0.6);
    color: #7ed38a;
  }
  .modes {
    gap: 12px;
  }
  .modes label {
    display: flex;
    align-items: center;
    gap: 4px;
    cursor: pointer;
  }
  .modes input {
    margin: 0;
    accent-color: #f0c040;
  }
  .period {
    flex-wrap: wrap;
  }
  select {
    padding: 2px 4px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.35);
    color: inherit;
    font: inherit;
  }
  option {
    background: #0b1016;
  }
  .apply-row {
    justify-content: flex-end;
    min-height: 22px;
  }
  .preview {
    margin-right: auto;
    color: #c8b88a;
  }
</style>
