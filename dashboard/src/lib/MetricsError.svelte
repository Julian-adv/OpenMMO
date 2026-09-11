<script lang="ts">
  import { formatDateTime } from './metrics'

  let { error, until, refreshing, refresh }: {
    error: string
    until?: number
    refreshing: boolean
    refresh: () => void
  } = $props()
</script>

{#if error}
  <div class="error-banner" role="alert">
    <span>{error} {until !== undefined ? `마지막 수신: ${formatDateTime(until)} KST. 아래는 마지막으로 받은 기록입니다.` : ''}</span>
    <button onclick={refresh} disabled={refreshing}>다시 시도</button>
  </div>
{/if}
