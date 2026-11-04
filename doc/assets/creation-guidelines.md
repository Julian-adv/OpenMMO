# Asset Creation Guidelines

These guidelines were confirmed by the user on 2026-09-20 and apply until the user reports a change.

## Modular Outfit Workflow

- For new modular outfits, follow [the reusable production workflow](modular-outfit-workflow.md)
  and [the connection rules](modular-outfit-connections.md).
- Keep generation, fitting, and runtime compatibility as separate review stages. Record failures
  and corrections alongside accepted assets; the rogue outfit is the first workflow example.

## Source and License Records

- When adding an asset, record its source and license in the matching `doc/assets/` document.
- For AI or paid tools, also record the subscription tier and generation date.
- Mark assets that are no longer used with **[미사용]** (unused).

## Hugging Face Uploads

- Store item models' Blender files, source models, textures, and generation records under `assets/items/<item-name>/`. Keep character fitting parts and shared animations in their existing directories.

- Exclude Blender `.blend1` backups from all HF uploads, including targeted uploads, and from `assets.lock` (user-confirmed 2026-10-09).
- Preserve the current `.blend` source file.

## Item Icon Framing

- Use a slightly elevated three-quarter view, following `client/public/items/armor/plate_armor.png` (Breastplate), rather than a straight front view (user-confirmed 2026-11-03).
- Render gloves and boots as a close side-by-side pair, aligned upright. Do not leave paired pieces spread apart in the character's fitting pose (user-confirmed 2026-11-03).
- Frame the item large and preserve the transparent 128×128 RGBA output.

## Image Generation Tier

- Record **ChatGPT Pro 20x** as the tier for ChatGPT/Codex ImageGen assets.
- Use the user-confirmed tier even when tool metadata omits it. Do not label it as undisclosed.

## Tripo API Authentication

- The Tripo API key is stored at **`~/.config/tripo/key`** (user-confirmed 2026-10-11).
- Read it locally and never print it or place it in prompts, source files, or generation records.
- Record request parameters and task IDs before polling; never automatically resubmit a paid task after an ambiguous response.

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

## Character Scale and Rigging Units

- Fit and rig characters in actual metres from the start (user-confirmed 2026-10-18).
- Use Blender Metric units with `scale_length = 1`; bake the intended size into mesh and bone coordinates before binding.
- Keep mesh and armature object scales at `(1, 1, 1)` and export at runtime scale `1`.
- Do not author a one-metre character and apply its intended height through a later game multiplier.
- The player reference is approximately **1.9m**. Barkan's bind-pose height is **2.5m**, measured from the soles to the highest horn/wing point.

## Human Concept Art: A/T Pose

- 사람 캐릭터의 리깅용 A/T 포즈 원화부터 **손바닥이 몸쪽을 향하도록** 그린다. 정면에 손바닥을 보여 주려고 하완·손목을 바깥으로 비틀지 않는다. T 포즈로 팔을 벌렸을 때는 손바닥이 아래를 향하는 자연스러운 방향을 유지한다.
- 손목은 하완과 자연스럽게 이어지고, 손가락은 곧게 펴서 서로 구분되게 한다. 정면·측면에서 손바닥 방향과 손가락 형태를 확인한 뒤 3D 생성과 Mixamo 리깅으로 넘긴다.
- 사용자 재확인: **2026-10-25**. 이졸데의 원화·모델은 손바닥이 앞을 향한 상태로 제작되어 기존 여성 강철 장검 그립을 옮길 때 손가락 방향을 따로 보정해야 했다. 이후 사람 원화 프롬프트와 결과 검수에 이 기준을 적용한다.

## Mixamo Upload Preparation

- For Mixamo characters, prepare the neutral A/T pose with palms facing down/toward the body, not forward (user-confirmed project requirement, 2026-09-28). Inspect finger separation from a side view instead of twisting the forearms to expose the palms in front views.
- If the hand rest pose changes, prepare a new rigging input and validate the returned rig with existing game animations before accepting it. The modular male uses the corrected palms-down/inward rig; its final files and source record are in [the asset guide](modular-human-male-01.md).
- Prepare character uploads as a ZIP containing the original OBJ, an MTL, and the base-color texture at the archive root.
- Match the OBJ's `mtllib` to the MTL filename and its `usemtl` to the MTL's `newmtl` name. Set `map_Kd` to the exact base-color filename using a relative path; match filename case.
- Use a simple base-color-only material in the upload ZIP. Preserve the original GLB and all PBR textures separately for material restoration after rigging.
- Preserve the original geometry. Check ZIP integrity, included files, and material references before handing off the upload.
- Tobin's ZIP used `tobin.obj`, `model.mtl`, and `texture_0_base_color.png`. The user confirmed that its texture displayed correctly in Mixamo on 2026-09-21. Use this packaging method for future characters. See the [Tobin asset record](characters.md#tobin--토빈--강가의-낚시꾼-2026-09-21).
