<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { SvelteMap } from 'svelte/reactivity'
  import PlayerModel from '../PlayerModel.svelte'
  import PlayerControl from '../PlayerControl.svelte'
  import type {
    ChatBubble,
    LocalPlayer,
    RemotePlayer,
  } from '../../stores/gameStore'
  import type { PlayerState } from '../../utils/movementUtils'
  import type Monster from '../Monster.svelte'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import { remotePlayerManager } from '../../managers/remotePlayerManager'

  import { applyTorchFlickerWorld, TORCH_BASE_INTENSITY, TORCH_BASE_DISTANCE, TORCH_BASE_DECAY, TORCH_BASE_POSITION } from '../../utils/torchFlicker'
  import { playerFloorLevel, playerInsideHouseId } from '../../stores/housingStore'
  import { housingManager } from '../../managers/housingManager'
  import { OFFSCREEN_Y } from '../../utils/house-geo-utils'

  // Pre-mounted pool of non-shadow torch lights for visible (but not closest)
  // torch-bearing remote players. The closest torch player is handled by the
  // shared `remoteShadowLight` below. Keeping this pool size constant means
  // the number of PointLights in the scene never changes when players join
  // or leave, so WebGPU does not recompile pipelines (which would stall the
  // main thread for several seconds per join).
  const REMOTE_TORCH_POOL_SIZE = 3
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
    doorMeshes: THREE.Object3D[]
    furnitureMeshes: THREE.Object3D[]
    groundItemMeshes: THREE.Object3D[]
    monsterModels: (Monster | undefined)[]
    playerAttackDuration: number
    heightManager: TerrainHeightManager
    onStateChange: (newState: PlayerState) => void
    onAttackDuration: (duration: number) => void
    onCurrentPlayerDyingFinished?: () => void
    isCurrentPlayerLoading?: boolean
    playerControl?: PlayerControl
    currentPlayerModel?: PlayerModel | null
    otherPlayerModels?: (PlayerModel | undefined)[]
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
    doorMeshes,
    furnitureMeshes,
    groundItemMeshes,
    monsterModels,
    playerAttackDuration,
    heightManager,
    onStateChange,
    onAttackDuration,
    onCurrentPlayerDyingFinished,
    isCurrentPlayerLoading = $bindable(false),
    playerControl = $bindable<PlayerControl>(),
    currentPlayerModel = $bindable<PlayerModel | null>(null),
    otherPlayerModels = $bindable<(PlayerModel | undefined)[]>([]),
  }: Props = $props()

  // Sync attack animation duration to remote player manager
  $effect(() => {
    remotePlayerManager.attackAnimationDuration = playerAttackDuration
  })

  let localFloorLevel = $derived(Math.max(0, $playerFloorLevel))
  let localHouseId = $derived($playerInsideHouseId)

  function isRemotePlayerVisible(remoteFloorLevel: number, pos: { x: number; y: number; z: number }): boolean {
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

  // Sort visible, torch-on remote players by squared distance from local
  // player. Slot 0 (closest) → shadow light; slots 1..N → pool lights.
  let remoteTorchTargets = $derived.by(() => {
    const targets: THREE.Vector3[] = []
    if (!currentPlayer) return targets

    const playerPos = currentPlayer.position
    const candidates: { rp: PlayerState; dist: number }[] = []
    for (const [id, player] of otherPlayers) {
      const rp = remotePlayers.get(id)
      if (!player.torchOn || !rp || !remoteVisibility.get(id)) continue
      const dx = rp.position.x - playerPos.x
      const dz = rp.position.z - playerPos.z
      candidates.push({ rp, dist: dx * dx + dz * dz })
    }
    candidates.sort((a, b) => a.dist - b.dist)

    for (const { rp } of candidates) {
      const y = heightManager.getHeightAtWorldPosition(rp.position.x, rp.position.z) ?? rp.position.y
      const base = new THREE.Vector3(rp.position.x, y, rp.position.z)
      const offset = TORCH_OFFSET.clone().applyAxisAngle(Y_AXIS, rp.rotation)
      targets.push(base.add(offset))
      if (targets.length >= 1 + REMOTE_TORCH_POOL_SIZE) break
    }
    return targets
  })

  // Closest torch target (or null) — the shared shadow-casting PointLight
  // tracks this position; intensity drops to 0 when no torch is lit.
  let remoteShadowLightPos = $derived(remoteTorchTargets[0] ?? null)

  let remoteShadowLight = $state<THREE.PointLight | undefined>(undefined)

  // Pre-mounted non-shadow torch light pool for the next N closest torch
  // players. These are always in the scene; their position + intensity are
  // updated each frame from the game loop.
  let remoteTorchPoolLights = $state<(THREE.PointLight | undefined)[]>(
    new Array(REMOTE_TORCH_POOL_SIZE).fill(undefined)
  )
  // Flicker phase per slot: index 0 = shadow light, 1..N = pool lights.
  const remoteTorchFlickerTimes = new Array(1 + REMOTE_TORCH_POOL_SIZE).fill(0)

  function flickerTorchSlot(
    light: THREE.PointLight | undefined,
    slot: number,
    target: THREE.Vector3 | undefined,
    deltaTime: number,
  ) {
    if (!light) return
    if (target) {
      remoteTorchFlickerTimes[slot] = applyTorchFlickerWorld(
        light, remoteTorchFlickerTimes[slot], deltaTime,
        target.x, target.y, target.z,
      )
    } else {
      light.intensity = 0
    }
  }

  export function updateRemoteTorchFlicker(deltaTime: number) {
    flickerTorchSlot(remoteShadowLight, 0, remoteTorchTargets[0], deltaTime)
    for (let i = 0; i < REMOTE_TORCH_POOL_SIZE; i++) {
      flickerTorchSlot(
        remoteTorchPoolLights[i],
        i + 1,
        remoteTorchTargets[i + 1],
        deltaTime,
      )
    }
  }
</script>

{#if camera && terrainMeshes.some((mesh) => mesh !== undefined)}
  <PlayerControl
    bind:this={playerControl}
    onStateChange={onStateChange}
    {camera}
    {heightManager}
    groundMeshes={[
      ...terrainMeshes.filter((mesh) => mesh !== undefined) as THREE.Mesh[],
      ...(housingGroup ? [housingGroup] : []),
    ]}
    monsterMeshes={monsterModels
      .map((model) => model?.getMeshGroup())
      .filter((group) => group !== undefined) as THREE.Group[]}
    {doorMeshes}
    {furnitureMeshes}
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
    bind:isLoading={isCurrentPlayerLoading}
    lastDamageInfo={currentPlayer.lastDamageInfo}
    lastRegenInfo={currentPlayer.lastRegenInfo}
  />
{/if}

{#if cameraInitialized && camera}
  {#each [...otherPlayers.values()] as player, index (player.id)}
    {@const remotePlayer = remotePlayers.get(player.id)}
    {#if remotePlayer}
      {@const visible = remoteVisibility.get(player.id) ?? false}
      {@const terrainY = heightManager.getHeightAtWorldPosition(remotePlayer.position.x, remotePlayer.position.z)}
      {@const baseY = (terrainY != null && remotePlayer.position.y > terrainY + 1.0) ? remotePlayer.position.y : (terrainY ?? remotePlayer.position.y)}
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
      />
    {/if}
  {/each}

  <!-- Standalone shadow-casting point light for closest remote torch player.
       castShadow is always true (never toggled). Intensity=0 when no target. -->
  <T.PointLight
    bind:ref={remoteShadowLight}
    position={remoteShadowLightPos
      ? [remoteShadowLightPos.x, remoteShadowLightPos.y, remoteShadowLightPos.z]
      : [0, 0, 0]}
    color="#ffcc66"
    intensity={remoteShadowLightPos ? TORCH_BASE_INTENSITY : 0}
    distance={TORCH_BASE_DISTANCE}
    decay={TORCH_BASE_DECAY}
    castShadow
    shadow.mapSize.width={512}
    shadow.mapSize.height={512}
    shadow.camera.near={0.5}
    shadow.camera.far={TORCH_BASE_DISTANCE}
    shadow.bias={-0.005}
    shadow.normalBias={0.05}
    shadow.radius={5}
  />

  <!-- Pre-mounted non-shadow torch light pool. See comment on
       REMOTE_TORCH_POOL_SIZE above for why these are static. -->
  {#each remoteTorchPoolLights as _light, i (i)}
    {@const target = remoteTorchTargets[i + 1]}
    <T.PointLight
      bind:ref={remoteTorchPoolLights[i]}
      position={target ? [target.x, target.y, target.z] : [0, 0, 0]}
      color="#ffcc66"
      intensity={target ? TORCH_BASE_INTENSITY : 0}
      distance={TORCH_BASE_DISTANCE}
      decay={TORCH_BASE_DECAY}
    />
  {/each}
{/if}
