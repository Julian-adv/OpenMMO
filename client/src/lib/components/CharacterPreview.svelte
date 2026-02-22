<script lang="ts">
  import { T, useLoader, useTask } from '@threlte/core'
  import * as THREE from 'three'
  import { GLTFLoader } from 'three/examples/jsm/Addons.js'
  import { onMount } from 'svelte'
  import { AnimationIndex } from '../types/animations'
  import {
    LOCOMOTION_WAIT_TIMEOUT_MS,
    createCharacterModelRoot,
    getGltfAnimations,
    normalizeCharacterModelScale,
    retargetAnimationsForCharacterModel,
    selectOrderedCharacterAnimations,
  } from '../utils/characterAnimationUtils'
  import {
    CHARACTER_ANIMATION_SOURCE_MODEL_PATH,
    CHARACTER_ANIMATION_PACK_PATHS,
    CHARACTER_MODEL_PATH,
  } from '../utils/modelPaths'

  interface Props {
    positionX: number
    positionY: number
    positionZ: number
    selected: boolean
  }

  let { positionX, positionY, positionZ, selected }: Props = $props()
  const gltf = useLoader(GLTFLoader).load(CHARACTER_MODEL_PATH)
  const locomotionGltf = useLoader(GLTFLoader).load(
    CHARACTER_ANIMATION_PACK_PATHS.locomotion
  )
  const combatMeleeGltf = useLoader(GLTFLoader).load(
    CHARACTER_ANIMATION_PACK_PATHS.combatMelee
  )
  const retargetSourceGltf = useLoader(GLTFLoader).load(
    CHARACTER_ANIMATION_SOURCE_MODEL_PATH
  )

  let mixer = $state<THREE.AnimationMixer | null>(null)
  let currentAction = $state<THREE.AnimationAction | null>(null)
  let modelRoot = $state<THREE.Group | null>(null)
  let validAnimations = $state<THREE.AnimationClip[]>([])
  let spotlightRef = $state<THREE.SpotLight | undefined>(undefined)
  let fillSpotlightRef = $state<THREE.SpotLight | undefined>(undefined)
  let spotlightTarget = $state<THREE.Object3D | undefined>(undefined)
  const OVERLAP_BEFORE_END = 0.3

  function playIdleAnimation() {
    if (!mixer || validAnimations.length === 0) return

    const idleIndices = [
      AnimationIndex.IDLE1,
      AnimationIndex.IDLE2,
      AnimationIndex.IDLE3,
      AnimationIndex.IDLE4,
    ]
    const idleIndex = idleIndices[Math.floor(Math.random() * idleIndices.length)]
    const clip = validAnimations[idleIndex]
    if (!clip) return

    const newAction = mixer.clipAction(clip)
    newAction.reset()
    newAction.loop = THREE.LoopOnce
    newAction.clampWhenFinished = true
    newAction.paused = !selected

    if (currentAction && newAction !== currentAction) {
      newAction.crossFadeFrom(currentAction, 0.3, true)
    }

    newAction.play()
    currentAction = newAction
  }

  function setupModel(
    sourceScene: THREE.Object3D,
    retargetSourceScene: THREE.Object3D | null | undefined,
    baseAnimations: THREE.AnimationClip[],
    locomotionAnimations: THREE.AnimationClip[],
    combatMeleeAnimations: THREE.AnimationClip[]
  ) {
    if (mixer || modelRoot) return

    const { modelRoot: newModelRoot } = createCharacterModelRoot(sourceScene)
    modelRoot = newModelRoot

    validAnimations = retargetAnimationsForCharacterModel(
      newModelRoot,
      retargetSourceScene,
      selectOrderedCharacterAnimations(
        baseAnimations,
        locomotionAnimations,
        combatMeleeAnimations
      ).map(({ clip }) => clip)
    )

    if (validAnimations.length > 0) {
      try {
        mixer = new THREE.AnimationMixer(newModelRoot)
        playIdleAnimation()
      } catch (error) {
        console.warn('Failed to start preview animation clips', error)
        if (mixer) {
          mixer.stopAllAction()
          mixer = null
        }
        currentAction = null
        validAnimations = []
      }
    }

    normalizeCharacterModelScale(newModelRoot)
  }

  $effect(() => {
    if (!mixer || !currentAction) return

    if (selected) {
      currentAction.paused = false
      return
    }

    currentAction.paused = true
    currentAction.time = 0
    mixer.setTime(0)
  })

  $effect(() => {
    if (!spotlightTarget) return
    if (spotlightRef) spotlightRef.target = spotlightTarget
    if (fillSpotlightRef) fillSpotlightRef.target = spotlightTarget
  })

  onMount(() => {
    const waitStartTime = Date.now()
    const checkGltf = () => {
      const animationPackTimedOut =
        Date.now() - waitStartTime >= LOCOMOTION_WAIT_TIMEOUT_MS
      const animationPacksReady =
        ($locomotionGltf && $combatMeleeGltf) || animationPackTimedOut
      const retargetSourceReady = !!$retargetSourceGltf || animationPackTimedOut

      if ($gltf && animationPacksReady && retargetSourceReady) {
        if (!$retargetSourceGltf && animationPackTimedOut) {
          console.warn(
            'Retarget source GLB load timeout in preview, using non-retargeted clips'
          )
        }
        setupModel(
          $gltf.scene,
          $retargetSourceGltf?.scene,
          getGltfAnimations($gltf),
          getGltfAnimations($locomotionGltf),
          getGltfAnimations($combatMeleeGltf)
        )
        return
      }

      setTimeout(checkGltf, 100)
    }
    checkGltf()

    return () => {
      if (mixer) {
        mixer.stopAllAction()
        mixer = null
      }
      modelRoot = null
    }
  })

  useTask((delta) => {
    if (!selected || !mixer || !currentAction) return

    mixer.update(delta)

    const clip = currentAction.getClip()
    if (clip && clip.duration > 0) {
      const remainingTime = clip.duration - currentAction.time
      if (remainingTime <= OVERLAP_BEFORE_END) {
        playIdleAnimation()
      }
    }
  })
</script>

{#if modelRoot}
  <T.Group position={[positionX, positionY, positionZ]}>
    <T is={modelRoot} />
  </T.Group>
  <T.Object3D
    position={[positionX, positionY + 0.9, positionZ]}
    bind:ref={spotlightTarget}
  />
  {#if selected}
    <T.SpotLight
      bind:ref={spotlightRef}
      position={[positionX, positionY + 4.0, positionZ + 1.2]}
      intensity={9.0}
      angle={0.34}
      penumbra={0.22}
      distance={14}
      decay={1.2}
      color="#ffffff"
      castShadow
      shadow.mapSize.width={2048}
      shadow.mapSize.height={2048}
      shadow.camera.near={0.5}
      shadow.camera.far={18}
      shadow.bias={-0.0002}
      shadow.normalBias={0.02}
    />
    <T.SpotLight
      bind:ref={fillSpotlightRef}
      position={[positionX, positionY + 2.5, positionZ + 3.1]}
      intensity={3.4}
      angle={0.52}
      penumbra={0.8}
      distance={12}
      decay={1.2}
      color="#fff2d8"
    />
  {/if}
{/if}
