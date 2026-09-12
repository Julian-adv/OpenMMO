# Equipment enchantment success VFX

- File: `client/public/textures/vfx/enchant-filament.png`.
- Source: OpenAI built-in image generation, 2026-09-12 KST. Paid/account tier is not exposed by the tool.
- License: AI-generated output under applicable OpenAI service terms; no separate third-party asset license supplied. Not designated CC0.
- Original generated PNG copied unchanged with alpha preserved.
- Runtime: the server sends one `EquipmentEnchantSucceeded` event (player ID and weapon/armor flag) on successful enchantment to nearby players on the same floor, including the owner. Uses the existing spatial event radius and direct fanout. No per-frame updates or persistent effect state.
- Presentation: 0.3 seconds gathering and 0.3 seconds dispersing. Local and remote weapons follow the wielding hand bone; armor follows the torso bone. Anchors include riding and interaction poses. Effects share the character layer's house and floor visibility rules.
- All known nearby players can display effects; there is no seven-player cap. Pending events coalesce only repeated events for the same player. Eight effects are prewarmed, and the reusable pool grows when needed.
- Detail: 20 motes and three filaments normally; eight motes and one filament beyond 15m or when more than eight effects are active. Offscreen remote effects skip drawing. Remote effects have no scene light. Only one permanent local light changes intensity; shaders compile during loading.
- Texture: PNG is tracked in Git, consistent with existing PNG textures; it is not in the HF-managed GLB/audio/source-asset set.
- Preview: run the client Vite development server and open `/enchant-vfx-preview.html` for a side-by-side comparison at real speed or 4x slow motion. Uses the same runtime effect class.
- Texture animation: gentle UV distortion, dissolving edges, and eased fading continue through the release phase.
- GIF previews: `doc/assets/enchant-success-preview/{armor,weapon}-{full,reduced}.gif`, rendered from the preview on 2026-09-12 KST using the existing Valkyrie and locomotion assets. These are repository-rendered derivatives, retain the underlying assets' license terms, and have no separate AI generation or paid-tool tier. Each GIF preserves the default game-scale character at a 912px viewport height, with 0.6 seconds of effect and 0.5 seconds between loops. They are documentation assets tracked in Git, not HF-managed game binaries.

## Generation prompt

Use case: stylized-concept. Asset type: a single reusable game VFX particle texture on genuine transparent alpha background, square. Subject: a small delicate tuft of luminous silk-like filaments curling loosely upward, interspersed with a few very fine dust motes, with a subtle soft luminous knot at its base. For a gentle non-combat LIGHT spell in a grounded medieval fantasy forest RPG. Restrained warm ivory, pale straw gold, desaturated antique gold at the faint edges. Fine smoky translucent fibers with natural irregular spacing, readable soft silhouette, mostly transparent negative space. Centered in middle 65 percent of canvas, all edges fully transparent, no clipping. Hand-painted semi-realistic texture, no neon yellow, no orange flames, no large white blown highlights, no star-shaped sparkles, no lens flare, no circular glyphs, no characters or props, no text, no checkerboard. This is a small particle billboard asset, not a complete effect illustration. Preserve actual alpha channel.
