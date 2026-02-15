<script lang="ts">
  import Monster from '../Monster.svelte'
  import type { MonsterData } from '../../types/Monster'

  interface Props {
    monsters: Map<string, MonsterData>
    monsterModels?: (Monster | undefined)[]
  }

  let { monsters, monsterModels = $bindable<(Monster | undefined)[]>([]) }: Props =
    $props()
</script>

{#each [...monsters.values()] as monster, index (monster.id)}
  <Monster
    bind:this={monsterModels[index]}
    id={monster.id}
    type={monster.type}
    position={monster.position}
    rotation={monster.rotation}
    monsterState={monster.state}
    lastDamageInfo={monster.lastDamageInfo}
  />
{/each}
