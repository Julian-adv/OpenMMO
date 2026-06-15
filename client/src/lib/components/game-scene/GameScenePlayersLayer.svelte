<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { get } from 'svelte/store'
  import { SvelteMap } from 'svelte/reactivity'
  import PlayerModel from '../PlayerModel.svelte'
  import PlayerControl from '../PlayerControl.svelte'
  import type { PlayerControlEvent } from '../player-control/events'
  import type {
    ChatBubble,
    LocalPlayer,
    RemotePlayer,
  } from '../../stores/gameStore'
  import type { PlayerState } from '../../utils/movementUtils'
  import type Monster from '../Monster.svelte'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import { remotePlayerManager } from '../../managers/remotePlayerManager'

  import { applyTorchFlickerWorld, TORCH_BASE_DISTANCE, TORCH_BASE_DECAY, TORCH_BASE_POSITION, TORCH_SHADOW_FAR, TORCH_SHADOW_MAP_SIZE, TORCH_SHADOW_BIAS } from '../../utils/torchFlicker'
  import { playerFloorLevel, playerInsideHouseId } from '../../stores/housingStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import { dungeonManager } from '../../managers/dungeonManager'
  import { housingManager } from '../../managers/housingManager'
  import { bridgeManager } from '../../managers/bridgeManager'
  import { OFFSCREEN_Y } from '../../utils/house-geo-utils'
  import { torchLightEnabled } from '../../stores/debugStore'
  import { localTorchEquipped } from '../../stores/inventoryStore'

  const TORCH_OFFSET = new THREE.Vector3(TORCH_BASE_POSITION.x, TORCH_BASE_POSITION.y, TORCH_BASE_POSITION.z)
  const Y_AXIS = new THREE.Vector3(0, 1, 0)

  interface Props {
    camera: THREE.OrthographicCamera | undefined
    cameraInitialized: boolean
    currentPlayer: LocalPlayer | null
    otherPlayers: Map<string, RemotePlayer>
    remotePlayers: Map<string, PlayerState>
    chatBubbles: Map<string, ChatBubble>
    currentPlayerState: PlayerState
    terrainMeshes: (THREE.Mesh | undefined)[]
    housingGroup: THREE.Group | null
    dungeonGroup: THREE.Group | null
    doorMeshes: THREE.Object3D[]
    objectMeshes: THREE.Object3D[]
    groundItemMeshes: THREE.Object3D[]
    monsterModels: (Monster | undefined)[]
    playerAttackDuration: number
    heightManager: TerrainHeightManager
    onStateChange: (newState: PlayerState) => void
    onPlayerControlEvent?: (event: PlayerControlEvent) => void
    onAttackDuration: (duration: number) => void
    onCurrentPlayerDyingFinished?: () => void
    isCurrentPlayerLoading?: boolean
    torchEffectsDisabled?: boolean
    playerControl?: PlayerControl
    currentPlayerModel?: PlayerModel | null
    otherPlayerModels?: (PlayerModel | undefined)[]
    torchLightCastsShadow?: boolean
  }

  let {
    camera,
    cameraInitialized,
    currentPlayer,
    otherPlayers,
    remotePlayers,
    chatBubbles,
    currentPlayerState,
    terrainMeshes,
    housingGroup,
    dungeonGroup,
    doorMeshes,
    objectMeshes,
    groundItemMeshes,
    monsterModels,
    playerAttackDuration,
    heightManager,
    onStateChange,
    onPlayerControlEvent,
    onAttackDuration,
    onCurrentPlayerDyingFinished,
    isCurrentPlayerLoading = $bindable(false),
    torchEffectsDisabled = false,
    playerControl = $bindable<PlayerControl>(),
    currentPlayerModel = $bindable<PlayerModel | null>(null),
    otherPlayerModels = $bindable<(PlayerModel | undefined)[]>([]),
    torchLightCastsShadow = true,
  }: Props = $props()

  // Sync attack animation duration to remote player manager
  $effect(() => {
    remotePlayerManager.attackAnimationDuration = playerAttackDuration
  })

  let localFloorLevel = $derived(Math.max(0, $playerFloorLevel))
  let localHouseId = $derived($playerInsideHouseId)
  let localDungeonDepth = $derived($currentDungeonDepth)

  function isRemotePlayerVisible(remoteFloorLevel: number, pos: { x: number; y: number; z: number }): boolean {
    // Dungeon: only players on the same depth are visible; from the
    // surface, underground players are hidden (and vice versa).
    if (localDungeonDepth >= 1) {
      return remoteFloorLevel === -localDungeonDepth
    }
    if (remoteFloorLevel < 0) return false
    const remoteHouse = housingManager.findHouseAtPoint(pos.x, pos.y, pos.z)
    if (localHouseId) {
      return remoteFloorLevel === localFloorLevel && remoteHouse?.id === localHouseId
    }
    return remoteHouse == null
  }

  let remoteVisibility = $derived.by(() => {
    const map = new SvelteMap<string, boolean>()
    for (const [id, player] of otherPlayers) {
      const rp = remotePlayers.get(id)
      map.set(id, rp ? isRemotePlayerVisible(player.floorLevel, rp.position) : false)
    }
    return map
  })

  // Unified torch: exactly one PointLight for the entire scene.
  // Priority: local player's torch (if ON) > closest visible remote player
  // with torchOn. When no candidate, intensity drops to 0. Keeping the
  // PointLight count at a constant 1 avoids WebGPU pipeline recompile stalls.
  //
  // Position/intensity are driven imperatively from the game loop (not a
  // $derived) because currentPlayer.position is a mutated plain object that
  // Svelte reactivity cannot track. The game loop runs every frame anyway,
  // so recomputing the target here has no extra cost.
  let unifiedTorchLight = $state<THREE.PointLight | undefined>(undefined)
  let unifiedTorchFlickerTime = 0
  const _unifiedTorchTmp = new THREE.Vector3()
  const _torchOffsetTmp = new THREE.Vector3()

  function setTorchTargetFromPose(x: number, z: number, fallbackY: number, rotation: number): THREE.Vector3 {
    const y =
      dungeonManager.sampleHeightAt(x, z) ??
      heightManager.getHeightAtWorldPosition(x, z) ??
      fallbackY
    _torchOffsetTmp.copy(TORCH_OFFSET).applyAxisAngle(Y_AXIS, rotation)
    return _unifiedTorchTmp.set(x + _torchOffsetTmp.x, y + _torchOffsetTmp.y, z + _torchOffsetTmp.z)
  }

  function computeUnifiedTorchTarget(): THREE.Vector3 | null {
    if (torchEffectsDisabled) return null
    if (!currentPlayer) return null
    if (get(localTorchEquipped) || get(torchLightEnabled)) {
      const p = currentPlayer.position
      return setTorchTargetFromPose(p.x, p.z, p.y, currentPlayer.rotation)
    }
    const playerPos = currentPlayer.position
    let bestRp: PlayerState | null = null
    let bestDist = Infinity
    for (const [id, player] of otherPlayers) {
      const rp = remotePlayers.get(id)
      if (!player.torchOn || !rp || !remoteVisibility.get(id)) continue
      const dx = rp.position.x - playerPos.x
      const dz = rp.position.z - playerPos.z
      const dist = dx * dx + dz * dz
      if (dist < bestDist) {
        bestDist = dist
        bestRp = rp
      }
    }
    if (!bestRp) return null
    return setTorchTargetFromPose(bestRp.position.x, bestRp.position.z, bestRp.position.y, bestRp.rotation)
  }

  export function updateUnifiedTorchFlicker(deltaTime: number) {
    if (!unifiedTorchLight) return
    const target = computeUnifiedTorchTarget()
    if (target) {
      unifiedTorchFlickerTime = applyTorchFlickerWorld(
        unifiedTorchLight, unifiedTorchFlickerTime, deltaTime,
        target.x, target.y, target.z,
      )
    } else {
      unifiedTorchLight.intensity = 0
    }
  }

  export function getUnifiedTorchLight(): THREE.PointLight | undefined {
    return unifiedTorchLight
  }
</script>

{#if camera && terrainMeshes.some((mesh) => mesh !== undefined)}
  <PlayerControl
    bind:this={playerControl}
    onStateChange={onStateChange}
    {camera}
    {heightManager}
    groundMeshes={localDungeonDepth >= 1 && dungeonGroup
      ? [dungeonGroup]
      : [
          ...terrainMeshes.filter((mesh) => mesh !== undefined) as THREE.Mesh[],
          ...(housingGroup ? [housingGroup] : []),
        ]}
    monsterMeshes={monsterModels
      .map((model) => model?.getMeshGroup())
      .filter((group) => group !== undefined) as THREE.Group[]}
    npcMeshes={(otherPlayerModels ?? [])
      .map((model) => model?.getModelGroup())
      .filter(
        (group): group is THREE.Group =>
          group !== undefined && typeof group.userData.npcPlayerId === 'string'
      )}
    {doorMeshes}
    {objectMeshes}
    {groundItemMeshes}
    attackCooldown={playerAttackDuration}
  />
{/if}

{#if currentPlayer && cameraInitialized && camera}
  <PlayerModel
    bind:this={currentPlayerModel}
    position={currentPlayer.position}
    name={currentPlayer.name}
    isCurrentPlayer={true}
    playerState={currentPlayerState.state}
    interactionAnim={currentPlayerState.interactionAnim}
    interactOffsetY={currentPlayerState.interactOffsetY}
    attackCounter={currentPlayerState.attackCounter}
    speed={currentPlayerState.speed}
    rotation={currentPlayerState.rotation}
    movementMode={currentPlayerState.movementMode}
    {camera}
    chatBubble={chatBubbles.get(currentPlayer.id)?.message}
    characterClass={currentPlayer.characterClass}
    gender={currentPlayer.gender}
    health={currentPlayer.health}
    maxHealth={currentPlayer.maxHealth}
    onAttackDuration={onAttackDuration}
    onDyingFinished={onCurrentPlayerDyingFinished}
    onInteractionFinished={() => {
      onPlayerControlEvent?.({ type: 'anim_interaction_finished' })
    }}
    onPickupGrab={() => {
      onPlayerControlEvent?.({ type: 'anim_pickup_grab' })
    }}
    bind:isLoading={isCurrentPlayerLoading}
    lastDamageInfo={currentPlayer.lastDamageInfo}
    lastRegenInfo={currentPlayer.lastRegenInfo}
    {torchEffectsDisabled}
  />
{/if}

{#if cameraInitialized && camera}
  {#each [...otherPlayers.values()] as player, index (player.id)}
    {@const remotePlayer = remotePlayers.get(player.id)}
    {#if remotePlayer}
      {@const visible = remoteVisibility.get(player.id) ?? false}
      {@const baseY = player.floorLevel > 0 || player.floorLevel < 0
        ? remotePlayer.position.y
        : (bridgeManager.findDeckYAt(remotePlayer.position.x, remotePlayer.position.z, null)
            ?? heightManager.getHeightAtWorldPosition(remotePlayer.position.x, remotePlayer.position.z)
            ?? remotePlayer.position.y)}
      <PlayerModel
        bind:this={otherPlayerModels[index]}
        position={new THREE.Vector3(
          remotePlayer.position.x,
          visible ? baseY : OFFSCREEN_Y,
          remotePlayer.position.z
        )}
        name={player.name}
        isCurrentPlayer={false}
        playerState={remotePlayer.state}
        interactionAnim={remotePlayer.interactionAnim}
        interactOffsetY={remotePlayer.interactOffsetY}
        attackCounter={remotePlayer.attackCounter}
        speed={remotePlayer.speed}
        rotation={remotePlayer.rotation}
        movementMode={remotePlayer.movementMode}
        {camera}
        chatBubble={chatBubbles.get(player.id)?.message}
        characterClass={player.characterClass}
        gender={player.gender}
        health={player.health}
        maxHealth={player.maxHealth}
        torchOn={player.torchOn}
        {torchEffectsDisabled}
        npcPlayerId={player.isNpc ? player.id : undefined}
      />
    {/if}
  {/each}

  <!-- Unified point light. Mounted exactly once, priority:
       local torch > closest visible remote torch. Shadow mode is fixed by the
       effective graphics preset (mobile keeps the light but skips shadow maps).
       Position/intensity are driven from the game loop. -->
  {#if !torchEffectsDisabled}
    <T.PointLight
      bind:ref={unifiedTorchLight}
      position={[0, 0, 0]}
      color="#ffcc66"
      intensity={0}
      distance={TORCH_BASE_DISTANCE}
      decay={TORCH_BASE_DECAY}
      castShadow={torchLightCastsShadow}
      shadow.mapSize.width={TORCH_SHADOW_MAP_SIZE}
      shadow.mapSize.height={TORCH_SHADOW_MAP_SIZE}
      shadow.camera.near={1.5}
      shadow.camera.far={TORCH_SHADOW_FAR}
      shadow.bias={TORCH_SHADOW_BIAS}
      shadow.normalBias={0.005}
      shadow.radius={2}
    />
  {/if}
{/if}
