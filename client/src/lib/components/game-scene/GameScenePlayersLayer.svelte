<script lang="ts">
  import * as THREE from 'three'
  import PlayerModel from '../PlayerModel.svelte'
  import PlayerControl from '../PlayerControl.svelte'
  import type {
    ChatBubble,
    LocalPlayer,
    RemotePlayer,
  } from '../../stores/gameStore'
  import type { PlayerState } from '../../utils/movementUtils'
  import type Monster from '../Monster.svelte'

  interface Props {
    camera: THREE.OrthographicCamera | undefined
    cameraInitialized: boolean
    currentPlayer: LocalPlayer | null
    otherPlayers: Map<string, RemotePlayer>
    remotePlayers: Map<string, PlayerState>
    chatBubbles: Map<string, ChatBubble>
    currentPlayerState: PlayerState
    terrainMeshes: (THREE.Mesh | undefined)[]
    monsterModels: (Monster | undefined)[]
    playerAttackDuration: number
    onStateChange: (newState: PlayerState) => void
    onAttackDuration: (duration: number) => void
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
    monsterModels,
    playerAttackDuration,
    onStateChange,
    onAttackDuration,
    playerControl = $bindable<PlayerControl>(),
    currentPlayerModel = $bindable<PlayerModel | null>(null),
    otherPlayerModels = $bindable<(PlayerModel | undefined)[]>([]),
  }: Props = $props()
</script>

{#if camera && terrainMeshes.some((mesh) => mesh !== undefined)}
  <PlayerControl
    bind:this={playerControl}
    onStateChange={onStateChange}
    {camera}
    groundMeshes={terrainMeshes.filter((mesh) => mesh !== undefined) as THREE.Mesh[]}
    monsterMeshes={monsterModels
      .map((model) => model?.getMeshGroup())
      .filter((group) => group !== undefined) as THREE.Group[]}
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
    attackCounter={currentPlayerState.attackCounter}
    speed={currentPlayerState.speed}
    rotation={currentPlayerState.rotation}
    movementMode={currentPlayerState.movementMode}
    {camera}
    chatBubble={chatBubbles.get(currentPlayer.id)?.message}
    onAttackDuration={onAttackDuration}
    lastDamageInfo={currentPlayer.lastDamageInfo}
  />
{/if}

{#if cameraInitialized && camera}
  {#each [...otherPlayers.values()] as player, index (player.id)}
    {@const remotePlayer = remotePlayers.get(player.id)}
    {#if remotePlayer}
      <PlayerModel
        bind:this={otherPlayerModels[index]}
        position={new THREE.Vector3(
          remotePlayer.position.x,
          remotePlayer.position.y,
          remotePlayer.position.z
        )}
        name={player.name}
        isCurrentPlayer={false}
        playerState={remotePlayer.state}
        attackCounter={remotePlayer.attackCounter}
        speed={remotePlayer.speed}
        rotation={remotePlayer.rotation}
        movementMode={remotePlayer.movementMode}
        {camera}
        chatBubble={chatBubbles.get(player.id)?.message}
      />
    {/if}
  {/each}
{/if}
