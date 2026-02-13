<script lang="ts">
  import { T, useLoader, useTask } from '@threlte/core'
  import * as THREE from 'three'
  import { GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js'
  import { onMount } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { ANIMATION_ORDER, AnimationIndex } from '../types/animations'

  interface Props {
    positionX: number
    selected: boolean
  }

  let { positionX, selected }: Props = $props()

  const gltf = useLoader(GLTFLoader).load('/models/maria.glb')

  let mixer = $state<THREE.AnimationMixer | null>(null)
  let currentAction = $state<THREE.AnimationAction | null>(null)
  let modelRoot = $state<THREE.Group | null>(null)
  let validAnimations = $state<THREE.AnimationClip[]>([])
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
    newAction.paused = false

    if (currentAction && newAction !== currentAction) {
      newAction.crossFadeFrom(currentAction, 0.3, true)
    }

    newAction.play()
    currentAction = newAction
  }

  function setupModel() {
    if ($gltf && !mixer && !modelRoot) {
      const cloned = SkeletonUtils.clone($gltf.scene)
      const newModelRoot = new THREE.Group()
      newModelRoot.add(cloned)

      newModelRoot.traverse((child) => {
        if (child instanceof THREE.Mesh) {
          child.castShadow = true
          child.receiveShadow = true
        }
      })

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const animations: THREE.AnimationClip[] = ($gltf as any).animations || []

      const modelNodeNames = new SvelteSet()
      cloned.traverse((obj) => {
        if (obj.name) modelNodeNames.add(obj.name)
      })

      validAnimations = ANIMATION_ORDER.map((targetName) => {
        const foundClip = animations.find((clip) => clip.name === targetName)
        return foundClip ?? animations[0]
      })

      if (validAnimations.length > 0) {
        mixer = new THREE.AnimationMixer(newModelRoot)
        playIdleAnimation()
      }

      modelRoot = newModelRoot
    }
  }

  onMount(() => {
    const checkGltf = () => {
      if ($gltf) {
        setupModel()
      } else {
        setTimeout(checkGltf, 100)
      }
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
    if (!mixer || !currentAction) return

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
  <T.Group position={[positionX, 0, 0]}>
    <T is={modelRoot} />
  </T.Group>
  {#if selected}
    <T.PointLight
      position={[positionX, 2.5, 1]}
      intensity={1.5}
      color="#7cc9ff"
      distance={5}
    />
  {/if}
{/if}
