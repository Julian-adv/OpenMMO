<script lang="ts">
  import { onDestroy } from 'svelte'
  import Dashboard from './Dashboard.svelte'
  import GoogleLogin from './lib/GoogleLogin.svelte'
  import { provideDashboardAuth } from './lib/auth.svelte'

  const auth = provideDashboardAuth()
  onDestroy(() => auth.dispose())
</script>

<svelte:head>
  <title>운영자 대시보드 · OpenMMO Pulse</title>
</svelte:head>

{#if auth.signedIn}
  <Dashboard />
{:else}
  <GoogleLogin />
{/if}
