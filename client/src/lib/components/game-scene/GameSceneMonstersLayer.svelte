<script lang="ts">
  import { T, useTask, useThrelte } from '@threlte/core'
  import { onMount } from 'svelte'
  import { Vector3 } from 'three'
  import type { WebGPURenderer } from 'three/webgpu'
  import { BowMarkEffect, MARK_FADE } from '../../effects/bow-mark'
  import { bowMark } from '../../stores/abilityStore'
  import { teleportLoading } from '../../stores/debugStore'
  import Monster from '../Monster.svelte'
  import { monsterManager } from '../../managers/monsterManager'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import { OFFSCREEN_Y } from '../../utils/house-geo-utils'
  import { unwrapWorldXNear, WORLD_WIDTH_X } from '../../terrain/world-wrap'
  import type { LocalPlayer } from '../../stores/gameStore'
  import type { MonsterData } from '../../types/Monster'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'

  interface Props {
    monsters: Map<string, MonsterData>
    currentPlayer: LocalPlayer | null
    heightManager?: TerrainHeightManager | null
    monsterModels?: (Monster | undefined)[]
  }

  let {
    monsters,
    currentPlayer,
    heightManager = null,
    monsterModels = $bindable<(Monster | undefined)[]>([]),
  }: Props = $props()

  const { camera, renderer, scene } = useThrelte()
  const markEffect = new BowMarkEffect()
  const markAnchor = new Vector3()
  markEffect.setColor('#ed9984')
  let markPrepared = false

  onMount(() => {
    let disposed = false
    void (renderer as unknown as WebGPURenderer)
      .compileAsync(markEffect.group, $camera, scene)
      .catch((error: unknown) =>
        console.error('True Aim shader warmup failed', error)
      )
      .finally(() => {
        if (!disposed) markPrepared = true
      })
    return () => {
      disposed = true
      markEffect.dispose()
    }
  })

  useTask(() => {
    markEffect.group.visible = false
    const mark = $bowMark
    if (
      !markPrepared ||
      !mark ||
      $teleportLoading ||
      !currentPlayer ||
      currentPlayer.health <= 0
    )
      return
    if (Date.now() >= mark.until + MARK_FADE * 1000) return
    const target = monsters.get(mark.monsterId)
    if (!target || target.state === 'dead' || !isOnViewerFloor(target)) return
    const index = [...monsters.keys()].indexOf(mark.monsterId)
    if (!monsterModels[index]?.getMarkAnchor(markAnchor)) return
    markEffect.update(
      (Date.now() - mark.startedAt) / 1000,
      markAnchor,
      $camera,
      0.75,
      (mark.until - mark.startedAt) / 1000
    )
  })

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

  // Server state is canonical X; render the periodic copy nearest the viewer so
  // a monster across the seam isn't a world width away. Returns the stored
  // object unless the seam actually moves it — a fresh literal every frame
  // would push a pose into three.js for every idle monster and corpse.
  function displayPosition(monster: MonsterData) {
    if (!isOnViewerFloor(monster)) return HIDDEN_POS
    const viewerX = currentPlayer?.position.x
    if (
      viewerX === undefined ||
      Math.abs(monster.position.x - viewerX) <= WORLD_WIDTH_X / 2
    ) {
      return monster.position
    }
    return {
      ...monster.position,
      x: unwrapWorldXNear(viewerX, monster.position.x),
    }
  }
</script>

<T is={markEffect.group} />

{#each [...monsters.values()] as monster, index (monster.id)}
  <Monster
    bind:this={monsterModels[index]}
    id={monster.id}
    type={monster.type}
    floorLevel={monster.floorLevel ?? 0}
    {heightManager}
    position={displayPosition(monster)}
    rotation={monster.rotation}
    monsterState={monster.state}
    attackCounter={monster.attackCounter}
    hitCounter={monster.hitCounter}
    lastDamageInfo={monster.lastDamageInfo}
    droppedWeaponItemDefId={monster.droppedWeaponItemDefId}
    onHitFinished={() => monsterManager.handleMonsterHitFinished(monster.id)}
  />
{/each}
