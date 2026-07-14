<script lang="ts">
  import Monster from '../Monster.svelte'
  import { monsterManager } from '../../managers/monsterManager'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import { OFFSCREEN_Y } from '../../utils/house-geo-utils'
  import type { MonsterData } from '../../types/Monster'

  interface Props {
    monsters: Map<string, MonsterData>
    monsterModels?: (Monster | undefined)[]
  }

  let {
    monsters,
    monsterModels = $bindable<(Monster | undefined)[]>([]),
  }: Props = $props()

  // Floor filter: underground shows only same-depth monsters; on the
  // surface dungeon monsters are hidden. Mismatches are parked at
  // OFFSCREEN_Y instead of unmounted (no pipeline churn, indices stable).
  let viewerFloor = $derived(
    $currentDungeonDepth >= 1 ? -$currentDungeonDepth : 0
  )
  const HIDDEN_POS = { x: 0, y: OFFSCREEN_Y, z: 0 }

  function isOnViewerFloor(monster: MonsterData): boolean {
    const fl = monster.floorLevel ?? 0
    return viewerFloor < 0 ? fl === viewerFloor : fl >= 0
  }
</script>

{#each [...monsters.values()] as monster, index (monster.id)}
  <Monster
    bind:this={monsterModels[index]}
    id={monster.id}
    type={monster.type}
    position={isOnViewerFloor(monster) ? monster.position : HIDDEN_POS}
    rotation={monster.rotation}
    monsterState={monster.state}
    attackCounter={monster.attackCounter}
    lastDamageInfo={monster.lastDamageInfo}
    droppedWeaponItemDefId={monster.droppedWeaponItemDefId}
    onHitFinished={() => monsterManager.handleMonsterHitFinished(monster.id)}
  />
{/each}
