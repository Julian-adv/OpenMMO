<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { onDestroy } from 'svelte'
  import { AnimationIndex } from '../types/animations'
  import {
    createCharacterModelRoot,
    getGltfAnimations,
    retargetOrderedCharacterAnimationsForModel,
    selectOrderedCharacterAnimations,
  } from '../utils/characterAnimationUtils'
  import {
    CHARACTER_ANIMATION_PACK_PATHS,
    getCharacterModelPath,
  } from '../utils/modelPaths'
  import { loadGLB } from '../utils/gltfCache'
  import type { CharacterClass, Gender } from '../network/networkTypes'

  interface Props {
    positionX: number
    positionY: number
    positionZ: number
    rotationY?: number
    selected: boolean
    characterClass: CharacterClass
    gender?: Gender
  }

  let { positionX, positionY, positionZ, rotationY = 0, selected, characterClass, gender }: Props = $props()

  // Load via shared cache so GLBs persist across Canvas lifecycles
  let characterGltfData = $state<GLTF | null>(null)
  let locomotionGltfData = $state<GLTF | null>(null)
  let combatMeleeGltfData = $state<GLTF | null>(null)

  loadGLB(getCharacterModelPath(characterClass, gender)).then((g) => { characterGltfData = g })
  loadGLB(CHARACTER_ANIMATION_PACK_PATHS.locomotion).then((g) => { locomotionGltfData = g })
  loadGLB(CHARACTER_ANIMATION_PACK_PATHS.combatMelee).then((g) => { combatMeleeGltfData = g })

  let mixer = $state<THREE.AnimationMixer | null>(null)
  let currentAction = $state<THREE.AnimationAction | null>(null)
  let modelRoot = $state<THREE.Group | null>(null)
  let validAnimations = $state<THREE.AnimationClip[]>([])
  let setupDone = $state(false)

  const OVERLAP_BEFORE_END = 0.3

  let gltfReady = $derived(!!characterGltfData && !!locomotionGltfData && !!combatMeleeGltfData)

  function playIdleAnimation() {
    if (!mixer || validAnimations.length === 0) return

    const idleIndices = [
      AnimationIndex.IDLE1,
      AnimationIndex.IDLE2,
      AnimationIndex.IDLE3,
      AnimationIndex.IDLE4,
      AnimationIndex.IDLE5,
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

  // --- Exported interface for parent game loop ---

  export function isGltfReady(): boolean {
    return gltfReady
  }

  export function isSetUp(): boolean {
    return setupDone
  }

  export function setup(): void {
    if (setupDone || !characterGltfData || !locomotionGltfData || !combatMeleeGltfData) return
    setupDone = true

    const sourceScene = characterGltfData.scene
    const { modelRoot: newModelRoot } = createCharacterModelRoot(sourceScene)

    const orderedAnims = selectOrderedCharacterAnimations(
      getGltfAnimations(characterGltfData),
      getGltfAnimations(locomotionGltfData),
      getGltfAnimations(combatMeleeGltfData)
    )

    retargetOrderedCharacterAnimationsForModel(
      newModelRoot,
      orderedAnims,
      {
        base: sourceScene,
        locomotion: locomotionGltfData.scene,
        combatMelee: combatMeleeGltfData.scene,
      }
    ).then((clips) => {
      validAnimations = clips

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

      // Set modelRoot only after animation is playing to avoid T-pose flash
      modelRoot = newModelRoot
    })
  }

  export function update(delta: number): void {
    if (!selected || !mixer || !currentAction) return

    mixer.update(delta)

    const clip = currentAction.getClip()
    if (clip && clip.duration > 0) {
      const remainingTime = clip.duration - currentAction.time
      if (remainingTime <= OVERLAP_BEFORE_END) {
        playIdleAnimation()
      }
    }
  }

  export function dispose(): void {
    if (mixer) {
      mixer.stopAllAction()
      mixer = null
    }
    currentAction = null
    modelRoot = null
    validAnimations = []
    setupDone = false
  }

  // Pause/resume on selection change
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

  onDestroy(() => {
    dispose()
  })
</script>

{#if modelRoot}
  <T.Group position={[positionX, positionY, positionZ]} rotation.y={rotationY}>
    <T is={modelRoot} />
  </T.Group>
{/if}
