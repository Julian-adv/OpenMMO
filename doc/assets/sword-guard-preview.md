# Sword / Mace party guard VFX preview

- Preview: `/sword-buff-preview.html` on the client Vite development server.
- The standalone study now shares its VFX implementation with the game. Gameplay and timing rules are documented in [GUARDIAN_WARD.md](../GUARDIAN_WARD.md).
- Requested concept: main-hand weapon with `weaponType` exactly `sword` or `mace`, plus an equipped shield in `off_hand`; +10% defense for party members within range. Goblin Sword and Small Sword now belong to `sword`; `dagger` and `great_sword` remain separate. The preview character currently demonstrates Sword + Shield.
- Game configuration: caster included, 20 m radius, 60 s duration, 45 s cooldown. The preview keeps an adjustable 3–7 m staging radius so the characters and crest details fit the close camera; it is not the gameplay range. The cast effect lasts one second; the preview repeats every 2.5 seconds, including a quiet interval.
- Sequence: all affected characters receive a shield crest at chest height immediately. No expanding wave or distance-based arrival delay. The crest stays at chest height, starts fading at 0.35 seconds, and disappears with the remaining glow at one second. No overhead mark or upward crest movement. An out-of-range party member receives no effect. Party state in the preview is local illustration data only.
- Palette: pale gold (#f2dfa9) in the game; pale gold or silver-white selectable in the preview. Range guide can be hidden independently of VFX.
- Shield arrival accent: a 40 ms scale punch and a smaller second beat at 150 ms, settling by 270 ms. Two short flashes brighten the outline, shield fill, and soft central glow, with a faint shield-outline echo. No radial light streaks or outward-flying fragments. Timing follows the [Guardian Ward sound](sfx.md#abilities): strong onset, a second emphasis around 140 ms, and a quick decay after about 400 ms. The source audio is unchanged.
- At 1× speed, pressing the preview's cast button plays the sound through the shared SFX manager. Automatic loops, slow motion, and timeline scrubbing remain visual so they do not repeatedly or incorrectly trigger the sound.

## Equipment validation notes

- Equipment is fixed in the standalone preview. The game validates both equipped item types on the server.
- Item classification is implemented in local commit `d24b0b16`, included in this buff branch. `wooden_shield` and `raven_shield` have `category=armor`, `equipSlot=off_hand`, and `armorType=shield`. `torch` and `worn_torch` also occupy `off_hand`, but have `category=weapon`, `weaponType=torch`, and no `armorType`.
- Skill equipment validation uses the explicit `armorType=shield` classification of the actually equipped off-hand item. Item classification will be submitted together with the completed buff; no separate PR has been published.
- The server resolves equipped item instances to definitions; the client mirrors these conditions for quickslot availability.

## Sources (2026-09-13)

- VFX geometry and radial sprite texture are created procedurally in `client/src/lib/effects/sword-guard.ts`. Authored in this repository with Codex; no new image-generation service, external bitmap, paid asset purchase, or downloaded VFX asset. Follows the repository's code licensing; no new third-party license or generation tier applies. The earlier procedural expanding ring and its feathered texture are no longer used.
- Reused models: existing Knight, Rogue, Priest, Sword, and Raven Shield GLBs. See [characters.md](characters.md), [items.md](items.md), and [props.md](props.md) for their original sources and license terms.
- Reused animation: existing `combat_melee.glb` / `combat_idle`, through the existing retargeting, grounding, and hand-prop placement helpers. See [animation.md](animation.md). No new or edited animation asset.
- HF assets and `assets.lock` are unchanged. Preview files do not alter the normal game entry point.
