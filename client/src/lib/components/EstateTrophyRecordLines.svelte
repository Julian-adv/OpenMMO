<script lang="ts">
  import { t } from '../i18n'
  import { formatCalendarDate } from '../data/gameCalendar'
  import type { EstateTrophyRecord } from '../network/networkTypes'

  let { record }: { record: EstateTrophyRecord } = $props()
</script>

<div class="record">
  <span class="date"
    >{$t('estateTrophy.defended', {
      date: formatCalendarDate($t, record.defended_on),
    })}</span
  >
  <span>{$t('estateTrophy.summoner', { name: record.summoner })}</span>
  {#if record.defenders.length}
    <span
      >{$t('estateTrophy.defenders', {
        names: record.defenders.join(', '),
      })}</span
    >
  {/if}
</div>

<style>
  .record {
    display: flex;
    flex-direction: column;
    gap: 2px;
    color: #d8c99a;
  }
  .date {
    font-weight: 600;
  }
</style>
