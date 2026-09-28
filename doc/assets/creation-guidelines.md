# Asset Creation Guidelines

These guidelines were confirmed by the user on 2026-09-20 and apply until the user reports a change.

## Source and License Records

- When adding an asset, record its source and license in the matching `doc/assets/` document.
- For AI or paid tools, also record the subscription tier and generation date.
- Mark assets that are no longer used with **[미사용]** (unused).

## Image Generation Tier

- Record **ChatGPT Pro 20x** as the tier for ChatGPT/Codex ImageGen assets.
- Use the user-confirmed tier even when tool metadata omits it. Do not label it as undisclosed.

## Meshy API Authentication

- The Meshy API key is stored at **`~/.config/meshy/key`** (user-confirmed 2026-09-21).
- Read the key from that file when using the Meshy API. Do not print the key or copy it into prompts, source files, generation records, or documentation.

## 3D Model Polygon Targets

- Item models: approximately **4,000 polygons**.
- Non-modular character models: approximately **10,000 polygons**.
- Modular customizable characters: approximately **15,000–20,000 polygons across all assembled parts**,
  with more geometry allocated to the face (user-confirmed 2026-09-28).
  Count one equipped character, including attached weapons and procedural capes.
  Sum triangles across exported GLBs and procedural geometry; record both the assembled total
  including hidden regions and the visible count, plus the face allocation separately.
  See [Character Customization](../CHARACTER_CUSTOMIZATION.md).
- Preserve facial detail in topology, UV allocation, and final compressed textures.
  Review the exported face in the close-up character preview.
- Set these targets in Meshy or the generation tool being used. For modular characters, divide
  the total budget among parts; do not apply the full character target or the general item target to every part.
- These are approximate targets, not exact caps. Do not decimate generated models solely to match the numbers.

## Mixamo Upload Preparation

- For Mixamo characters, prepare the neutral A/T pose with palms facing down/toward the body, not forward (user-confirmed project requirement, 2026-09-28). Inspect finger separation from a side view instead of twisting the forearms to expose the palms in front views.
- If the hand rest pose changes, prepare a new rigging input and validate the returned rig with existing game animations before accepting it. The modular male's previous forward-palm rig is retained only for comparison; the corrected upload is documented in [the palms-down sample](../../assets/modular_human_male_01/refined_palms_down/README.md).
- Prepare character uploads as a ZIP containing the original OBJ, an MTL, and the base-color texture at the archive root.
- Match the OBJ's `mtllib` to the MTL filename and its `usemtl` to the MTL's `newmtl` name. Set `map_Kd` to the exact base-color filename using a relative path; match filename case.
- Use a simple base-color-only material in the upload ZIP. Preserve the original GLB and all PBR textures separately for material restoration after rigging.
- Preserve the original geometry. Check ZIP integrity, included files, and material references before handing off the upload.
- Tobin's ZIP used `tobin.obj`, `model.mtl`, and `texture_0_base_color.png`. The user confirmed that its texture displayed correctly in Mixamo on 2026-09-21. Use this packaging method for future characters. See the [Tobin asset record](characters.md) and [file guide](../../assets/tobin/README.md).
