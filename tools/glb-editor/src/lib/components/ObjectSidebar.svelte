<script lang="ts">
  import type { CandidateSummary } from '../viewer'

  interface Props {
    candidates: CandidateSummary[]
    selectedIndex: number
    onSelect: (index: number) => void
  }

  let { candidates, selectedIndex, onSelect }: Props = $props()
</script>

<aside class="sidebar">
  <div class="small title">오브젝트 목록 (메시 포함 노드)</div>
  <div class="list">
    {#each candidates as item (item.index)}
      <button
        class="item"
        class:active={item.index === selectedIndex}
        onclick={() => onSelect(item.index)}
      >
        <div class="name">{item.name}</div>
        <div class="small">{item.stats}</div>
      </button>
    {/each}
  </div>
</aside>

<style>
  .small {
    color: #9ca3af;
    font-size: 12px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }

  .title {
    margin-bottom: 10px;
  }

  .sidebar {
    grid-column: 1;
    grid-row: 2;
    background: #1a1f2d;
    border-right: 1px solid #000;
    padding: 10px;
    overflow: auto;
  }

  .list {
    display: grid;
    gap: 8px;
  }

  .item {
    text-align: left;
    border: 1px solid #07090f;
    background: #111622;
    padding: 8px;
    border-radius: 8px;
    cursor: pointer;
    color: inherit;
  }

  .item.active {
    outline: 2px solid #67e8f9;
  }

  .name {
    font-weight: 700;
    margin-bottom: 4px;
    overflow-wrap: anywhere;
  }
</style>
