<script lang="ts" module>
  import { applyCrystalGlow } from '../effects/crystal-glow'
  import * as THREE from 'three'

  // Bind-pose bounding box per model file, shared across instances. Sizes the
  // nameplate height and the invisible hover proxy. Deliberately non-reactive.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const bindPoseBoxByModel = new Map<string, THREE.Box3>()

  // Raw corpse ground offset per monster type (die-clip final pose is
  // type-invariant), so the per-vertex scan runs once per type, not per
  // corpse. Deliberately non-reactive.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const corpseGroundOffsetByType = new Map<string, number>()

  // Primal Roar's dizzy marker: stars circling over the head, shared by every
  // stunned monster.
  const STUN_STAR_COUNT = 3
  const STUN_STAR_GEOMETRY = (() => {
    const shape = new THREE.Shape()
    for (let i = 0; i < 10; i++) {
      const r = i % 2 === 0 ? 0.09 : 0.04
      const a = Math.PI / 2 + (i * Math.PI) / 5
      if (i === 0) shape.moveTo(Math.cos(a) * r, Math.sin(a) * r)
      else shape.lineTo(Math.cos(a) * r, Math.sin(a) * r)
    }
    return new THREE.ShapeGeometry(shape)
  })()
  const STUN_STAR_MATERIAL = new THREE.MeshBasicMaterial({
    color: '#ffe066',
    transparent: true,
    depthTest: false,
    depthWrite: false,
  })
</script>

<script lang="ts">
  import { T, useLoader, useThrelte } from '@threlte/core'
  import TextLabel from './TextLabel.svelte'
  import ResourceBar, { RESOURCE_BAR_HEIGHT } from './ResourceBar.svelte'
  import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'
  import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js'
  import { get } from 'svelte/store'
  import { onDestroy, untrack } from 'svelte'
  import { timeScale } from '../stores/timeStore'
  import { DamageTextEmitter } from '../effects/damage-text-pool'

  import type { MonsterData } from '../types/Monster'
  import { withBarkanRuneGrip } from '../utils/barkanRuneGrip'
  import { seatMonsterWeapon } from '../utils/monsterWeapon'
  import {
    MONSTER_SKILLS,
    type MonsterSkillEffects,
  } from '../effects/monster-skills'
  import {
    attackClipNames,
    getMonsterDef,
    splitClipNames,
  } from '../data/monsterDefs'
  import { getItemDef } from '../data/itemDefs'
  import { playMonsterSound } from '../managers/sfxManager'
  import {
    computeCorpseGroundOffset,
    loadSharedPackClipsForModel,
  } from '../utils/characterAnimationUtils'
  import { billboardScale } from '../utils/billboardScale'
  import { hoveredMonsterId } from '../stores/gameStore'
  import { HOVER_SCALE_IDLE, stickyHoverScale } from '../utils/stickyHover'
  import type { TerrainHeightManager } from '../managers/terrainHeightManager'
  import TargetRing from './TargetRing.svelte'
  import { warmupPipelines } from '../utils/pipelineWarmup'
  import { shake, sleepSink } from '../effects/dormant-sleep'
  import { DeathCrumble } from '../effects/death-crumble'
  import {
    disposeMeshMaterials,
    disposeObjectTree,
  } from '../utils/disposeObjectTree'

  interface Props {
    position: { x: number; y: number; z: number }
    rotation: number
    monsterState: MonsterData['state']
    attackCounter?: number
    attackFromLeft?: boolean
    hitCounter?: number
    stunnedUntil?: number
    skill?: MonsterData['skill']
    id: string
    type: string
    floorLevel?: number
    heightManager?: TerrainHeightManager | null
    lastDamageInfo?: MonsterData['lastDamageInfo']
    droppedWeaponItemDefId?: string
    sinkAt?: number
    health: number
    maxHealth: number
    showHealth?: boolean
    onHitFinished?: () => void
  }

  let {
    position,
    rotation,
    monsterState,
    attackCounter,
    attackFromLeft = false,
    hitCounter,
    stunnedUntil,
    skill,
    id,
    type,
    floorLevel = 0,
    heightManager = null,
    lastDamageInfo,
    droppedWeaponItemDefId,
    sinkAt,
    health,
    maxHealth,
    showHealth = false,
    onHitFinished,
  }: Props = $props()

  const def = $derived(getMonsterDef(type))
  const attackClips = $derived(attackClipNames(def))
  // Re-rolled at the start of every swing, so this is only a seed — `untrack`
  // says that out loud, the way the model resolution below does, instead of
  // leaving a reactive read that never re-runs.
  let attackClip = untrack(() => attackClips[0])

  // Monster type is fixed for the component's lifetime, so the model and any
  // hand weapon are resolved once at init from the initial type, not reactively.
  const initialDef = untrack(() => getMonsterDef(type))
  const initialModel = initialDef?.model ?? 'monsters/scp939.glb'
  const initialScale = initialDef?.scale ?? 1
  const isBoss = initialDef?.boss === true
  const gltf = useLoader(GLTFLoader, {
    extend: (loader) => loader.setMeshoptDecoder(MeshoptDecoder),
  }).load(`/models/${initialModel}`)

  // Monsters rigged on the character skeleton borrow the player's animation
  // packs, which every client already has cached, instead of shipping clips.
  let sharedClips: THREE.AnimationClip[] = []

  // Every anim* field of the def, so only the clips this monster plays are
  // retargeted — the packs carry three times as many.
  const usedClipNames = Object.entries(initialDef ?? {})
    .filter(([key, value]) => key.startsWith('anim') && !!value)
    .flatMap(([, value]) => splitClipNames(value as string))

  function findClip(name: string): THREE.AnimationClip | undefined {
    const clip =
      $gltf?.animations.find((c) => c.name === name) ??
      sharedClips.find((c) => c.name === name)
    return clip &&
      initialDef?.id === 'barkan' &&
      initialDef.weapon === 'weapons/rune_blade.glb'
      ? withBarkanRuneGrip(clip)
      : clip
  }

  // Optional hand weapon, attached to a skeleton bone.
  const initialWeapon = initialDef?.weapon
  const initialWeaponBone = initialDef?.weaponBone
  const initialWeaponModel = initialWeapon
    ? (getItemDef(initialWeapon)?.worldModel ?? initialWeapon)
    : undefined
  const weaponGltf = initialWeaponModel
    ? useLoader(GLTFLoader).load(`/models/${initialWeaponModel}`)
    : undefined

  const WEAPON_SCALE = 1
  let weaponAttached = false
  let weaponObject: THREE.Object3D | undefined

  let mixer = $state<THREE.AnimationMixer | undefined>(undefined)
  let currentAction = $state<THREE.AnimationAction | undefined>(undefined)
  let model: THREE.Group | undefined = $state(undefined)
  let modelReady = $state(false)
  const threlte = useThrelte()
  let group = $state<THREE.Group>()
  let nametagGroup = $state<THREE.Group | undefined>(undefined)
  let nametagHeight = $state(2.5)
  let stunned = $state(false)
  let stunGroup = $state<THREE.Group | undefined>(undefined)
  let stunSpin = 0
  let stunRadius = 0.25
  let hoverBox = $state<THREE.Box3 | null>(null)
  let hoverProxyGroup = $state<THREE.Group | undefined>(undefined)
  // Hover hysteresis: the hovered monster's proxy inflates so the pointer
  // must drift further out before the target drops.
  const stickyScale = $derived.by(() => {
    if (!hoverBox || $hoveredMonsterId !== id) return HOVER_SCALE_IDLE
    return stickyHoverScale(
      {
        x: hoverBox.max.x - hoverBox.min.x,
        y: hoverBox.max.y - hoverBox.min.y,
        z: hoverBox.max.z - hoverBox.min.z,
      },
      initialScale
    )
  })
  // Target ring sized to the bind-pose footprint.
  const ringRadius = $derived(
    hoverBox
      ? Math.max(
          0.4,
          (Math.max(
            hoverBox.max.x - hoverBox.min.x,
            hoverBox.max.z - hoverBox.min.z
          ) /
            2) *
            initialScale
        )
      : 0
  )
  let animDebugInfo = $state('')
  // Starts true for a monster that is already a corpse on spawn (AOI
  // re-entry), so the dead-pose clip plays instead of the fall.
  let isDeadAnimationFinished = $state(untrack(() => monsterState) === 'dead')
  let isAttackAnimationFinished = $state(true)
  let lastMonsterState = $state<MonsterData['state'] | undefined>(undefined)
  let lastDeadAnimFinished = $state(false)
  let lastAttackAnimFinished = $state(true)
  let lastAttackCounter = $state<number | undefined>(undefined)
  let lastHitCounter = $state<number | undefined>(undefined)
  let lastSkillCounter = $state<number | undefined>(
    untrack(() => skill?.counter)
  )
  // One set per skill this monster has cast, kept for its next cast.
  const skillEffects: {
    id: string
    clip: THREE.AnimationClip
    effects: MonsterSkillEffects
    sounded: boolean
  }[] = []
  onDestroy(() => {
    for (const { effects } of skillEffects) effects.dispose()
  })
  const crumbleSound = initialDef?.crumbleSound
  const crumble = crumbleSound
    ? new DeathCrumble(threlte.scene, crumbleSound)
    : undefined
  // A corpse already down when this mounted has crumbled away.
  let deadMs = untrack(() => monsterState) === 'dead' ? Infinity : 0
  if (crumble && deadMs === 0) {
    crumble.warm(threlte.renderer, threlte.camera.current, threlte.scene)
  }
  onDestroy(() => crumble?.dispose())
  const damageText = new DamageTextEmitter()
  onDestroy(() => damageText.dispose())
  onDestroy(() => {
    if (weaponObject) disposeMeshMaterials(weaponObject)
    if (!model) return
    if (materialsCloned) disposeMeshMaterials(model)
    disposeObjectTree(model)
  })
  let lastAppliedOpacity = 1
  let materialsCloned = false
  let deadGroundApplied = false
  let corpseTimer = 0
  const CORPSE_FADE_START = 55
  const CORPSE_FADE_DURATION = 5

  function cloneMaterials() {
    if (materialsCloned || !model) return
    materialsCloned = true
    model.traverse((child) => {
      if ((child as THREE.Mesh).isMesh) {
        const mesh = child as THREE.Mesh
        if (Array.isArray(mesh.material)) {
          mesh.material = mesh.material.map((m) => m.clone())
        } else {
          mesh.material = mesh.material.clone()
        }
      }
    })
  }

  function applyOpacity(opacity: number) {
    if (!model || opacity === lastAppliedOpacity) return
    cloneMaterials()
    lastAppliedOpacity = opacity
    model.traverse((child) => {
      if ((child as THREE.Mesh).isMesh) {
        const mesh = child as THREE.Mesh
        const materials = Array.isArray(mesh.material)
          ? mesh.material
          : [mesh.material]
        for (const mat of materials) {
          mat.transparent = true
          mat.opacity = opacity
        }
        mesh.castShadow = opacity >= 0.25
      }
    })
  }

  // The death clip clamps with the body still raised, so settle the
  // corpse onto the ground — unless its clip was already grounded on
  // load, where settling again would just show as a jump, or the
  // type opts out because its clip ends at ground level as authored.
  // Pose-dependent: only call with the corpse pose applied to the skeleton.
  function applyCorpseGround() {
    if (
      model &&
      !deadGroundApplied &&
      !initialDef?.sharedAnims &&
      initialDef?.corpseAutoGround !== false
    ) {
      deadGroundApplied = true
      let offset = corpseGroundOffsetByType.get(type)
      if (offset === undefined) {
        offset = computeCorpseGroundOffset(model)
        corpseGroundOffsetByType.set(type, offset)
      }
      // corpseGroundOffset is authored in world metres; de-scale it
      // since model.position lives inside the scaled group.
      model.position.y += offset + (def?.corpseGroundOffset ?? 0) / initialScale
    }
  }

  function settleCorpse() {
    isDeadAnimationFinished = true
    applyCorpseGround()
  }

  function playAnimation(forceRestart = false) {
    if (!mixer || !$gltf) return

    // Monsters without a hit clip: keep what is playing and end the flinch
    // right away.
    if (monsterState === 'hit' && !def?.animHit) {
      onHitFinished?.()
      return
    }

    let clipName = def?.animIdle ?? 'Idle'
    if (monsterState === 'walk') clipName = def?.animWalk ?? 'Walk'
    if (monsterState === 'run') clipName = def?.animRun ?? 'Run'
    if (monsterState === 'attack') {
      clipName = isAttackAnimationFinished
        ? (def?.animAttackIdle ?? def?.animIdle ?? 'Idle')
        : attackClip
    }
    if (monsterState === 'hit') clipName = def?.animHit ?? 'Hit'
    if (monsterState === 'dead') {
      clipName = isDeadAnimationFinished
        ? (def?.animDead ?? 'Dead')
        : (def?.animDie ?? 'Die')
    }

    const clip = findClip(clipName)

    if (clip) {
      const newAction = mixer.clipAction(clip)
      if (newAction !== currentAction || forceRestart) {
        const fadeDuration =
          monsterState === 'hit'
            ? 0.03
            : monsterState === 'dead'
              ? (def?.deathFadeSeconds ?? 0.2)
              : 0.2

        if (currentAction && newAction !== currentAction) {
          currentAction.fadeOut(fadeDuration)
        }

        if (monsterState === 'dead') {
          if (clipName === (def?.animDie ?? 'Die')) {
            newAction.setLoop(THREE.LoopOnce, 1)
            newAction.clampWhenFinished = true
          } else {
            // Post-death pose clip should loop / stay idle
            newAction.setLoop(THREE.LoopRepeat, Infinity)
            newAction.clampWhenFinished = false
          }
        } else if (monsterState === 'hit') {
          newAction.setLoop(THREE.LoopOnce, 1)
          newAction.clampWhenFinished = true
        } else if (monsterState === 'attack' && clipName === attackClip) {
          newAction.setLoop(THREE.LoopOnce, 1)
          newAction.clampWhenFinished = true
        } else {
          newAction.setLoop(THREE.LoopRepeat, Infinity)
          newAction.clampWhenFinished = false
          isDeadAnimationFinished = false
        }

        newAction.reset()
        if (monsterState === 'dead' && !currentAction) {
          // Already dead on our very first play (AOI re-entry corpse): every
          // def reuses the die clip as the dead pose, so jump to its final
          // frame instead of replaying the fall, then evaluate so the ground
          // offset measures the lying body.
          newAction.play()
          newAction.time = Math.max(0, clip.duration - 1e-4)
          mixer.update(0)
          settleCorpse()
        } else {
          newAction.fadeIn(fadeDuration).play()
        }
        currentAction = newAction
      }
    } else {
      console.warn(
        `Animation ${clipName} not found used for state ${monsterState}`
      )
      if (monsterState === 'hit') {
        onHitFinished?.()
      }
      const firstClip = $gltf.animations[0] ?? sharedClips[0]
      if (!currentAction && firstClip) {
        const newAction = mixer.clipAction(firstClip)
        newAction.play()
        currentAction = newAction
      }
    }
  }

  // Plays the cast's clip; false when this model has none to play.
  function startSkill(cast: NonNullable<MonsterData['skill']>) {
    const def = MONSTER_SKILLS[cast.id]
    const clip = def && findClip(def.clip)
    if (!mixer || !model || !clip || monsterState === 'dead') return false
    let entry = skillEffects.find((e) => e.id === cast.id)
    if (!entry && (!initialWeapon || weaponObject?.parent)) {
      entry = {
        id: cast.id,
        clip,
        effects: def.createEffects(
          threlte.scene,
          model,
          weaponObject,
          clip,
          cast,
          initialDef?.footprint
        ),
        sounded: false,
      }
      skillEffects.push(entry)
    }
    if (entry && group) {
      entry.effects.start(group.getWorldPosition(new THREE.Vector3()), cast)
      entry.sounded = false
    }
    attackClip = clip.name
    isAttackAnimationFinished = false
    return true
  }

  function updateSkillEffects(deltaTime: number, camera?: THREE.Camera) {
    for (const entry of skillEffects) {
      const action =
        currentAction?.getClip() === entry.clip ? currentAction : undefined
      const active =
        !!action && monsterState !== 'dead' && !isAttackAnimationFinished
      const time = action?.time ?? 0
      const sound = MONSTER_SKILLS[entry.id].sound
      if (sound && active && !entry.sounded && time * 1000 >= sound.atMs) {
        entry.sounded = true
        playMonsterSound(sound.url)
      }
      entry.effects.update(time, active, deltaTime, camera)
    }
  }

  export function update(deltaTime: number, camera?: THREE.Camera) {
    // 0. Sync Three.js group position imperatively so the refraction render
    //    (which runs during the game loop, before Svelte's reactive updates)
    //    sees the monster at its current position.
    // A sleeping boss grinds down into the ground, nameplate and all.
    const slept = sinkAt === undefined ? 0 : Date.now() - sinkAt
    const sink = sleepSink(slept)
    const sunk = sink * nametagHeight
    if (group) {
      group.position.set(position.x, position.y - sunk, position.z)
      group.rotation.y = rotation
      if (sink > 0) shake(group, slept / 1000, 0.12 * (1 - sink))
    }
    // Nothing left of a crumbled body to animate.
    if (crumble && model && group && camera && monsterState === 'dead') {
      deadMs += deltaTime * 1000
      if (!crumble.update(group, model, position.y, deadMs, camera)) return
    }
    if (hoverProxyGroup) {
      hoverProxyGroup.position.set(position.x, position.y - sunk, position.z)
      hoverProxyGroup.rotation.y = rotation
    }

    // 1. Sync animation with state
    if (monsterState !== 'attack') {
      isAttackAnimationFinished = true
    }
    if (lastSkillCounter !== skill?.counter) {
      lastSkillCounter = skill?.counter
      if (skill && startSkill(skill)) {
        lastAttackCounter = attackCounter
        lastMonsterState = monsterState
        lastAttackAnimFinished = isAttackAnimationFinished
        playAnimation(true)
      }
    }
    if (
      lastAttackCounter !== attackCounter ||
      lastHitCounter !== hitCounter ||
      lastMonsterState !== monsterState ||
      lastDeadAnimFinished !== isDeadAnimationFinished ||
      lastAttackAnimFinished !== isAttackAnimationFinished
    ) {
      const attackCounterChanged = lastAttackCounter !== attackCounter
      // A repeat hit re-lands while the clip is clamped on its last frame; the
      // restart also re-arms the 'finished' event a pending death waits on.
      const hitCounterChanged = lastHitCounter !== hitCounter
      if (attackCounterChanged && monsterState === 'attack') {
        isAttackAnimationFinished = false
        const pool = attackClipNames(def, attackFromLeft)
        attackClip = pool[Math.floor(Math.random() * pool.length)]
        // A swing already under way when this mounted has landed.
        if (def?.attackSound && lastAttackCounter !== undefined) {
          playMonsterSound(def.attackSound, def.attackImpactDelay)
        }
      }
      lastAttackCounter = attackCounter
      lastHitCounter = hitCounter
      lastMonsterState = monsterState
      lastDeadAnimFinished = isDeadAnimationFinished
      lastAttackAnimFinished = isAttackAnimationFinished
      playAnimation(
        (attackCounterChanged && monsterState === 'attack') ||
          (hitCounterChanged && monsterState === 'hit')
      )
    }

    updateSkillEffects(deltaTime, camera)

    // 2. Update damage texts; bosses spawn them above the nameplate so the
    // number isn't drawn behind the name (both are transparent billboards).
    if (camera) {
      damageText.update(
        deltaTime,
        position.x,
        position.y,
        position.z,
        camera,
        { damage: lastDamageInfo },
        isBoss ? nametagHeight + 0.3 : 1.8 * initialScale
      )
    }

    // 3. Corpse fade
    if (monsterState === 'dead') {
      corpseTimer += deltaTime
      if (corpseTimer >= CORPSE_FADE_START) {
        const fadeProgress =
          (corpseTimer - CORPSE_FADE_START) / CORPSE_FADE_DURATION
        applyOpacity(Math.max(0, 1 - fadeProgress))
      }
    } else {
      corpseTimer = 0
    }

    // 4. Update mixer
    if (mixer) {
      mixer.update(deltaTime)

      // Update debug info for slow mode
      const currentTS = get(timeScale)
      if (currentTS < 1.0 && currentAction) {
        const time = currentAction.time.toFixed(2)
        const duration = currentAction.getClip().duration.toFixed(2)
        const animName = currentAction.getClip().name
        animDebugInfo = `[${animName}] ${time}s / ${duration}s`
      } else {
        animDebugInfo = ''
      }
    }

    stunned = (stunnedUntil ?? 0) > performance.now()
    if (stunned && stunGroup && camera) {
      stunGroup.position.set(
        position.x,
        position.y + nametagHeight - 0.25 - sunk,
        position.z
      )
      stunSpin += deltaTime * 5
      stunGroup.children.forEach((star, i) => {
        const a = stunSpin + (i * Math.PI * 2) / STUN_STAR_COUNT
        star.position.set(Math.cos(a) * stunRadius, 0, Math.sin(a) * stunRadius)
        star.quaternion.copy(camera.quaternion)
      })
    }

    // Update nametag to face camera
    if (camera && nametagGroup) {
      nametagGroup.position.set(
        position.x,
        position.y + nametagHeight - sunk,
        position.z
      )
      const s = billboardScale(
        camera.position.distanceTo(nametagGroup.position)
      )
      nametagGroup.scale.set(s, s, s)
      nametagGroup.quaternion.copy(camera.quaternion)
    }
  }

  $effect(() => {
    if ($gltf) {
      // Clone the model for this instance
      if (!model) {
        if (initialDef?.crystalGlow) applyCrystalGlow($gltf.scene)
        const clonedScene = SkeletonUtils.clone($gltf.scene) as THREE.Group

        // Enable shadows on all meshes
        clonedScene.traverse((child) => {
          if ((child as THREE.Mesh).isMesh) {
            child.castShadow = true
            child.receiveShadow = true
            // Add user data to identify as monster part
            child.userData.monsterId = id
          }
        })

        let box = bindPoseBoxByModel.get(initialModel)
        if (!box) {
          box = new THREE.Box3().setFromObject(clonedScene)
          // A bind pose with spread arms overstates the footprint; the def
          // override clamps the XZ extents (model space) around the centre,
          // shrinking the ring, sticky scale, and hover proxy together.
          const radius = def?.hoverRadius
          if (radius !== undefined) {
            const half = radius / initialScale
            for (const axis of ['x', 'z'] as const) {
              const center = (box.min[axis] + box.max[axis]) / 2
              box.min[axis] = Math.max(box.min[axis], center - half)
              box.max[axis] = Math.min(box.max[axis], center + half)
            }
          }
          bindPoseBoxByModel.set(initialModel, box)
        }
        // Hang the nameplate just above the scaled bind-pose head.
        nametagHeight = box.max.y * initialScale + 0.4
        stunRadius = Math.min(
          0.8,
          Math.max(0.25, (box.max.x - box.min.x) * initialScale * 0.35)
        )
        hoverBox = box

        model = clonedScene
        void warmupPipelines(
          threlte,
          `monster:${initialModel}`,
          clonedScene
        ).then(() => {
          modelReady = true
        })
        // Setup mixer on the cloned scene
        mixer = new THREE.AnimationMixer(clonedScene)

        mixer.addEventListener('finished', (e) => {
          const finishedClipName = e.action.getClip().name
          if (finishedClipName === (def?.animHit ?? 'Hit')) {
            onHitFinished?.()
          }
          if (
            attackClips.includes(finishedClipName) ||
            finishedClipName === attackClip
          ) {
            isAttackAnimationFinished = true
          }
          if (finishedClipName === (def?.animDie ?? 'Die')) {
            settleCorpse()
          }
        })

        if (initialDef?.sharedAnims) {
          loadSharedPackClipsForModel(
            initialModel,
            $gltf.scene,
            usedClipNames,
            {
              restClip: initialDef.animDie,
              restOffset: (initialDef.corpseGroundOffset ?? 0) / initialScale,
            }
          ).then((clips) => {
            sharedClips = clips
            playAnimation()
          })
        } else {
          playAnimation()
        }
      }
    }
  })

  // Attach the hand weapon once both the model and weapon GLB are ready, and
  // detach it again if the monster dies and drops the weapon to the ground.
  $effect(() => {
    // The held weapon dropped on death — detach it if it was attached.
    if (monsterState === 'dead' && droppedWeaponItemDefId) {
      if (weaponObject) {
        weaponObject.removeFromParent()
        disposeMeshMaterials(weaponObject)
        disposeObjectTree(weaponObject)
        weaponObject = undefined
        weaponAttached = false
      }
      return
    }

    if (
      weaponAttached ||
      !model ||
      !initialWeapon ||
      !initialWeaponBone ||
      !weaponGltf ||
      !$weaponGltf
    )
      return

    let bone: THREE.Object3D | undefined
    model.traverse((o) => {
      if (o.name === initialWeaponBone) bone = o
    })
    if (!bone) {
      console.warn(`Weapon bone ${initialWeaponBone} not found on ${type}`)
      weaponAttached = true
      return
    }

    const weapon = $weaponGltf.scene.clone(true)
    weaponObject = weapon
    const attachBone = seatMonsterWeapon(bone, weapon, initialDef ?? {})
    weaponObject.scale.setScalar(WEAPON_SCALE)
    // Skill effects find the blade again in their clones by this name.
    weapon.name = 'MonsterWeapon'
    weaponObject.traverse((child) => {
      if ((child as THREE.Mesh).isMesh) {
        const mesh = child as THREE.Mesh
        // Clone materials so corpse-fade opacity is per-instance.
        mesh.material = Array.isArray(mesh.material)
          ? mesh.material.map((m) => m.clone())
          : mesh.material.clone()
        mesh.castShadow = true
        mesh.receiveShadow = true
        // Clicking the weapon should still target the monster.
        child.userData.monsterId = id
      }
    })
    weaponAttached = true
    void warmupPipelines(
      threlte,
      `monster-weapon:${initialWeaponModel}`,
      weapon
    ).then(() => {
      if (weaponObject === weapon) attachBone.add(weapon)
    })
  })

  // Export the model group for raycasting from parent
  export function getMeshGroup() {
    return group
  }

  export function getMarkAnchor(target: THREE.Vector3) {
    if (!group) return false
    group.getWorldPosition(target)
    target.y += nametagHeight + 0.15
    return true
  }

  export function getNametagGroup() {
    return nametagGroup
  }

  export function getHoverMeshGroup() {
    return hoverProxyGroup
  }
</script>

{#if model && modelReady}
  <T.Group
    bind:ref={group}
    position={[position.x, position.y, position.z]}
    rotation={[0, rotation, 0]}
    scale={[initialScale, initialScale, initialScale]}
  >
    <T is={model} castShadow receiveShadow />
  </T.Group>
{/if}

{#if model && hoverBox}
  <!-- Invisible bind-pose box the 20 Hz hover raycast tests instead of the
       skinned triangles. Raycasts ignore `visible`, so it never renders. Kept
       out of the model group so clicks still hit the actual silhouette. -->
  <T.Group
    bind:ref={hoverProxyGroup}
    position={[position.x, position.y, position.z]}
    rotation={[0, rotation, 0]}
    scale={[initialScale, initialScale, initialScale]}
    userData={{ monsterId: id }}
  >
    <T.Mesh
      visible={false}
      scale={stickyScale}
      position={[
        (hoverBox.min.x + hoverBox.max.x) / 2,
        (hoverBox.min.y + hoverBox.max.y) / 2,
        (hoverBox.min.z + hoverBox.max.z) / 2,
      ]}
    >
      <T.BoxGeometry
        args={[
          hoverBox.max.x - hoverBox.min.x,
          hoverBox.max.y - hoverBox.min.y,
          hoverBox.max.z - hoverBox.min.z,
        ]}
      />
      <T.MeshBasicMaterial />
    </T.Mesh>
  </T.Group>
{/if}

{#if monsterState !== 'dead' && $hoveredMonsterId === id && ringRadius > 0}
  <TargetRing
    {heightManager}
    x={position.x}
    z={position.z}
    radius={ringRadius}
    {floorLevel}
    fallbackY={position.y}
  />
{/if}

{#if stunned && monsterState !== 'dead'}
  <T.Group bind:ref={stunGroup}>
    {#each { length: STUN_STAR_COUNT }, i (i)}
      <T.Mesh
        geometry={STUN_STAR_GEOMETRY}
        material={STUN_STAR_MATERIAL}
        renderOrder={4}
        dispose={false}
      />
    {/each}
  </T.Group>
{/if}

<!-- Name tag / Debug info -->
<T.Group bind:ref={nametagGroup}>
  {#if monsterState !== 'dead' && (isBoss || $hoveredMonsterId === id)}
    <!-- Boss: permanent gold nameplate. Others: hover-only name, sized to
         match the ground-item hover label. -->
    <TextLabel
      text={initialDef?.name ?? type}
      outlineWidth={5}
      position={[0, 0, 0]}
      anchorX="center"
      anchorY="bottom"
      {...isBoss
        ? { fontSize: 0.3, color: '#ffd166', outlineColor: '#422d00' }
        : {
            fontSize: 0.22,
            color: '#ffffff',
            outlineColor: '#000000',
            depthTest: false,
            renderOrder: 4,
          }}
    />
  {/if}
  {#if showHealth && monsterState !== 'dead' && maxHealth > 0}
    <ResourceBar
      y={-RESOURCE_BAR_HEIGHT}
      ratio={Math.max(0.001, health / maxHealth)}
      color="#ff0000"
    />
  {/if}
  {#if animDebugInfo}
    <TextLabel
      text={id}
      fontSize={0.2}
      color="#ffffff"
      position={[0, 0.3, 0]}
      anchorX="center"
      anchorY="middle"
    />
    <TextLabel
      text={animDebugInfo}
      fontSize={0.2}
      color="#ffff00"
      position={[0, 0.6, 0]}
      anchorX="center"
      anchorY="middle"
    />
  {/if}
</T.Group>
