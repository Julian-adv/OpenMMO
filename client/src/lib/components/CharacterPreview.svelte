<script lang="ts">
  import { T, useLoader, useTask } from '@threlte/core'
  import * as THREE from 'three'
  import { GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js'
  import { onMount } from 'svelte'
  import {
    ANIMATION_ORDER,
    AnimationIndex,
    AnimationName,
  } from '../types/animations'

  interface Props {
    positionX: number
    positionY: number
    positionZ: number
    selected: boolean
  }

  let { positionX, positionY, positionZ, selected }: Props = $props()
  const gltf = useLoader(GLTFLoader).load('/models/maria.glb')
  const locomotionGltf = useLoader(GLTFLoader).load('/models/animations/locomotion.glb')

  let mixer = $state<THREE.AnimationMixer | null>(null)
  let currentAction = $state<THREE.AnimationAction | null>(null)
  let modelRoot = $state<THREE.Group | null>(null)
  let validAnimations = $state<THREE.AnimationClip[]>([])
  let spotlightRef = $state<THREE.SpotLight | undefined>(undefined)
  let fillSpotlightRef = $state<THREE.SpotLight | undefined>(undefined)
  let spotlightTarget = $state<THREE.Object3D | undefined>(undefined)
  const OVERLAP_BEFORE_END = 0.3
  const LOCOMOTION_WAIT_TIMEOUT_MS = 2000
  const LOCOMOTION_ANIMATION_NAMES = new Set<string>([
    AnimationName.IDLE1,
    AnimationName.IDLE2,
    AnimationName.IDLE3,
    AnimationName.IDLE4,
    AnimationName.WALK,
    AnimationName.JOG,
    AnimationName.RUN,
  ])

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
    baseAnimations: THREE.AnimationClip[],
    locomotionAnimations: THREE.AnimationClip[]
  ) {
    if (mixer || modelRoot) return

    const scene = SkeletonUtils.clone(sourceScene)
    const newModelRoot = new THREE.Group()
    newModelRoot.add(scene)

    newModelRoot.traverse((child) => {
      if (child instanceof THREE.Mesh) {
        child.castShadow = true
        child.receiveShadow = true
      }
    })

    const baseClipByName = new Map(baseAnimations.map((clip) => [clip.name, clip]))
    const locomotionClipByName = new Map(
      locomotionAnimations.map((clip) => [clip.name, clip])
    )

    validAnimations = ANIMATION_ORDER.map((targetName) => {
      const baseClip = baseClipByName.get(targetName)
      const locomotionClip = locomotionClipByName.get(targetName)
      return LOCOMOTION_ANIMATION_NAMES.has(targetName)
        ? (locomotionClip ?? baseClip ?? baseAnimations[0] ?? locomotionAnimations[0])
        : (baseClip ?? locomotionClip ?? baseAnimations[0] ?? locomotionAnimations[0])
    })

    if (validAnimations.length > 0) {
      mixer = new THREE.AnimationMixer(newModelRoot)
      playIdleAnimation()
    }

    modelRoot = newModelRoot
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
      const locomotionTimedOut =
        Date.now() - waitStartTime >= LOCOMOTION_WAIT_TIMEOUT_MS

      if ($gltf && ($locomotionGltf || locomotionTimedOut)) {
        setupModel(
          $gltf.scene,
          $gltf.animations ?? [],
          $locomotionGltf?.animations ?? []
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
