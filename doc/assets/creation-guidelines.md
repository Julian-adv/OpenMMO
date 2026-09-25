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
- Character models: approximately **10,000 polygons**.
- Set these targets in Meshy or the generation tool being used.
- These are approximate targets, not exact caps. Do not decimate generated models solely to match the numbers.

## Mixamo Upload Preparation

- Prepare character uploads as a ZIP containing the original OBJ, an MTL, and the base-color texture at the archive root.
- Match the OBJ's `mtllib` to the MTL filename and its `usemtl` to the MTL's `newmtl` name. Set `map_Kd` to the exact base-color filename using a relative path; match filename case.
- Use a simple base-color-only material in the upload ZIP. Preserve the original GLB and all PBR textures separately for material restoration after rigging.
- Preserve the original geometry. Check ZIP integrity, included files, and material references before handing off the upload.
- Tobin's ZIP used `tobin.obj`, `model.mtl`, and `texture_0_base_color.png`. The user confirmed that its texture displayed correctly in Mixamo on 2026-09-21. Use this packaging method for future characters. See the [Tobin asset record](characters.md) and [file guide](../../assets/tobin/README.md).
