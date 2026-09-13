# Bow target mark VFX preview

- Page: `/bow-mark-preview.html` on the local client development server.
- Scope: standalone presentation; gameplay uses the same VFX class. See [True Aim](../TRUE_AIM.md) for the implemented ability.
- Requested concept: Bow-only ability that marks a chosen target for 5 seconds with a 10-second cooldown; only the caster's attacks against that target are guaranteed to hit. Party members and other attackers receive no benefit. The mark VFX is visible only to the caster. Costs and acquisition remain undecided for gameplay implementation.
- Default color: pale red (`#ed9984`), selected by the user. The preview applies this palette on initial load as well as on selection changes.
- Four fine arc segments and cardinal sight ticks converge above the target's head in 0.42 seconds. A brief local glow acknowledges the lock; the center diamond and reticle then remain gently lit. No expanding area wave, projectile, damage flash, or attack demonstration.
- The mark faces the camera and follows the selected target. Orc and Goblin demonstrate different target heights; the unselected monster has no mark. Select a target through the dropdown or by clicking the model.
- Default mark size is 75% of the first preview; the head clearance scales with it. After the acquisition accent, the whole mark smoothly grows and shrinks by ±10% around that size on a 1.2-second cycle, eased in over 0.25 seconds.
- The preview has a 0.42-second acquisition, a 5-second marked interval, and a 0.4-second visual fade, followed by a quiet replay interval. The agreed gameplay effect lasts 5 seconds; the extra acquisition/fade frames are presentation only.
- Controls: cast/replay, pause, scrub, expiry, target, camera, pale gold/silver/red palette, scale, moving targets, 0.25x playback, loop, and caster view. Disable caster view to preview the mark being hidden from other players. Reduced-motion preference starts on a paused held mark.
- Game integration keys the mark by caster and target and delivers mark state privately to its caster. This preview itself has no server connection.

## Sources (2026-09-13)

- Reticle geometry and soft radial glow texture: created procedurally in `client/src/lib/effects/bow-mark.ts` with Three.js/Canvas, re-exported by `client/src/previews/bow-mark-effect.ts`. Authored in this repository with Codex under the repository's code license. No external VFX asset. The separate game icon is recorded in [ui.md](ui.md).
- Ranger character and Bow: existing local GLBs, with the existing character retargeting, grounding, forearm measurement and hand-prop helpers. Original sources and terms: [characters.md](characters.md), [items.md](items.md).
- Existing `combat_ranged.glb` / `bow_shoot` is sampled only for the targeting stance, and `combat_melee.glb` / `combat_idle` for rest. No arrow is fired. See [animation.md](animation.md).
- Orc and Goblin: existing GLBs and their embedded `Idle_Loop_Rig` / `Walk_Loop_Rig` clips. Preview display heights are 2.1 m / 1.65 m. See [monsters.md](monsters.md).
- Ground, river rocks and crates: reused local `cobblestone_color_1k.glb`, `river_rock_01.glb` and `crate.glb`. The cobblestone material is tinted darker for the preview. See [environment.md](environment.md), [props.md](props.md).
- Existing terrain and assets were reused; `assets.lock` was unchanged during feature setup.
