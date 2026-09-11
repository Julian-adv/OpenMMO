<script lang="ts" generics="M extends LeaderboardMetric">
  import LeaderboardPanel from './LeaderboardPanel.svelte'
  import LeaderboardHistoryPanel from './LeaderboardHistoryPanel.svelte'
  import { createCharacterColors } from './leaderboardHistory'
  import type { CharacterLeaderboard, LeaderboardHours, LeaderboardMetric } from './metrics'
  import type { MetricsResource } from './metricsResource.svelte'

  let { metric, hours = $bindable(), resource }: {
    metric: M
    hours: LeaderboardHours
    resource: MetricsResource<CharacterLeaderboard<M>>
  } = $props()
  let selectedCharacter = $state<string | null>(null)
  const assignColors = createCharacterColors()
  let colors = $derived(resource.history ? assignColors(resource.history.entries.map((entry) => entry.name)) : {})
</script>

<div class="leaderboard-grid">
  <LeaderboardPanel {metric} leaderboard={resource.history} {colors} bind:selectedCharacter loading={resource.loading} refreshing={resource.refreshing} error={resource.error} refresh={() => resource.refresh()} />
  <LeaderboardHistoryPanel {metric} bind:hours leaderboard={resource.history} {colors} bind:selectedCharacter loading={resource.loading} refreshing={resource.refreshing} error={resource.error} refresh={() => resource.refresh()} />
</div>
