<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { SvelteMap } from 'svelte/reactivity'
  import PlayerModel, { type TorchMode } from '../PlayerModel.svelte'
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

  // Max remote players that get torch point lights (no shadows — WebGPU PointShadowNode
  // crashes when castShadow is toggled dynamically, so remote torches are light-only).
  const MAX_REMOTE_TORCH_LIGHTS = 4
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

  // Compute torch mode for each remote player:
  // - Closest N torch-bearing players get 'light-only'
  // - Rest get 'off'
  let remoteTorchModes = $derived.by(() => {
    const modes = new SvelteMap<string, TorchMode>()
    if (!currentPlayer) return modes

    const playerPos = currentPlayer.position
    const torchPlayers: { id: string; dist: number }[] = []

    for (const [id, player] of otherPlayers) {
      const rp = remotePlayers.get(id)
      if (!player.torchOn || !rp || !remoteVisibility.get(id)) {
        modes.set(id, 'off')
        continue
      }
      const dx = rp.position.x - playerPos.x
      const dz = rp.position.z - playerPos.z
      torchPlayers.push({ id, dist: dx * dx + dz * dz })
    }

    torchPlayers.sort((a, b) => a.dist - b.dist)

    for (let i = 0; i < torchPlayers.length; i++) {
      if (i === 0) {
        // Closest torch player: standalone shadow light handles lighting + shadows
        modes.set(torchPlayers[i].id, 'shadow')
      } else if (i < MAX_REMOTE_TORCH_LIGHTS) {
        modes.set(torchPlayers[i].id, 'light-only')
      } else {
        modes.set(torchPlayers[i].id, 'off')
      }
    }

    return modes
  })

  // Standalone shadow light that follows the closest torch-on remote player.
  // castShadow is always true (never toggled) to avoid WebGPU PointShadowNode crash.
  // When no target exists, intensity is set to 0.
  let remoteShadowLightPos = $derived.by(() => {
    if (!currentPlayer) return null

    const playerPos = currentPlayer.position
    const torchPlayers: { id: string; dist: number }[] = []

    for (const [id, player] of otherPlayers) {
      const rp = remotePlayers.get(id)
      if (!player.torchOn || !rp || !remoteVisibility.get(id)) continue
      const dx = rp.position.x - playerPos.x
      const dz = rp.position.z - playerPos.z
      torchPlayers.push({ id, dist: dx * dx + dz * dz })
    }

    if (torchPlayers.length === 0) return null

    torchPlayers.sort((a, b) => a.dist - b.dist)
    const closestId = torchPlayers[0].id
    const rp = remotePlayers.get(closestId)!
    const y = heightManager.getHeightAtWorldPosition(rp.position.x, rp.position.z) ?? rp.position.y
    const base = new THREE.Vector3(rp.position.x, y, rp.position.z)
    const offset = TORCH_OFFSET.clone().applyAxisAngle(Y_AXIS, rp.rotation)
    return base.add(offset)
  })

  let remoteShadowLight = $state<THREE.PointLight | undefined>(undefined)
  let remoteShadowFlickerTime = 0

  export function updateRemoteShadowFlicker(deltaTime: number) {
    if (remoteShadowLight && remoteShadowLightPos) {
      remoteShadowFlickerTime = applyTorchFlickerWorld(
        remoteShadowLight, remoteShadowFlickerTime, deltaTime,
        remoteShadowLightPos.x, remoteShadowLightPos.y, remoteShadowLightPos.z,
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
    health={currentPlayer.health}
    maxHealth={currentPlayer.maxHealth}
    onAttackDuration={onAttackDuration}
    onDyingFinished={onCurrentPlayerDyingFinished}
    bind:isLoading={isCurrentPlayerLoading}
    lastDamageInfo={currentPlayer.lastDamageInfo}
    lastRegenInfo={currentPlayer.lastRegenInfo}
    torchMode="local"
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
        health={player.health}
        maxHealth={player.maxHealth}
        torchMode={remoteTorchModes.get(player.id) ?? 'off'}
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
{/if}
