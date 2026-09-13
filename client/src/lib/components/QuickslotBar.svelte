<script lang="ts">
  import {
    DOUBLE_SLASH,
    abilityRequirementsNotMet,
    getAbility,
    abilityEquipmentAllowed,
  } from '../data/abilities'
  import { DAGGER_SKILL } from '../data/daggerSkill'
  import { combatController } from '../managers/combatController'
  import { monsterManager } from '../managers/monsterManager'
  import {
    daggerSkillState,
    daggerSkillClock,
    queueDaggerSkill,
  } from '../stores/daggerSkillStore'
  import {
    gameStore,
    addChatMessage,
    hoveredMonsterId,
  } from '../stores/gameStore'
  import {
    abilityCooldowns,
    abilityPending,
    abilityClock,
    beginAbility,
    activeBuffs,
  } from '../stores/abilityStore'
  import { skillTooltip } from '../actions/skillTooltip'
  import { inventoryStore } from '../stores/inventoryStore'
  import { getItemDef } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import {
    quickslots,
    QUICKSLOT_COUNT,
    loadQuickslots,
    clearQuickslot,
    resolveQuickslot,
    quickslotAction,
  } from '../stores/quickslotStore'
  import { dragMeta, dragPos, quickslotAt } from '../stores/dragStore'
  import { itemTooltip } from '../actions/itemTooltip'
  import { instrumentPanelVisible } from '../stores/instrumentStore'

  interface Props {
    /** Active character id — used to load that character's saved quickslots. */
    characterId: number | null
  }

  let { characterId }: Props = $props()

  const daggerEquipped = $derived(
    getItemDef($inventoryStore.equipped.main_hand?.item_def_id ?? '')
      ?.weaponType === DAGGER_SKILL.weaponType
  )

  $effect(() => {
    if (characterId != null) loadQuickslots(characterId)
  })

  const slots = $derived.by(() => {
    const { bag, equipped } = $inventoryStore
    return $quickslots.map((entry) => {
      if (!entry) return null
      if ('skill' in entry) {
        const ability = getAbility(entry.skill)
        return ability ? { kind: 'ability' as const, skill: ability } : null
      }
      const def = getItemDef(entry.defId)
      if (!def) return null
      return {
        kind: 'item' as const,
        def,
        ...resolveQuickslot(entry, def, equipped, bag),
      }
    })
  })

  // Match the drop handler's target.
  const dropIndex = $derived(
    $dragMeta && ('skill' in $dragMeta || $dragMeta.groupItems === undefined)
      ? quickslotAt($dragPos.x, $dragPos.y)
      : -1
  )

  function useSlot(index: number) {
    const entry = slots[index]
    if (!entry) return
    if (entry.kind === 'ability') {
      if (
        !$gameStore.currentPlayer ||
        $gameStore.currentPlayer.health <= 0 ||
        $gameStore.currentPlayer.mounted
      )
        return
      if (entry.skill.id === DOUBLE_SLASH.id) {
        if (!daggerEquipped) {
          addChatMessage({
            text: abilityRequirementsNotMet(entry.skill.name),
            sender: 'system',
          })
          return
        }
        queueDaggerSkill()
        return
      }
      if (!abilityEquipmentAllowed(entry.skill.id, $inventoryStore.equipped)) {
        addChatMessage({
          text: abilityRequirementsNotMet(entry.skill.name),
          sender: 'system',
        })
        return
      }
      const needsTarget =
        'target' in entry.skill && entry.skill.target === 'monster'
      const target = needsTarget
        ? combatController.getAbilityTarget($hoveredMonsterId, (id) =>
            monsterManager.monsters.get(id)
          )
        : null
      if (needsTarget && !target) {
        addChatMessage({
          text: `Select or hover over a target for ${entry.skill.name}.`,
          sender: 'system',
        })
        return
      }
      if (beginAbility(entry.skill.id))
        networkManager.sendUseAbility(entry.skill.id, target)
      return
    }
    const action = quickslotAction(entry.def, entry)
    if (!action) return
    if (action.kind === 'unequip') networkManager.sendUnequipItem(action.slot)
    else if (action.kind === 'equip')
      networkManager.sendEquipItem(action.instanceId)
    else networkManager.sendUseItem(action.instanceId)
  }

  // Digit1..Digit9 → slots 0..8, Digit0 → slot 9.
  function handleKeydown(event: KeyboardEvent) {
    if (event.repeat || $instrumentPanelVisible) return
    if (event.ctrlKey || event.altKey || event.metaKey) return
    const tag = (document.activeElement?.tagName ?? '').toLowerCase()
    if (
      tag === 'input' ||
      tag === 'textarea' ||
      tag === 'select' ||
      (document.activeElement instanceof HTMLElement &&
        document.activeElement.isContentEditable)
    )
      return
    const match = /^Digit(\d)$/.exec(event.code)
    if (!match) return
    const digit = Number(match[1])
    const index = digit === 0 ? 9 : digit - 1
    if (index >= QUICKSLOT_COUNT) return
    event.preventDefault()
    useSlot(index)
  }

  // The 1-based key label shown on each slot (last slot is "0").
  function keyLabel(index: number): string {
    return index === 9 ? '0' : String(index + 1)
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="quickslot-bar" role="toolbar" aria-label="Quickslots">
  {#each slots as entry, i (i)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="quickslot"
      class:empty={!entry}
      class:drop-target={i === dropIndex}
      class:skill-queued={entry?.kind === 'ability' &&
        entry.skill.id === DOUBLE_SLASH.id &&
        $daggerSkillState.queued}
      class:skill-active={entry?.kind === 'ability' &&
        entry.skill.id !== DOUBLE_SLASH.id &&
        ($activeBuffs[entry.skill.id] ?? 0) > $abilityClock}
      data-quickslot={i}
      use:skillTooltip={entry && entry.kind === 'ability' ? entry.skill : null}
      use:itemTooltip={entry && !(entry.kind === 'ability')
        ? { def: entry.def, enchant: entry.enchant ?? undefined, side: 'right' }
        : null}
      onclick={() => useSlot(i)}
      oncontextmenu={(e) => {
        e.preventDefault()
        clearQuickslot(i)
      }}
    >
      <span class="key-label">{keyLabel(i)}</span>
      {#if entry && entry.kind === 'ability'}
        {@const remaining = Math.max(
          0,
          entry.skill.id === DOUBLE_SLASH.id
            ? $daggerSkillState.cooldownUntil - $daggerSkillClock
            : ($abilityCooldowns[entry.skill.id] ?? 0) - $abilityClock
        )}
        {@const pending =
          entry.skill.id === DOUBLE_SLASH.id
            ? $daggerSkillState.pending
            : ($abilityPending[entry.skill.id] ?? 0) > $abilityClock}
        <img
          class="item-icon skill-icon"
          class:depleted={!(entry.skill.id === DOUBLE_SLASH.id
            ? daggerEquipped
            : abilityEquipmentAllowed(
                entry.skill.id,
                $inventoryStore.equipped
              )) || remaining > 0}
          src={entry.skill.icon}
          alt={entry.skill.name}
          draggable="false"
        />
        {#if remaining > 0}<span class="skill-cooldown"
            >{remaining < 1000
              ? (Math.ceil(remaining / 100) / 10).toFixed(1)
              : Math.ceil(remaining / 1000)}</span
          >
        {:else if pending}<span class="skill-cooldown">…</span>{/if}
      {:else if entry}
        <img
          class="item-icon"
          class:depleted={entry.qty === 0}
          src="/items/{entry.def.icon}"
          alt=""
          draggable="false"
        />
        {#if entry.enchant !== null && entry.enchant > 0}
          <span class="item-enchant">+{entry.enchant}</span>
        {/if}
        {#if entry.qty !== 1}
          <span class="item-qty" class:zero={entry.qty === 0}>{entry.qty}</span>
        {/if}
      {/if}
    </div>
  {/each}
</div>

<style>
  .quickslot.skill-queued,
  .quickslot.skill-active {
    border-color: #a3f0d2;
    box-shadow: 0 0 8px #89d8b960;
  }
  .skill-cooldown {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: #0006;
    color: white;
    font-size: 18px;
    font-weight: bold;
    pointer-events: none;
  }
  .item-icon.skill-icon {
    width: 100%;
    height: 100%;
    border-radius: 3px;
    image-rendering: auto;
  }
  .quickslot-bar {
    /* Wide-screen single-row slot size (~70% of the original 56px). The
       wrap/phone media queries below shrink it for narrow viewports. */
    --quickslot-size: 40px;
    --quickslot-gap: 4px;
    display: flex;
    flex-direction: row;
    gap: var(--quickslot-gap);
    /* No padding or border: the bar's box is exactly the slots, so its bottom
       edge lines up with the chat panel and menu buttons. */
    border-radius: 10px;
    font-family: 'Courier New', monospace;
    pointer-events: auto;
    max-width: calc(100vw - 32px);
  }

  .quickslot {
    position: relative;
    box-sizing: border-box;
    width: var(--quickslot-size);
    height: var(--quickslot-size);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    background: rgba(6, 10, 14, 0.55);
    backdrop-filter: blur(4px);
  }

  /* Outline keeps the slot's box (and the bar's alignment) unchanged. */
  .quickslot.drop-target {
    border-color: rgba(88, 255, 88, 0.7);
    outline: 2px solid rgba(88, 255, 88, 0.7);
    outline-offset: 1px;
    box-shadow: 0 0 10px rgba(88, 255, 88, 0.35);
  }

  .key-label {
    z-index: 1;
    position: absolute;
    top: 2px;
    left: 4px;
    font-size: 11px;
    font-weight: 700;
    color: #9fb2c3;
    text-shadow: 0 0 3px rgba(0, 0, 0, 0.9);
    pointer-events: none;
  }

  .item-icon {
    /* Slightly inset and centred so edge-to-edge icons (sword, spear) stay
       inside the slot's border instead of spilling over it. */
    position: absolute;
    inset: 0;
    margin: auto;
    width: 90%;
    height: 90%;
    object-fit: contain;
    image-rendering: pixelated;
  }

  .item-icon.depleted {
    filter: grayscale(1) brightness(0.5);
  }

  .item-qty {
    position: absolute;
    bottom: 2px;
    right: 4px;
    font-size: 11px;
    font-weight: 700;
    color: #fff;
    text-shadow: 0 0 3px rgba(0, 0, 0, 0.8);
  }

  /* Top-right: the key label owns the top-left corner. */
  .item-enchant {
    right: 4px;
  }

  .item-qty.zero {
    color: #e06c6c;
  }

  /* Very narrow (<1000px): wrap the 10 slots into exactly two rows of five.
     The width is pinned to five slots wide and the action cluster is rigid
     (flex-shrink:0 in GameHud), so the bar can never be squeezed into a third
     or fourth row — the chat panel takes all the shrinking instead. */
  @media (max-width: 999.98px) {
    .quickslot-bar {
      flex-wrap: wrap;
      justify-content: center;
      --quickslot-size: 40px;
      /* Exactly five slots + four gaps per row (+1px guards against rounding
         bumping the fifth slot to a new row). */
      width: calc(5 * var(--quickslot-size) + 4 * var(--quickslot-gap) + 1px);
      max-width: calc(100vw - 18px);
    }
  }

  /* Phone / narrow: keep the two-row layout but let each slot shrink so the
     five-wide rows still fit (with the menu) without overflowing the screen. */
  @media (max-width: 600px), (pointer: coarse) {
    .quickslot-bar {
      --quickslot-size: min(40px, calc(20vw - 36px));
    }
  }
</style>
