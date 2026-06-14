# Devlog

Development notes for notable project changes, asset additions, and design decisions.

## Entries

| Date | Topics | Notes |
|------|--------|-------|
| [2026-06-14](2026-06-14.md) | Procedural Dungeons | Added seed-deterministic, NetHack-style maze dungeons (fixed per entrance, up to 20 floors). Surface entrances reuse the housing procedural geometry as a stone building, with terrain holed out to expose the stairs. The final floor holds a boss with loot. |
| [2026-06-11](2026-06-11.md) | Economy Phase 2: LLM Haggling | Rica's LLM can now grant per-player price deals via the new `offer_deal` action; the server clamps offers to a CHA-derived price band, enforces daily budgets and cooldowns, logs every decision, and the trade window shows haggled prices. |
| [2026-06-10](2026-06-10.md) | Goblin, Player Control FSM, Economy Phase 1 | Added the goblin monster, converted player control to an explicit FSM, and implemented single-currency gold plus fixed-price merchant trading with Rica (basePrice data, trade protocol, server validation, trade window UI). |
| [2026-06-08](2026-06-08.md) | Kobold, Small Sword | Added the kobold monster, documented the 2D -> 3D asset workflow, adjusted the kobold scale, and added the small sword weapon. |
