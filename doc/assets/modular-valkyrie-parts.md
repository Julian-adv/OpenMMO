# 발키리 모듈 복장

2026-11-01. 기존 발키리 원화를 바탕으로 만든 복장 정면 시안과 날개 피팅 후보의 기록이다. 복장의 3D 제작과 날개의 게임 적용은 진행하지 않았다.

![발키리 복장 정면 시안](../../assets/items/valkyrie_starter/concepts/front-v1.png)

- 참조: `client/public/character_concepts/valkyrie.webp`. 출처는 [캐릭터 기록](characters.md)을 따른다.
- 시안: `assets/items/valkyrie_starter/concepts/front-v1.png`, 1024×1536 RGB PNG.
- 제작: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-11-01. OpenAI 생성 출력물 이용 조건 및 참조 에셋의 기존 이용 조건을 따른다.
- 참조 SHA-256: `dbbbd830209b0db4990424dbe0ad3021ed369d42a4f61faf31b20df0e38aabda`.
- 시안 SHA-256: `6dd4b9621682a602081c0254f9828c5450fdb55ffe15246723efd6789d2e12fb`.

은색 갑옷, 금색 장식, 견갑, 팔 보호대, 허리 판금과 긴 부츠를 참조했다. 복장의 형태를 보기 위해 시안에서 큰 등 날개를 숨겼다. 사용자는 발키리에 날개가 필요하다고 정했다. 팔의 검은 무늬와 인물의 얼굴·머리카락은 복장 파츠로 확정하지 않았다. 손가락과 손바닥 방향, 가슴과 허리 연결, 뒤쪽 구조는 실제 몸체 및 후속 부위 원화와 대조해야 한다.

## 제작 기준과 남은 작업

- 여성 공통 몸체: `assets/modular_human_female_01/rig_v1/female-base.glb`, 리그 `human_female_01_local_v1`, 약 1.8m. 피팅에 사용한 몸체의 SHA-256은 `3eb7c0a5f3c6b30aec0fbae0b70c8fd925e4cb91772647420007eee795a1e0a5`다.
- 상의·하의·손목 보호대·부츠·머리 장식의 슬롯 소속과 안팎 순서는 미정이다. 견갑은 상의에 포함한다. 허리 판금의 소속, 가림과 속옷 노출을 정해야 한다.
- 날개, 헤어 및 머리 장식의 착탈 관계와 다른 장비와의 호환은 미정이다.
- 여성 몸체의 실제 연결부를 확인한다. 남성용 테두리와 수치를 그대로 사용하지 않는다.
- 앞·옆·뒤와 파츠별 원화를 검토한 뒤 생성, 피팅 및 게임 동작을 각각 검증한다. 동작과 혼합 장비 검증 전에는 런타임 호환을 선언하지 않는다.
- 여성 기사 세트는 기존에 삼각형 예산을 초과한다. 발키리는 몸체·얼굴·헤어·무기와 선택한 부속을 포함한 전체 예산을 먼저 배분하고 실제 표시 수와 숨김 포함 수를 기록한다.

## 생성 프롬프트

```text
Use case: stylized-concept. Asset type: front-view outfit design reference for modular 3D game character. Input image is the existing OpenMMO Valkyrie and is the authoritative design reference. Create one full-body, straight-on front view of the same Valkyrie costume, clearly showing the entire costume from head to toes. Preserve the reference design: silvery sculpted breastplate with subtle gold central ornament, layered shoulder plates, forearm bracers, silver armored hip panels and central tabard, bare upper arms and thighs, silver knee-high armored boots with knee guards, white-blonde long hair, and small winged head ornaments. Do not invent new clothing or change the color palette. Natural relaxed A-pose suitable for the existing modular rig, hands slightly away from hips with palms facing inward toward body and fingers straight and individually separated, no twisting wrists to face camera. Keep legs comfortably separated with feet forward. For clarity, remove the large back wings from this garment reference only; do not add a weapon. Neutral light gray plain background, even studio lighting and realistic fantasy game concept rendering. Make armor contours, attachments and skin openings clearly legible for later part design. No text, labels, collage, inset, side view, watermark, or cropped limbs. This is a preliminary visual concept, not a fitted 3D model.
```

## 날개 원화 — 2026-11-01

발키리의 날개를 복장과 별도로 제작하기 위한 후면 원화다. 기존 원화의 흰 깃털 날개를 기준으로 한 쌍을 펼친 모습을 그렸다. 게임 내 장비 슬롯과 독립적인 날개 애니메이션은 아직 정하지 않았다.

![날개 후면 원화](../../assets/items/valkyrie_starter/concepts/wings-back-v1.png)

- 이미지: `assets/items/valkyrie_starter/concepts/wings-back-v1.png`, 1536×1024 RGBA PNG, SHA-256 `5300828da0981cd7a65ca737a3a22d910a95e0a83be58cf51afb528e903202ef`.
- 참조: 위의 기존 발키리 원화. 생성: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-11-01. OpenAI 생성 출력물 이용 조건 및 참조 에셋의 기존 이용 조건을 따른다.
- 투명 알파 범위는 0–254다. 날개 주변에 반투명한 밝은 영역이 있어 이 이미지를 그대로 3D 텍스처로 쓰기 전 별도 정리가 필요하다.
- 양쪽 날개 안쪽이 가까이 모여 있지만 실제 등 부착점과 관절 구조를 보여 주는 제작 도면은 아니다. 몸체와 갑옷·머리카락, 날개 접힘과 동작 중 충돌은 후속 검토 사항이다.

### 날개 생성 프롬프트

```text
Use case: stylized-concept. Asset type: isolated wing-pair concept art for a modular 3D fantasy game character. Image 1 is the existing OpenMMO Valkyrie; use her large white feathered wings as the authoritative design reference. Depict ONLY one complete matching pair of large angelic wings as a single wearable back attachment, seen straight from behind, symmetrically spread upward and outward in the same broad, graceful silhouette as the reference. Bright natural white feathers with subtle warm ivory and pale gray shading, layered small coverts and individually readable broad flight feathers, polished realistic fantasy game style. Inner wing roots converge naturally at the center to show a small plausible attachment area for a character's upper back. Keep feathers substantial and connected, with a clear hierarchy and no detached pieces, so the shape can guide subsequent 3D modeling. No body, human, mannequin, head, hair, armor, harness, weapon, halo, text, labels, multiple views, inset, ground or shadow. Frame the entire pair with generous margins, front-facing camera aligned to the back of the wings, no perspective or cropping. Soft neutral lighting; truly transparent background and transparent spaces between feathers, no checkerboard.
```

## 한쪽 날개 원화 — 2026-11-01

양쪽 날개 원화의 이미지 오른쪽 날개를 기준으로 한쪽만 다시 그렸다. Tripo에서 한쪽을 생성한 뒤 반대쪽을 대칭 복제하는 작업의 후보 원화다. 제작 목표는 한쪽 약 1,500삼각형이었다.

![한쪽 날개 후면 원화](../../assets/items/valkyrie_starter/concepts/wing-right-back-v1.png)

- 이미지: `assets/items/valkyrie_starter/concepts/wing-right-back-v1.png`, 1536×1024 RGBA PNG, SHA-256 `b515b137b1ab6e892cb2cc9b5e6c40bbf48e4f4b877a98496fc44fd389419cdb`.
- 참조: 위의 `wings-back-v1.png`. 제작: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-11-01. OpenAI 생성 출력물 이용 조건 및 참조 에셋의 기존 이용 조건을 따른다.
- 알파 범위는 0–254이며 모서리는 투명하다. 날개 주변에 반투명한 밝은 영역이 남아 있으므로 Tripo 입력에서 배경으로 해석되는지 확인해야 한다. 사용자 제공 Tripo 결과를 아래에 기록했다.

### 한쪽 날개 생성 프롬프트

```text
Use case: precise-object-edit. Asset type: one isolated wing concept for image-to-3D reconstruction in a fantasy game. Input image: Valkyrie wing-pair reference, seen from behind. Depict exactly ONE wing: the wing on the RIGHT side of the reference image, with its root at the lower LEFT and its long outer feathers rising and spreading to the upper RIGHT. Preserve that wing's broad silhouette, size proportions, natural white and ivory color, curved leading edge, dense overlapping coverts and long broad flight feathers. It is a single connected physical feathered wing with a compact clear inner root for attaching to a character's back; reconstruct its root cleanly where the other wing is removed. Entire wing and all feather tips visible with generous transparent margins, centered in a landscape or square frame, straight rear view, no perspective distortion. Maintain restrained realistic fantasy game concept lighting and crisp feather edges; no glow, aura, cast shadow or gradient. Remove the other wing completely. No body, mannequin, armor, harness, extra wings, text, labels, or background. Truly transparent RGBA background including clear empty spaces between separated flight feathers; no checkerboard.
```

## Tripo 원본과 날개 피팅 — 2026-11-01

사용자가 Tripo에서 생성한 `angel wing 3d model.glb`를 제공했다. [원본 GLB](../../assets/items/valkyrie_starter/tripo_wing_v1/source.glb)의 SHA-256은 `1c4068664f8849c77bd6e1d3f39a06d745d1415e1337781013aade6c90fc8839`이다. 생성 설정·작업 ID·비용·요금제·이용 조건과 실제 입력 이미지는 미확인이다. 원본은 한쪽 2,629삼각형이며 리그가 없다. 1,500개는 제작 목표였고 실제 Tripo 설정값은 확인되지 않았다.

[여성 공통 몸체](modular-human-female-01.md)의 `human_female_01_local_v1` 리그와 [긴 헤어](modular-female-knight-hair.md)를 사용했다. 미러 복제 후 사용자의 요청에 따라 폭을 원래의 60%로 줄였다. 현재 전체 폭은 약 1.445m다. 각 날개는 `Spine2`에 100%로 고정되며 독립적인 접기·펄럭임 동작은 없다.

- [수정 원본 Blender](../../assets/items/valkyrie_starter/tripo_wing_v1/source-patch-v1.blend)와 [수정 원본 GLB](../../assets/items/valkyrie_starter/tripo_wing_v1/source-patch-v1.glb) (`d1320d7d36c3f8dac6a5d2aac95fc472de7fe56e6cc76164ba39a6c9d33ce891`).
- [피팅 Blender 장면](../../assets/items/valkyrie_starter/tripo_wing_v1/fitted_patch_v1/wings-final.blend): 몸체·긴 헤어·노멀 보정된 날개. 몸체와 헤어는 각 원본 기록의 출처와 이용 조건을 따른다.
- [미리보기 GLB](../../assets/items/valkyrie_starter/tripo_wing_v1/fitted_patch_v1/wings-front-normals-preview.glb) (`a56bb925f7cc1edca6bbb5ed7be9f3e9cec33a65c2760d036447e77e63ae33d4`): 좌우 각 3,636, 총 7,272삼각형.
- [원본 진단](../../assets/items/valkyrie_starter/tripo_wing_v1/review.json), [겹침 보정 기록](../../assets/items/valkyrie_starter/tripo_wing_v1/fitted_patch_v1/clean-overlap-report.json), [노멀 보정 기록](../../assets/items/valkyrie_starter/tripo_wing_v1/fitted_patch_v1/front-normals-report.json).
- 여성 캐릭터 미리보기의 `발키리 날개 (피팅 시안)`에서 확인할 수 있다. URL의 `wings=1`로 처음부터 켠다.

### 수정 이력과 남은 검증

- 원본에서 같은 깊이의 겹친 면 때문에 뒷면이 깜박였다. 같은 방향의 면을 분할해 안쪽 면을 제거하고 미리보기 카메라의 깊이 정밀도를 높였다. 보정 전 양쪽 5,258삼각형에서 분할 후 7,316개였다. 이 이전 후보는 정리했으며 이후 사각형 보정으로 면 수가 달라졌다.
- 바깥쪽 사각형은 원본의 긴 삼각형과 주변 끝면이 만든 형상이었다. 삼각형만 제거한 시도는 아래 경계가 드러나 채택하지 않았다. 원본의 6개 면을 제거하고 같은 날개에서 복제한 13개 삼각형의 깃털을 변형해 주변 깃털 아래로 넣었다.
- 앞면의 어두운 선은 원본 정점 노멀이 주변과 달라 생겼다. 처음에는 다른 선을 보정해 효과가 없었고, 미리보기 화면에서 실제 면을 다시 찾아 좌우 각 4개 삼각형의 노멀을 보정했다. 위치·UV·가중치·삼각형 수는 유지했다.
- 브라우저 정면·후면·걷기, 여성 몸체 리그 연결, 유한 좌표와 퇴화 면, 타입·린트를 확인했다. 머리·갑옷과의 충돌, 게임 동작·장비 조합 및 성능은 아직 검증하지 않았다. 현재 GLB는 게임 출시용이 아니라 피팅 후보이며 면 수가 캐릭터 전체 예산에 미치는 영향도 검토해야 한다.

### 재현

저장소 루트에서 다음 순서로 실행한다. 정리한 중간 산출물은 각 명령이 다시 만든다. `wings-clean-preview.glb`는 노멀 보정 스크립트의 해시 검증 입력으로 보관한다.

```sh
blender -b --python-exit-code 1 --python assets/items/valkyrie_starter/tripo_wing_v1/repair-source.py
blender -b --python-exit-code 1 --python assets/items/valkyrie_starter/tripo_wing_v1/mirror-patch.py
blender -b --python-exit-code 1 --python assets/items/valkyrie_starter/tripo_wing_v1/fit-patch-preview.py
PYTHONPATH=tools .venv/bin/python assets/items/valkyrie_starter/tripo_wing_v1/fitted_patch_v1/prepare-preview.py
PYTHONPATH=tools .venv/bin/python assets/items/valkyrie_starter/tripo_wing_v1/fitted_patch_v1/clean-overlap-preview.py
PYTHONPATH=tools .venv/bin/python assets/items/valkyrie_starter/tripo_wing_v1/fitted_patch_v1/repair-front-normals.py
blender -b --python-exit-code 1 --python assets/items/valkyrie_starter/tripo_wing_v1/fitted_patch_v1/save-final-scene.py
```

## 상의 정면 원화 — 2026-11-01

![상의 정면 원화](../../assets/items/valkyrie_starter/concepts/upper-front-v1.png)

- 이미지: `assets/items/valkyrie_starter/concepts/upper-front-v1.png`, 1024×1536 RGB PNG, SHA-256 `beb13f965b3b26f079c8ffc44bfced3ebefd23b13a03415e69ebf7ba8b324cf0`.
- 참조: 위 복장 정면 시안 `front-v1.png` 및 기존 `client/public/character_concepts/valkyrie.webp`.
- 생성: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-11-01. OpenAI 생성 출력물 이용 조건 및 참조 에셋의 기존 이용 조건을 따른다.
- 은색 흉갑, 금색 장식, 견갑을 포함한 정면 디자인 후보다. 견갑은 사용자의 결정에 따라 상의에 포함한다. 하의와의 연결, 목 장식과 날개의 간섭은 아직 정하지 않았다. 후면·측면 구조와 실제 몸체 피팅을 검증하지 않았다.

### 상의 생성 프롬프트

```text
Use case: stylized-concept. Asset type: front-view concept for a modular 3D game character's Valkyrie upper-body armor. Image 1 is the existing full-body front outfit concept and primary authoritative design reference; image 2 is the original Valkyrie for consistency. Draw ONLY the upper-body armor as a single isolated wearable object, straight-on front view, centered and large, like a clean 3D reconstruction reference. Preserve the specific design and proportions of image 1: polished silver sculpted breastplate with two rounded plates, crossing silver trim below the bust, the restrained gold central branching motif, the smooth fitted silver waist and pointed lower hem, and matching layered silver shoulder pauldrons with small gold fasteners. Include the small silver neckline connector only where necessary to match the reference. Shape it as a plausible hollow wearable piece with clearly defined arm openings, neckline and lower edge; show open empty space where the body would go. The bottom ends around the waist/upper hip; do not include the reference's long front tabard, hip flaps or skirt panels. Shoulder pieces are shown as part of this preliminary concept; do not invent complex straps or new decoration. Bright neutral matte light-gray background, even soft studio lighting with controlled reflections to make silhouette and panel seams legible, polished realistic fantasy game concept render matching the reference. One view of exactly one object, symmetric and untwisted, no perspective distortion, full object visible with margin. No character, face, skin, mannequin, arms, hands, hair, wings, skirt, bracers, weapon, text, diagrams, inset, labels, watermark, or extra views. This is a design candidate, not a verified 3D fitting.
```

## 상의 후면 원화 — 2026-11-01

![상의 후면 원화](../../assets/items/valkyrie_starter/concepts/upper-back-v1.png)

- 이미지: `assets/items/valkyrie_starter/concepts/upper-back-v1.png`, 1024×1536 RGB PNG, SHA-256 `537dbada8c76a43d91d66b03f68021bbbec90f9013be13fcdb06f237334453ad`.
- 참조: 상의 정면 원화 `upper-front-v1.png`, SHA-256 `beb13f965b3b26f079c8ffc44bfced3ebefd23b13a03415e69ebf7ba8b324cf0`.
- 생성: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-11-01. OpenAI 생성 출력물 이용 조건 및 참조 에셋의 기존 이용 조건을 따른다.
- 사용자의 결정에 따라 견갑을 상의에 포함한다. 날개는 그리지 않았으며 등판과 날개 연결 위치·간섭은 실제 피팅에서 확인한다. 후면은 새로 제안한 디자인으로 정면·측면의 연결과 몸체 착용 상태는 아직 검증하지 않았다.

### 상의 후면 생성 프롬프트

```text
Use case: stylized-concept. Asset type: rear-view concept for the same modular Valkyrie upper-body armor. Input image is the authoritative FRONT view of this exact armor, including the shoulder pauldrons, which are part of the upper-body piece. Draw a single straight-on BACK view of the same wearable object in isolation, as if it were turned 180 degrees vertically; this is not another front view. Match the front image's exact outer silhouette, waist height, width, proportions, pointed lower hem, layered silver pauldrons with small gold fasteners, restrained gold accents, bright polished silver material and scale. The rear chest must be a plausible smooth contoured back plate, with coherent panel seams continuing from the sides of the front armor and shoulder attachments; do not repeat the breastplate cups or gold front chest ornament. Leave the upper back clean and readable for later fitting of a separate pair of wings; do not invent mounting holes, sockets, straps, or wing fixtures. Show neckline and arm openings and a plausible wearable construction. Symmetrical, no distortion, centered and fully visible with generous margins. Match the neutral light-gray background, controlled studio lighting and realistic fantasy game concept rendering of the reference. Show exactly one rear view of one object only. No person, mannequin, body, skin, hair, wings, skirt, tabard, forearm armor, diagram, labels, text or watermark. This is a preliminary rear design candidate and not an established fitted 3D model.
```

## Tripo 상의 원본 검토 — 2026-11-01

사용자가 전달한 `fantasy armor 3d model.glb`를 [원본 GLB](../../assets/items/valkyrie_starter/tripo_top_v1/source.glb)로 보관했다. SHA-256은 `73114c6ca023eccc4f785a1d34688fc29845c6782baa9ca41284ded5eaa1e9ea`다. Tripo 생성 날짜·설정·입력 이미지·작업 ID·요금제·이용 조건은 확인되지 않았다.

- 원본은 2,836삼각형, 정점 2,082개, 메시와 재질 각 1개이며 리그가 없다. 목표 3,000삼각형은 생성 전 제안값이고 실제 설정은 확인되지 않았다.
- [원본 정면](../../assets/items/valkyrie_starter/tripo_top_v1/source-front.png), [원본 후면](../../assets/items/valkyrie_starter/tripo_top_v1/source-back.png): Blender에서 원본을 불러와 렌더한 검사 이미지다.
- 정면의 흉갑·견갑은 원화와 유사하지만 후면 상부 등판은 넓게 비어 앞쪽 갑옷의 안쪽이 보인다. 후면 원화의 완전한 등판과 다르므로 생성 입력 및 보정 방법을 검토해야 한다.
- 여성 몸체에 원본을 단순 배치해 크기와 형태만 임시 확인했다. 피부·속옷과 관통하며 등판과 가림 처리, 몸체 곡면 보정 및 리깅은 하지 않았다. 피팅이나 게임 호환으로 간주하지 않는다.

### 열린 등 상의 피팅 — 2026-11-01

사용자는 원본처럼 등이 열린 형태를 선택했다. 여성 몸체와 기사 상의를 참고해 흉갑과 허리 간격을 조정하고 견갑을 상의에 포함했다. 가슴 중앙 피부를 보존하고 갑옷에 가려지는 부분은 숨긴다. 대기 동작에서 흉갑 옆이 접히지 않도록 흉갑은 몸통 본을, 분리된 견갑은 어깨 본을 따르게 했다.

- [피팅 GLB](../../assets/items/valkyrie_starter/tripo_top_v1/fitted_v7/top_female_valkyrie.glb), SHA-256 `0601e4f460d94152dd700b2df09599930706040862d16681b0db856cd97da465`, 2,836삼각형. [Blender 장면](../../assets/items/valkyrie_starter/tripo_top_v1/fitted_v7/fitting.blend), [피팅 스크립트](../../assets/items/valkyrie_starter/tripo_top_v1/fit-v7.py), [몸체 가림 정보](../../assets/items/valkyrie_starter/tripo_top_v1/fitted_v7/body-occlusion.json). 원본·몸체 및 기사 갑옷의 기존 출처와 이용 조건을 따른다.
- 여성 리그 `human_female_01_local_v1`, 몸체 SHA-256 `3eb7c0a5f3c6b30aec0fbae0b70c8fd925e4cb91772647420007eee795a1e0a5`. 기사 상의 GLB SHA-256 `410e32a90e4a1216cb7634329e907883d6879aaa2b91f31fe50170573fb824a7`.
- 대기 동작의 61개 시점에서 옆가슴 메시의 변형을 측정하고 미리보기에서 여러 시점과 좌우 단면을 확인했다. 본 이름과 역바인드 행렬, 정규화된 가중치와 퇴화 삼각형이 없음을 검사했다. 다른 동작의 전 구간과 장비 조합은 검증하지 않았다.

재생성: `blender -b --python-exit-code 1 --python assets/items/valkyrie_starter/tripo_top_v1/fit-v7.py`. 최종 가림 정보와 원본 GLB가 필요하다.
