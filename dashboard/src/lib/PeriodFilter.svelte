<script lang="ts" generics="H extends number">
  let { hours = $bindable(), options, label }: {
    hours: H
    options: readonly { hours: H; label: string }[]
    label: string
  } = $props()
</script>

<div class="period-filter" role="group" aria-label={label}>
  {#each options as option (option.hours)}
    <button class:active={hours === option.hours} aria-pressed={hours === option.hours} onclick={() => { hours = option.hours }}>{option.label}</button>
  {/each}
</div>
