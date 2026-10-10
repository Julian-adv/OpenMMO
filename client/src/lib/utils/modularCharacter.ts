import * as THREE from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { robeBaseSkeleton } from '../effects/robe-rig'
import { setPeltPhysicsEnabled } from '../effects/pelt-rig'
import { trimModularClothing } from './modularClothing'

interface SwordTransform {
  position: [number, number, number]
  quaternion: [number, number, number, number]
}

interface SwordPose extends SwordTransform {
  keyframes?: (SwordTransform & { phase: number })[]
}

export interface ModularHandProfile {
  rig_id: string
  iron_sword: SwordPose
  iron_sword_by_clip?: Record<string, SwordPose>
  finger_pose_sources: Record<string, Record<string, string>>
  finger_poses?: Record<
    string,
    {
      clips: string[]
      quaternions: Record<string, [number, number, number, number]>
    }
  >
  finger_relaxation?: Record<
    string,
    Record<
      string,
      {
        rest_quaternion: [number, number, number, number]
        amount: number
      }
    >
  >
}

export function modularRigId(root: THREE.Object3D): string {
  const ids = new Set<string>()
  root.traverse((node) => {
    if (typeof node.userData.rig_id === 'string') ids.add(node.userData.rig_id)
  })
  if (ids.size !== 1) throw new Error('파츠의 리그 ID가 없거나 서로 다릅니다.')
  return [...ids][0]
}

export function skinnedParts(root: THREE.Object3D): THREE.SkinnedMesh[] {
  const result: THREE.SkinnedMesh[] = []
  root.traverse((node) => {
    if (node instanceof THREE.SkinnedMesh) result.push(node)
  })
  return result
}

export const MODULAR_BOOTS = [
  'none',
  'leather',
  'plate',
  'barbarian',
  'rogue',
  'caveman',
  'ranger',
  'priest',
] as const

export interface ModularOutfit {
  hair:
    | 'hair_crop'
    | 'hair_sidepart'
    | 'hair_wavy_bone'
    | 'hair_ranger'
    | 'none'
  top:
    | 'linen'
    | 'leather'
    | 'plate'
    | 'barbarian'
    | 'rogue'
    | 'caveman'
    | 'ranger'
    | 'priest'
    | 'none'
  pants:
    | 'cloth'
    | 'plate'
    | 'barbarian'
    | 'rogue'
    | 'caveman'
    | 'ranger'
    | 'priest'
    | 'none'
  gloves:
    | 'none'
    | 'leather'
    | 'plate'
    | 'barbarian'
    | 'rogue'
    | 'caveman'
    | 'ranger'
  boots: (typeof MODULAR_BOOTS)[number]
  helmet: 'none' | 'plate' | 'barbarian' | 'priest'
}

export const SELECTABLE_HAIR_PARTS = new Set<ModularOutfit['hair']>([
  'hair_crop',
  'hair_wavy_bone',
  'hair_ranger',
])

export const DEFAULT_MODULAR_OUTFIT: ModularOutfit = {
  hair: 'hair_crop',
  top: 'linen',
  pants: 'cloth',
  gloves: 'none',
  boots: 'leather',
  helmet: 'none',
}

export const KNIGHT_MODULAR_OUTFIT: ModularOutfit = {
  hair: 'none',
  top: 'plate',
  pants: 'plate',
  gloves: 'plate',
  boots: 'plate',
  helmet: 'plate',
}

export const BARBARIAN_MODULAR_OUTFIT: ModularOutfit = {
  hair: 'none',
  top: 'barbarian',
  pants: 'barbarian',
  gloves: 'barbarian',
  boots: 'barbarian',
  helmet: 'barbarian',
}

export function modularOutfitParts(outfit: ModularOutfit): Set<string> {
  const selected = new Set<string>()
  if (outfit.pants !== 'none') selected.add(`pants_${outfit.pants}`)
  if (
    outfit.hair !== 'none' &&
    (outfit.helmet === 'none' || outfit.helmet === 'priest')
  )
    selected.add(outfit.hair)
  if (outfit.top === 'linen' || outfit.top === 'leather')
    selected.add('top_linen')
  if (outfit.top !== 'linen' && outfit.top !== 'none')
    selected.add(`top_${outfit.top}`)
  if (outfit.gloves !== 'none') selected.add(`gloves_${outfit.gloves}`)
  if (outfit.boots !== 'none') selected.add(`boots_${outfit.boots}`)
  if (outfit.helmet !== 'none') selected.add(`helmet_${outfit.helmet}`)
  return selected
}

const bodyMaterials = new WeakMap<
  THREE.Material,
  { skin: THREE.Material; underwear: THREE.Material }
>()

function bodyMaterial(material: THREE.Material, underwear: boolean) {
  let variants = bodyMaterials.get(material)
  if (!variants && material.name === 'restored_skin') {
    variants = {
      skin: material,
      underwear: new THREE.MeshStandardMaterial({
        name: 'covered_skin',
        color: new THREE.Color(0.58, 0.36, 0.25),
        metalness: 0,
        roughness: 0.7,
        side: material.side,
      }),
    }
    const covered = variants.underwear
    bodyMaterials.set(material, variants)
    bodyMaterials.set(covered, variants)
    material.addEventListener('dispose', () => {
      covered.dispose()
      bodyMaterials.delete(material)
      bodyMaterials.delete(covered)
    })
  }
  return variants ? variants[underwear ? 'underwear' : 'skin'] : material
}

export function region(mesh: THREE.Object3D): string | undefined {
  for (let node: THREE.Object3D | null = mesh; node; node = node.parent)
    if (typeof node.userData.region === 'string') return node.userData.region
}

function showBaseModularOutfit(
  body: THREE.SkinnedMesh[],
  parts: ReadonlyMap<string, THREE.SkinnedMesh[]>,
  outfit: ModularOutfit,
  leatherBoots: boolean,
  untuckedLinen: boolean
) {
  const linenWaist =
    untuckedLinen &&
    parts.get('top_linen')?.some((mesh) => region(mesh) === 'torso')
  const hidden = new Set<string>()
  const coveredLegs = outfit.pants === 'cloth' || outfit.pants === 'plate'
  const coveredFeet = outfit.boots === 'leather' || outfit.boots === 'plate'
  const shortPants = coveredLegs && outfit.boots === 'barbarian'
  const sleeveCut = (
    {
      none: undefined,
      rogue: undefined,
      caveman: undefined,
      ranger: 'ranger_sleeves',
      leather: 'gloves',
      plate: 'gauntlets',
      barbarian: 'bracers',
    } as const
  )[outfit.gloves]
  const coveredArms = outfit.top !== 'none' && outfit.top !== 'barbarian'
  const shortSleeves = coveredArms && sleeveCut
  if (coveredLegs) {
    if (!shortPants) {
      hidden.add('legs')
      hidden.add('ankles')
    }
    hidden.add('boot_ankles')
  }
  if (coveredArms) {
    for (const region of ['torso', 'upper_arms']) hidden.add(region)
    if (!shortSleeves) hidden.add('forearms')
  }
  if (outfit.top === 'plate') hidden.add('neck')
  if (outfit.gloves === 'leather' || outfit.gloves === 'plate')
    hidden.add('hands')
  if (coveredFeet) {
    hidden.add('feet')
    hidden.add('ankles')
  } else hidden.add('boot_ankles')
  for (const mesh of body) {
    const bodyRegion = region(mesh)
    mesh.visible =
      (untuckedLinen && bodyRegion === 'torso') || !hidden.has(bodyRegion ?? '')
    trimModularClothing(
      mesh,
      [
        outfit.pants === 'none' &&
        (bodyRegion === 'torso' || bodyRegion === 'legs')
          ? 'underwear_seat'
          : undefined,
        linenWaist && bodyRegion === 'torso'
          ? 'linen_waist'
          : shortPants && (bodyRegion === 'legs' || bodyRegion === 'ankles')
            ? 'greaves'
            : shortSleeves && bodyRegion === 'forearms'
              ? sleeveCut
              : undefined,
      ],
      true
    )
    const underwear = outfit.pants !== 'barbarian'
    mesh.material = Array.isArray(mesh.material)
      ? mesh.material.map((material) => bodyMaterial(material, underwear))
      : bodyMaterial(mesh.material, underwear)
  }
  const selected = modularOutfitParts(outfit)
  for (const [id, meshes] of parts)
    for (const mesh of meshes) {
      const partRegion = region(mesh)
      const cut =
        id.startsWith('hair_') &&
        outfit.helmet === 'priest' &&
        !!parts.get('helmet_priest')?.length
          ? 'priest_hat_hair'
          : id === 'helmet_priest' &&
              (outfit.hair === 'hair_ranger' ||
                outfit.hair === 'hair_wavy_bone') &&
              !!parts.get(outfit.hair)?.length
            ? 'priest_hat_lappets'
            : linenWaist && id === 'top_linen' && partRegion === 'torso'
              ? 'linen_untucked'
              : (id === 'top_linen' && partRegion === 'sleeves') ||
                  id === 'top_plate'
                ? sleeveCut
                : id === 'pants_plate' && leatherBoots
                  ? 'leather_boots'
                  : shortPants &&
                      (id === 'pants_plate' ||
                        (id === 'pants_cloth' && partRegion === 'main'))
                    ? 'greaves'
                    : undefined
      mesh.visible =
        selected.has(id) &&
        !(
          id === 'pants_cloth' &&
          (coveredFeet || shortPants) &&
          partRegion === 'cuffs'
        ) &&
        !(
          id === 'pants_cloth' &&
          !coveredFeet &&
          partRegion === 'tucked_cuffs'
        ) &&
        !(
          id === 'pants_cloth' &&
          outfit.top === 'plate' &&
          partRegion === 'waist'
        ) &&
        !(
          id === 'top_linen' &&
          (outfit.top === 'leather'
            ? partRegion === 'torso'
            : partRegion === 'armored_collar')
        )
      trimModularClothing(mesh, mesh.visible ? cut : undefined)
    }
  return selected
}

type ProxiedSlot = 'top' | 'pants' | 'gloves' | 'boots'

export const ROGUE_MODULAR_PARTS = [
  'top_rogue',
  'pants_rogue',
  'gloves_rogue',
  'boots_rogue',
] as const

export const ROGUE_MODULAR_OUTFIT: ModularOutfit = {
  hair: 'hair_crop',
  top: 'rogue',
  pants: 'rogue',
  gloves: 'rogue',
  boots: 'rogue',
  helmet: 'none',
}

const CAVEMAN_MODULAR_PARTS = [
  'top_caveman',
  'pants_caveman',
  'gloves_caveman',
  'boots_caveman',
] as const

export const CAVEMAN_MODULAR_OUTFIT: ModularOutfit = {
  hair: 'hair_crop',
  top: 'caveman',
  pants: 'caveman',
  gloves: 'caveman',
  boots: 'caveman',
  helmet: 'none',
}

export const RANGER_MODULAR_PARTS = [
  'top_ranger',
  'pants_ranger',
  'gloves_ranger',
  'boots_ranger',
] as const

export const RANGER_MODULAR_OUTFIT: ModularOutfit = {
  hair: 'hair_crop',
  top: 'ranger',
  pants: 'ranger',
  gloves: 'ranger',
  boots: 'ranger',
  helmet: 'none',
}

const PRIEST_MODULAR_PARTS = [
  'top_priest',
  'pants_priest',
  'boots_priest',
] as const

export const PRIEST_MODULAR_OUTFIT: ModularOutfit = {
  hair: 'hair_crop',
  top: 'priest',
  pants: 'priest',
  gloves: 'none',
  boots: 'priest',
  helmet: 'priest',
}

const proxied = (style: string) =>
  style === 'rogue' ||
  style === 'caveman' ||
  style === 'ranger' ||
  style === 'priest'

export function showModularOutfit(
  body: THREE.SkinnedMesh[],
  parts: ReadonlyMap<string, THREE.SkinnedMesh[]>,
  outfit: ModularOutfit
): Set<string> {
  const available = <Slot extends ProxiedSlot>(slot: Slot) =>
    proxied(outfit[slot]) && !parts.get(`${slot}_${outfit[slot]}`)?.length
      ? 'none'
      : outfit[slot]
  const top = available('top')
  const pants = available('pants')
  const gloves = available('gloves')
  const boots = available('boots')
  const clothPants =
    pants === 'rogue' || pants === 'ranger' || pants === 'priest'
  const tallBoots = boots === 'caveman' || boots === 'ranger'
  const priest = top === 'priest'
  for (const id of ['pants_barbarian', 'pants_caveman'])
    for (const mesh of parts.get(id) ?? []) setPeltPhysicsEnabled(mesh, !priest)
  const collar = parts
    .get('top_rogue')
    ?.some((mesh) => mesh.userData.fitting_status === 'candidate_tripo_v1')
    ? 'tripo_collar'
    : 'collar'
  const selected = showBaseModularOutfit(
    body,
    parts,
    {
      ...outfit,
      top: priest ? 'linen' : proxied(top) ? 'none' : top,
      pants: clothPants ? 'cloth' : pants === 'caveman' ? 'barbarian' : pants,
      gloves:
        gloves === 'rogue'
          ? 'none'
          : gloves === 'caveman'
            ? 'barbarian'
            : gloves,
      boots: proxied(boots) ? 'leather' : boots,
    },
    boots === 'leather' && !!parts.get('boots_leather')?.length,
    top === 'linen' && pants === 'none'
  )
  if (![top, pants, gloves, boots].some(proxied)) return selected
  for (const [shown, proxy] of [
    [clothPants, 'pants_cloth'],
    [pants === 'caveman', 'pants_barbarian'],
    [gloves === 'caveman', 'gloves_barbarian'],
    [proxied(boots), 'boots_leather'],
    [priest, 'top_linen'],
  ] as const) {
    if (!shown) continue
    selected.delete(proxy)
    for (const mesh of parts.get(proxy) ?? []) mesh.visible = false
  }
  for (const [slot, style] of Object.entries({ top, pants, gloves, boots })) {
    if (proxied(style)) selected.add(`${slot}_${style}`)
  }
  for (const id of [
    ...ROGUE_MODULAR_PARTS,
    ...CAVEMAN_MODULAR_PARTS,
    ...RANGER_MODULAR_PARTS,
    ...PRIEST_MODULAR_PARTS,
  ])
    for (const mesh of parts.get(id) ?? []) mesh.visible = selected.has(id)
  for (const id of boots === 'ranger'
    ? ['pants_plate', 'pants_cloth']
    : boots === 'caveman'
      ? ['pants_plate']
      : [])
    for (const mesh of parts.get(id) ?? [])
      if (mesh.visible) trimModularClothing(mesh, 'tall_boots')
  if (boots === 'priest')
    for (const id of ['pants_plate', 'pants_cloth'])
      for (const mesh of parts.get(id) ?? [])
        if (mesh.visible) trimModularClothing(mesh, 'priest_pants_boots')
  if (priest) {
    const bootCut = tallBoots
      ? 'tall_boots'
      : boots === 'barbarian'
        ? 'greaves'
        : boots === 'leather' &&
          parts.get('boots_leather')?.some((mesh) => mesh.visible) &&
          'leather_boots'
    for (const mesh of parts.get('pants_plate') ?? [])
      if (mesh.visible)
        trimModularClothing(mesh, [bootCut, 'priest_plate_waist'])
    for (const id of ['pants_barbarian', 'pants_caveman'])
      for (const mesh of parts.get(id) ?? [])
        if (mesh.visible)
          trimModularClothing(
            mesh,
            mesh.userData.pelt_physics ? 'priest_fur' : 'priest_plate_waist'
          )
  }
  const coveredWaist =
    pants === 'rogue' &&
    top !== 'none' &&
    top !== 'barbarian' &&
    top !== 'caveman' &&
    parts
      .get('pants_rogue')
      ?.some(
        (mesh) => mesh.userData.fitting_status === 'candidate_tripo_pants_v1'
      )
  const bootHem =
    boots === 'ranger'
      ? 'ranger_pants_boots'
      : boots === 'priest' && 'priest_pants_boots'
  for (const mesh of parts.get('pants_rogue') ?? [])
    trimModularClothing(
      mesh,
      pants === 'rogue' && [
        priest && 'priest_rogue_pockets',
        coveredWaist && 'tripo_pants_waist',
        bootHem,
      ]
    )
  for (const mesh of parts.get('pants_ranger') ?? [])
    trimModularClothing(
      mesh,
      pants === 'ranger' && [
        top === 'plate' && 'ranger_plate_waist',
        bootHem,
        priest && 'priest_plate_waist',
      ]
    )
  const priestBootHem = (
    {
      none: undefined,
      rogue: undefined,
      leather: 'leather_boots',
      plate: 'plate_boots',
      barbarian: 'barbarian_pants_boots',
      caveman: 'caveman_pants_boots',
      ranger: 'ranger_pants_boots',
      priest: 'priest_pants_boots',
    } as const
  )[boots]
  const priestPlateWaist =
    top === 'plate' && parts.get('top_plate')?.some((mesh) => mesh.visible)
  const priestRogueWaist =
    pants === 'priest' &&
    top === 'rogue' &&
    parts.get('top_rogue')?.some((mesh) => mesh.visible)
  for (const mesh of parts.get('pants_priest') ?? [])
    trimModularClothing(
      mesh,
      pants === 'priest' && [
        priestPlateWaist && 'priest_plate_waist',
        priestPlateWaist && 'ranger_plate_waist',
        priestRogueWaist && 'tripo_pants_waist',
        parts.get(`boots_${boots}`)?.some((boot) => boot.visible) &&
          priestBootHem,
      ]
    )
  const priestGreaves =
    pants === 'priest' &&
    boots === 'barbarian' &&
    parts.get('boots_barbarian')?.some((mesh) => mesh.visible)
  const hidden = new Set([
    ...(top === 'rogue' || top === 'ranger' ? ['torso', 'upper_arms'] : []),
    ...(clothPants
      ? priestGreaves
        ? ['boot_ankles']
        : ['legs', 'ankles', 'boot_ankles']
      : []),
    ...(tallBoots ? ['feet', 'ankles', 'boot_ankles'] : []),
  ])
  const rangerSleeves =
    gloves === 'ranger' &&
    (priest ||
      ((top === 'plate' || top === 'linen' || top === 'leather') &&
        parts
          .get(top === 'plate' ? 'top_plate' : 'top_linen')
          ?.some(
            (mesh) =>
              mesh.visible && (top === 'plate' || region(mesh) === 'sleeves')
          )))
  for (const mesh of body) {
    const bodyRegion = region(mesh)
    if (priestGreaves && (bodyRegion === 'legs' || bodyRegion === 'ankles'))
      trimModularClothing(mesh, 'barbarian_pants_boots', true)
    if (bodyRegion === 'neck')
      trimModularClothing(
        mesh,
        top === 'ranger'
          ? 'ranger_collar'
          : top === 'rogue'
            ? collar
            : priest
              ? 'collar'
              : undefined,
        true
      )
    if (hidden.has(bodyRegion ?? '')) mesh.visible = false
    if (gloves === 'caveman' && bodyRegion === 'forearms')
      trimModularClothing(mesh, 'bracers')
    if (gloves === 'ranger' && bodyRegion === 'forearms') {
      trimModularClothing(mesh, 'ranger_gloves')
      if (rangerSleeves) mesh.visible = false
    }
    if (tallBoots && bodyRegion === 'legs')
      trimModularClothing(mesh, 'tall_boots')
    if (boots === 'priest' && bodyRegion === 'legs')
      trimModularClothing(mesh, 'leather_boots')
    if (
      bodyRegion === 'torso' &&
      top === 'rogue' &&
      collar === 'tripo_collar'
    ) {
      trimModularClothing(
        mesh,
        coveredWaist || priestRogueWaist
          ? 'tripo_covered_waist'
          : 'tripo_waist',
        true
      )
      mesh.visible = true
    }
  }
  return selected
}

function matrixMatches(a: THREE.Matrix4, b: THREE.Matrix4): boolean {
  return a.elements.every((value, i) => Math.abs(value - b.elements[i]) < 1e-5)
}

export function modularAnimationClips(
  body: THREE.Object3D,
  pack: { scene: THREE.Object3D; animations: THREE.AnimationClip[] },
  variant: 'corrected' | 'comparison'
): THREE.AnimationClip[] {
  if (
    modularRigId(body) !== modularRigId(pack.scene) ||
    pack.scene.userData.animation_stage !== 'modular-baked-v1' ||
    pack.scene.userData.variant !== variant
  )
    throw new Error('전용 애니메이션의 리그 또는 보정 단계가 다릅니다.')
  const bones = skinnedParts(body)[0]?.skeleton.bones
  if (!bones?.length) throw new Error('몸체의 스킨 골격이 없습니다.')
  body.updateMatrixWorld(true)
  pack.scene.updateMatrixWorld(true)
  for (const bone of bones) {
    const source = pack.scene.getObjectByName(bone.name)
    if (
      !source ||
      source.parent?.name !== bone.parent?.name ||
      !matrixMatches(source.matrix, bone.matrix) ||
      !matrixMatches(source.matrixWorld, bone.matrixWorld)
    )
      throw new Error('전용 애니메이션의 기준 골격이 몸체와 다릅니다.')
  }
  const boneNames = new Set(bones.map((bone) => bone.name))
  const clipNames = new Set<string>()
  for (const clip of pack.animations) {
    const tracks = new Set<string>()
    if (!clip.name || clipNames.has(clip.name) || !clip.validate())
      throw new Error('전용 애니메이션 클립이 올바르지 않습니다.')
    clipNames.add(clip.name)
    for (const track of clip.tracks) {
      const match = /^(\w+)\.(position|quaternion|scale)$/.exec(track.name)
      if (!match || !boneNames.has(match[1]) || tracks.has(track.name))
        throw new Error('전용 애니메이션에 알 수 없는 본 트랙이 있습니다.')
      tracks.add(track.name)
    }
  }
  return pack.animations
}

export function bindModularFace(
  body: THREE.Object3D,
  source: THREE.Object3D
): THREE.SkinnedMesh[] {
  const torso = skinnedParts(body).find((mesh) => region(mesh) === 'torso')
  const skin = (
    Array.isArray(torso?.material) ? torso.material[0] : torso?.material
  ) as THREE.MeshStandardMaterial | undefined
  const meshes = bindModularPart(body, source)
  for (const mesh of meshes) {
    for (const material of Array.isArray(mesh.material)
      ? mesh.material
      : [mesh.material]) {
      if (!(material instanceof THREE.MeshStandardMaterial)) continue
      for (const slot of [
        'map',
        'normalMap',
        'metalnessMap',
        'roughnessMap',
      ] as const) {
        if (!material.userData.modular_body_textures?.includes(slot)) continue
        if (!skin?.[slot])
          throw new Error(`Missing shared body texture: ${slot}`)
        material[slot] = skin[slot]
      }
    }
  }
  return meshes
}

export function bindModularPart(
  body: THREE.Object3D,
  source: THREE.Object3D
): THREE.SkinnedMesh[] {
  if (modularRigId(body) !== modularRigId(source)) {
    throw new Error('몸체와 파츠의 리그 ID가 다릅니다.')
  }
  const bodyMeshes = skinnedParts(body)
  const skeleton = bodyMeshes[0] && robeBaseSkeleton(bodyMeshes[0])
  const sourceMeshes = skinnedParts(source)
  if (!skeleton || sourceMeshes.length === 0) {
    throw new Error('몸체 또는 파츠에 스킨 골격이 없습니다.')
  }
  for (const mesh of [...bodyMeshes, ...sourceMeshes]) {
    const candidate = robeBaseSkeleton(mesh)
    if (
      candidate.bones.length !== skeleton.bones.length ||
      candidate.bones.some(
        (bone, i) =>
          bone.name !== skeleton.bones[i].name ||
          bone.parent?.name !== skeleton.bones[i].parent?.name ||
          !matrixMatches(candidate.boneInverses[i], skeleton.boneInverses[i])
      )
    ) {
      throw new Error('파츠의 본 순서·계층·기준 행렬이 몸체와 다릅니다.')
    }
  }
  const part = clone(source)
  part.updateMatrixWorld(true)
  const meshes = skinnedParts(part)
  const discarded = new Set([
    ...meshes.map((mesh) => mesh.skeleton),
    ...bodyMeshes
      .map(robeBaseSkeleton)
      .filter((candidate) => candidate !== skeleton),
  ])
  for (const mesh of bodyMeshes)
    if (mesh.skeleton === robeBaseSkeleton(mesh))
      mesh.bind(skeleton, mesh.bindMatrix.clone())
  const inverseRoot = part.matrixWorld.clone().invert()
  for (const mesh of meshes) {
    mesh.matrix.multiplyMatrices(inverseRoot, mesh.matrixWorld)
    mesh.matrix.decompose(mesh.position, mesh.quaternion, mesh.scale)
    for (const key of ['pelt_physics', 'robe_physics']) {
      for (let node: THREE.Object3D | null = mesh; node; node = node.parent) {
        if (node.userData[key]) {
          mesh.userData[key] = node.userData[key]
          break
        }
      }
    }
    body.add(mesh)
    mesh.bind(skeleton, mesh.bindMatrix.clone())
    mesh.castShadow = mesh.receiveShadow = true
  }
  for (const unused of discarded) unused.dispose()
  return meshes
}

export function parseModularHandProfile(value: unknown): ModularHandProfile {
  const profile = value as ModularHandProfile | null
  const finiteTuple = (values: unknown, size: number) =>
    Array.isArray(values) &&
    values.length === size &&
    values.every((v) => typeof v === 'number' && Number.isFinite(v))
  const unitQuaternion = (values: unknown): values is number[] =>
    finiteTuple(values, 4) &&
    Math.abs(Math.hypot(...(values as number[])) - 1) <= 1e-4
  if (
    !profile ||
    typeof profile.rig_id !== 'string' ||
    !profile.rig_id ||
    !finiteTuple(profile.iron_sword?.position, 3) ||
    !finiteTuple(profile.iron_sword?.quaternion, 4) ||
    !profile.finger_pose_sources ||
    typeof profile.finger_pose_sources !== 'object' ||
    Array.isArray(profile.finger_pose_sources)
  ) {
    throw new Error('손 보정 프로파일 형식이 올바르지 않습니다.')
  }
  const norm = Math.hypot(...profile.iron_sword.quaternion)
  if (Math.abs(norm - 1) > 1e-4)
    throw new Error('검 장착 회전이 정규화되지 않았습니다.')
  if (profile.iron_sword_by_clip !== undefined) {
    if (
      !profile.iron_sword_by_clip ||
      typeof profile.iron_sword_by_clip !== 'object' ||
      Array.isArray(profile.iron_sword_by_clip) ||
      Object.values(profile.iron_sword_by_clip).some(
        (pose) =>
          !pose ||
          !finiteTuple(pose.position, 3) ||
          !unitQuaternion(pose.quaternion)
      )
    ) {
      throw new Error('동작별 검 장착 설정이 올바르지 않습니다.')
    }
  }
  for (const pose of [
    profile.iron_sword,
    ...Object.values(profile.iron_sword_by_clip ?? {}),
  ]) {
    if (pose.keyframes === undefined) continue
    const frames = pose.keyframes
    if (
      !Array.isArray(frames) ||
      frames.length < 2 ||
      frames.some(
        (frame, i) =>
          !frame ||
          !Number.isFinite(frame.phase) ||
          frame.phase < 0 ||
          frame.phase > 1 ||
          (i > 0 && frame.phase <= frames[i - 1].phase) ||
          !finiteTuple(frame.position, 3) ||
          !unitQuaternion(frame.quaternion)
      ) ||
      frames[0].phase !== 0 ||
      frames[frames.length - 1].phase !== 1
    )
      throw new Error('검 장착 키프레임이 올바르지 않습니다.')
  }
  const finger = /^(Left|Right)Hand(Thumb|Index|Middle|Ring|Pinky)[123]$/
  if (
    profile.finger_poses !== undefined &&
    (!profile.finger_poses ||
      typeof profile.finger_poses !== 'object' ||
      Array.isArray(profile.finger_poses) ||
      Object.values(profile.finger_poses).some(
        (pose) =>
          !pose ||
          !Array.isArray(pose.clips) ||
          pose.clips.some((name) => typeof name !== 'string' || !name) ||
          !pose.quaternions ||
          typeof pose.quaternions !== 'object' ||
          Array.isArray(pose.quaternions) ||
          Object.entries(pose.quaternions).some(
            ([bone, quaternion]) =>
              !finger.test(bone) || !unitQuaternion(quaternion)
          )
      ))
  )
    throw new Error('손가락 자세 설정이 올바르지 않습니다.')
  for (const mapping of Object.values(profile.finger_pose_sources)) {
    if (
      !mapping ||
      typeof mapping !== 'object' ||
      Array.isArray(mapping) ||
      Object.entries(mapping).some(
        ([target, source]) =>
          !finger.test(target) ||
          typeof source !== 'string' ||
          !finger.test(source)
      )
    ) {
      throw new Error('손가락 회전 트랙 매핑이 올바르지 않습니다.')
    }
  }
  if (profile.finger_relaxation !== undefined) {
    if (
      !profile.finger_relaxation ||
      typeof profile.finger_relaxation !== 'object' ||
      Array.isArray(profile.finger_relaxation)
    ) {
      throw new Error('손가락 펴기 설정이 올바르지 않습니다.')
    }
    for (const mapping of Object.values(profile.finger_relaxation)) {
      if (!mapping || typeof mapping !== 'object' || Array.isArray(mapping)) {
        throw new Error('손가락 펴기 설정이 올바르지 않습니다.')
      }
      for (const [bone, adjustment] of Object.entries(mapping)) {
        if (
          !finger.test(bone) ||
          !adjustment ||
          !unitQuaternion(adjustment.rest_quaternion) ||
          !Number.isFinite(adjustment.amount) ||
          adjustment.amount < 0 ||
          adjustment.amount > 1
        ) {
          throw new Error('손가락 펴기 설정이 올바르지 않습니다.')
        }
      }
    }
  }
  return profile
}

export function applyModularFingerPose(
  clip: THREE.AnimationClip,
  profile: ModularHandProfile,
  rigId: string
): THREE.AnimationClip {
  if (profile.rig_id !== rigId)
    throw new Error('손 보정 프로파일의 리그가 다릅니다.')
  const mapping = profile.finger_pose_sources[clip.name]
  const relaxation = profile.finger_relaxation?.[clip.name]
  const poses = Object.values(profile.finger_poses ?? {}).filter((pose) =>
    pose.clips.includes(clip.name)
  )
  if (!mapping && !relaxation && poses.length === 0) return clip
  const adjusted = clip.clone()
  for (const [target, source] of Object.entries(mapping ?? {})) {
    const track = clip.tracks.find(
      (track) => track.name === `${source}.quaternion`
    )
    if (!(track instanceof THREE.QuaternionKeyframeTrack)) {
      throw new Error(`손가락 회전 트랙이 없습니다: ${clip.name}/${source}`)
    }
    adjusted.tracks = adjusted.tracks.filter(
      (track) => track.name !== `${target}.quaternion`
    )
    const replacement = track.clone()
    replacement.name = `${target}.quaternion`
    adjusted.tracks.push(replacement)
  }
  for (const [bone, adjustment] of Object.entries(relaxation ?? {})) {
    const track = adjusted.tracks.find(
      (track) => track.name === `${bone}.quaternion`
    )
    if (!(track instanceof THREE.QuaternionKeyframeTrack)) {
      throw new Error(`손가락 회전 트랙이 없습니다: ${clip.name}/${bone}`)
    }
    const rest = new THREE.Quaternion().fromArray(adjustment.rest_quaternion)
    const rotation = new THREE.Quaternion()
    for (let i = 0; i < track.values.length; i += 4) {
      rotation
        .fromArray(track.values, i)
        .slerp(rest, adjustment.amount)
        .normalize()
        .toArray(track.values, i)
    }
  }
  const pinned = new Map<string, number[]>(
    poses.flatMap((pose) =>
      Object.entries(pose.quaternions).map(([bone, quaternion]) => [
        `${bone}.quaternion`,
        quaternion,
      ])
    )
  )
  adjusted.tracks = adjusted.tracks.filter((track) => !pinned.has(track.name))
  for (const [name, quaternion] of pinned)
    adjusted.tracks.push(
      new THREE.QuaternionKeyframeTrack(
        name,
        [0, clip.duration],
        [...quaternion, ...quaternion]
      )
    )
  return adjusted
}

function swordPose(
  profile: ModularHandProfile,
  rigId: string,
  clipName?: string
): SwordPose {
  if (profile.rig_id !== rigId)
    throw new Error('검 장착 프로파일의 리그가 다릅니다.')
  return (
    (clipName && profile.iron_sword_by_clip?.[clipName]) || profile.iron_sword
  )
}

export function modularSwordTracks(
  profile: ModularHandProfile,
  rigId: string,
  clip: THREE.AnimationClip,
  targetName: string
): [THREE.VectorKeyframeTrack, THREE.QuaternionKeyframeTrack] {
  const pose = swordPose(profile, rigId, clip.name)
  const frames = pose.keyframes ?? [{ phase: 0, ...pose }]
  const times = frames.map((frame) => frame.phase * clip.duration)
  return [
    new THREE.VectorKeyframeTrack(
      `${targetName}.position`,
      times,
      frames.flatMap((frame) => frame.position)
    ),
    new THREE.QuaternionKeyframeTrack(
      `${targetName}.quaternion`,
      times,
      frames.flatMap((frame) => frame.quaternion)
    ),
  ]
}

export function poseModularSword(
  prop: THREE.Object3D,
  profile: ModularHandProfile,
  rigId: string,
  clipName?: string,
  phase = 0
): void {
  const pose = swordPose(profile, rigId, clipName)
  const frames = pose.keyframes
  if (!frames) {
    prop.position.fromArray(pose.position)
    prop.quaternion.fromArray(pose.quaternion)
    return
  }
  const at = THREE.MathUtils.clamp(phase, 0, 1)
  const index = Math.max(
    1,
    frames.findIndex((frame) => frame.phase >= at)
  )
  const previous = frames[index - 1],
    next = frames[index]
  const amount = (at - previous.phase) / (next.phase - previous.phase)
  prop.position
    .fromArray(previous.position)
    .lerp(new THREE.Vector3().fromArray(next.position), amount)
  prop.quaternion
    .fromArray(previous.quaternion)
    .slerp(new THREE.Quaternion().fromArray(next.quaternion), amount)
}
