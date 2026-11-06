import monstersJson from '../../../../data/monsters.json'

export interface MonsterDefinition {
  id: string
  name: string
  model: string
  health?: number
  level: number
  guard: number
  attackBonus?: number
  walkSpeed: number
  runSpeed: number
  attackRange: number
  chaseRange: number
  attackCooldown: number
  attackImpactDelay: number
  attackDamageTextDelay: number
  behavior: string
  damageRoll?: string
  animIdle: string
  animWalk: string
  animRun: string
  /** One clip, or several `|`-separated to pick from at random per swing. */
  animAttack: string
  /** Clips for targets on the monster's left; `animAttack` then covers the right. */
  animAttackLeft?: string
  animAttackIdle?: string
  /** Empty for monsters on the shared packs — those have no hit reaction. */
  animHit?: string
  animDie: string
  animDead: string
  /**
   * Extra world metres (independent of `scale`) applied after the corpse is
   * auto-grounded on its lowest vertex; negative sinks it. Only needed when a
   * dangling appendage pegs the offset and leaves the body hovering, as the
   * kobold's tail does.
   */
  corpseGroundOffset?: number
  /**
   * When false, skip the corpse auto-grounding entirely: the death clip is
   * authored to end at ground level (stone_golem).
   */
  corpseAutoGround?: boolean
  /**
   * World-metre footprint radius for the hover ring and proxy, overriding the
   * bind-pose box. Needed when the bind pose spreads the arms and the box
   * overstates the body (stone_golem).
   */
  hoverRadius?: number
  /** Lights the model's blue-violet crystal like the mine's clusters
   *  (`applyCrystalGlow`). */
  crystalGlow?: boolean
  /** Solid body as `halfWidth|front|back` meters; fixes the facing and moves
   *  reach to the body surface (shared `monster_body`). */
  footprint?: string
  material?: string
  /**
   * When true (default), a killing blow plays the hit reaction before the death
   * clip. Set false for monsters whose hit clip looks awkward as a death lead-in
   * (e.g. scp939's long additive stagger).
   */
  deathPlaysHit?: boolean
  /** Raid boss: landed hits neither stagger it nor play its flinch. */
  staggerImmune?: boolean
  /**
   * Cross-fade seconds into the death clip (default 0.2). Raise for monsters
   * whose death pose is a static clip the body should settle into slowly
   * (e.g. scp939's 939_Sleeping).
   */
  deathFadeSeconds?: number
  /** Visual scale multiplier (default 1). Purely cosmetic — server-side
   * ranges and collision are unaffected. */
  scale?: number
  /** Dungeon boss: shows a nameplate on the client. */
  boss?: boolean
  /** Healing Prayer harms it instead of healing. */
  undead?: boolean
  /** Optional weapon item id, or legacy model path relative to /models/. */
  weapon?: string
  /** Chance from 0-1 that the weapon is dropped on death. */
  weaponDropChance?: number
  /** Skeleton bone name the weapon is parented to (e.g. 'RightHand'). */
  weaponBone?: string
  /** Metres along the weapon bone's local +Y — wrist toward fingers. The hand
   * bone sits at the wrist, so without it the weapon hangs off the wrist.
   * Aim for the knuckle line, as the player's own 0.08 does; measure where the
   * hand's finger bases sit in bone space rather than trusting the finger
   * joint, which auto-rigs misplace (the ogre's is 0.24). */
  weaponOffset?: number
  /** Sideways and forward nudge along the weapon bone's local X and Z, in
   * metres. A palm is not on the bone axis, so a grip that reads right from the
   * front can still float beside the hand from above. */
  weaponOffsetX?: number
  weaponOffsetZ?: number
  /** Grip rotation as `rx|ry|rz` in degrees, about the weapon bone's local
   * axes. Weapons are modelled pointing +Y; a rig whose hand bone runs a
   * different way needs turning to sit in the fist. */
  weaponRotation?: string | number
  /** Death cry, played on the killing blow (path under /sounds). */
  deathSound?: string
  /** The blow landing, `attackImpactDelay` into each swing (path under
   * /sounds); it replaces the miss whoosh. */
  attackSound?: string
  /** The ground bursting under a ranged strike's mark. */
  rangedAttackSound?: string
  /** A field boss shaking off its stand-in, and sinking back under it. */
  wakeSound?: string
  sleepSound?: string
  /** Set on a stone boss whose fallen body bursts into rubble and sinks
   * away; the sound of the burst. */
  crumbleSound?: string
  /** Play the shared character packs; only for models rigged on the character
   * skeleton, which then ship no clips of their own. */
  sharedAnims?: boolean
}

const DEGREES_TO_RADIANS = Math.PI / 180

/**
 * Read a `rx|ry|rz` grip rotation into radians.
 *
 * A single number reaches this as a number rather than a string, because the
 * CSV converter coerces anything numeric — so `90` means 90 degrees about x.
 */
export function parseWeaponRotation(
  value: string | number | undefined
): [number, number, number] {
  if (value === undefined || value === '') return [0, 0, 0]

  const parts = String(value)
    .split('|')
    .map((part) => Number(part.trim()))
  const angles: [number, number, number] = [0, 0, 0]
  for (let axis = 0; axis < 3; axis++) {
    const angle = parts[axis]
    angles[axis] = Number.isFinite(angle) ? angle * DEGREES_TO_RADIANS : 0
  }
  return angles
}

const monsterDefs = monstersJson as Record<string, MonsterDefinition>

export function getMonsterDef(type: string): MonsterDefinition | undefined {
  return monsterDefs[type]
}

export function splitClipNames(value: string | undefined): string[] {
  return value ? value.split('|').filter(Boolean) : []
}

export function attackClipNames(
  def: MonsterDefinition | undefined,
  fromLeft = false
): string[] {
  const left = fromLeft ? splitClipNames(def?.animAttackLeft) : []
  if (left.length > 0) return left
  const names = splitClipNames(def?.animAttack)
  return names.length > 0 ? names : ['Attack']
}

export default monsterDefs
