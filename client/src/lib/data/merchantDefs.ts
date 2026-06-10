import merchantsJson from '../../../../data/merchants.json'

export interface MerchantDefinition {
  id: string
  npcName: string
  sellRatePercent: number
  /** Semicolon-separated item def ids. */
  catalog: string
}

const merchantDefs = merchantsJson as Record<string, MerchantDefinition>

const byNpcName = new Map(
  Object.values(merchantDefs).map((def) => [def.npcName, def])
)

/** Merchant lookup by NPC character name (NPCs are agent-controlled players).
 *  TODO(Phase 3): replace name-keyed lookup with server-sent NPC capabilities
 *  (doc/ECONOMY.md "거래 진입 UI") so non-merchant NPCs can trade too. */
export function getMerchantByNpcName(
  npcName: string
): MerchantDefinition | undefined {
  return byNpcName.get(npcName)
}

export default merchantDefs
