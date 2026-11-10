import { derived, writable } from 'svelte/store'
import { pickupKeyCode } from './pickupSettings'
import type { CharacterClass } from '../network/networkTypes'

// Limits, ranks and permission bits mirror shared/src/family.rs.
export const FAMILY_MAX_MEMBERS = 30
export const FAMILY_CREATE_COST = 10_000
export const FAMILY_RENAME_COST = FAMILY_CREATE_COST
export const FAMILY_REJOIN_COOLDOWN_HOURS = 24
export const FAMILY_RANK_LABEL_MAX_CHARS = 12
export const FAMILY_INVITE_TTL_MS = 120_000
export const MAX_PENDING_FAMILY_INVITES = 5

export const FAMILY_RANK_HEAD = 0
export const FAMILY_RANK_GUEST = 3

export const FAMILY_PERMISSION = {
  INVITE: 1 << 0,
  KICK: 1 << 1,
  BUILD_HOUSE: 1 << 2,
  DEMOLISH_HOUSE: 1 << 3,
  PLACE_FURNITURE: 1 << 4,
  LANDSCAPE: 1 << 5,
  TREASURY_DEPOSIT: 1 << 10,
  EDIT_RANKS: 1 << 12,
} as const

export type FamilyPermissionKey = keyof typeof FAMILY_PERMISSION

/** Bits the rank editor lists; the rest do nothing until later phases. */
export const FAMILY_EDITABLE_PERMISSIONS = Object.keys(
  FAMILY_PERMISSION
) as FamilyPermissionKey[]

/** Server-stored default labels, by rank; shown localized instead. */
export const FAMILY_DEFAULT_LABELS = ['Head', 'Elder', 'Family', 'Guest']
const DEFAULT_LABEL_KEYS = [
  'family.rank.head',
  'family.rank.elder',
  'family.rank.member',
  'family.rank.guest',
] as const

export function familyDefaultLabelKey(
  label: string
): (typeof DEFAULT_LABEL_KEYS)[number] | undefined {
  return DEFAULT_LABEL_KEYS[FAMILY_DEFAULT_LABELS.indexOf(label)]
}

/** Same character set as character names (server/src/auth.rs). */
const NAME_CHARS = /^[A-Za-z0-9_가-힣ㄱ-ㅣぁ-ゖァ-ヺー一-鿿]+$/

export function validFamilyRankLabel(label: string): boolean {
  return (
    NAME_CHARS.test(label) && [...label].length <= FAMILY_RANK_LABEL_MAX_CHARS
  )
}

export interface FamilyRank {
  label: string
  permissions: number
}

export interface FamilyMember {
  character_id: number
  name: string
  level: number
  class: CharacterClass
  rank: number
  online: boolean
}

export interface FamilyEstate {
  estate_id: number
  owner_name: string
  plots: number
  missed: number
  mine: boolean
}

export interface FamilyInfo {
  name: string
  ranks: FamilyRank[]
  /** Succession order, as sent. */
  members: FamilyMember[]
  estates: FamilyEstate[]
  my_rank: number
  can_attach_estate: boolean
  can_claim_head: boolean
  /** Unix seconds; 0 = renaming is open. */
  rename_at: number
}

export interface PendingFamilyInvite {
  inviterId: number
  inviterName: string
  familyName: string
  offeredAt: number
}

/** null = no family. */
export const familyInfo = writable<FamilyInfo | null>(null)
export const pendingFamilyInvites = writable<PendingFamilyInvite[]>([])
export const familyPanelVisible = writable(false)

/** H opens the family panel unless the player bound pickup to H. */
export const familyHotkeyActive = derived(
  pickupKeyCode,
  (code) => code !== 'KeyH'
)

export function applyFamilyState(family: FamilyInfo | null) {
  familyInfo.set(
    family && {
      ...family,
      rename_at: Number(family.rename_at),
      members: family.members.map((member) => ({
        ...member,
        character_id: Number(member.character_id),
      })),
      estates: family.estates.map((estate) => ({
        ...estate,
        estate_id: Number(estate.estate_id),
      })),
    }
  )
  if (family) pendingFamilyInvites.set([])
}

export function resetFamilyStores() {
  familyInfo.set(null)
  pendingFamilyInvites.set([])
  familyPanelVisible.set(false)
}

export function familyPermissions(family: FamilyInfo): number {
  return family.ranks[family.my_rank]?.permissions ?? 0
}
