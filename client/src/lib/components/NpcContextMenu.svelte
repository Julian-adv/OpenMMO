<script lang="ts">
  import { npcContextMenu, closeNpcContextMenu } from '../stores/npcMenuStore'

  const menu = $derived($npcContextMenu)

  function runEntry(action: () => void) {
    closeNpcContextMenu()
    action()
  }

  // Keep the menu on screen when the click lands near the right/bottom edge.
  const menuStyle = $derived.by(() => {
    if (!menu) return ''
    const x = Math.min(menu.screenX, window.innerWidth - 150)
    const y = Math.min(
      menu.screenY,
      window.innerHeight - 40 * (menu.entries.length + 1)
    )
    return `left: ${x}px; top: ${y}px;`
  })
</script>

{#if menu}
  <!-- contextmenu only suppresses the browser menu: the right-click that
       opened this menu fires contextmenu on the freshly mounted backdrop,
       so closing here would dismiss the menu within the same gesture. -->
  <div
    class="menu-backdrop"
    role="presentation"
    onpointerdown={closeNpcContextMenu}
    oncontextmenu={(e) => e.preventDefault()}
  ></div>
  <div
    class="npc-menu"
    style={menuStyle}
    role="menu"
    aria-label="{menu.npcName} actions"
  >
    <div class="menu-title">{menu.npcName}</div>
    {#each menu.entries as entry (entry.label)}
      <button
        class="menu-entry"
        role="menuitem"
        onclick={() => runEntry(entry.action)}
      >
        {entry.label}
      </button>
    {/each}
  </div>
{/if}

<style>
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 59;
  }

  .npc-menu {
    position: fixed;
    z-index: 60;
    min-width: 120px;
    display: flex;
    flex-direction: column;
    padding: 4px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 8px;
    background: rgba(6, 10, 14, 0.92);
    backdrop-filter: blur(4px);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 12px;
  }

  .menu-title {
    padding: 4px 8px 6px;
    font-weight: 700;
    color: #f0c040;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
    margin-bottom: 4px;
  }

  .menu-entry {
    padding: 6px 8px;
    border: none;
    border-radius: 4px;
    background: none;
    color: inherit;
    font-family: inherit;
    font-size: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 150ms ease;
  }

  .menu-entry:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  @media (pointer: coarse) {
    .menu-entry {
      min-height: 36px;
    }
  }
</style>
