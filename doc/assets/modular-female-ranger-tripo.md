# 여성 순찰자 Tripo 복장

여성 순찰자 전용 원화·복장은 이전에 없었다. 이 복장이 처음이다.
여성 공통 몸체는 [여성 몸체 기록](modular-human-female-01.md),
제작 절차는 [공통 워크플로우](modular-outfit-workflow.md)와 [연결 기준](modular-outfit-connections.md)을 따른다.

## 원화 — 2026-11-04

![여성 순찰자 원화](../../assets/items/female_ranger_outfit/concepts/front-v1.png)

- 시안: `assets/items/female_ranger_outfit/concepts/front-v1.png`, 1024×1536 RGB PNG, SHA-256 `2c7a2c13ee1332d8859cb0276c72641e967e136acd66370fe4d60816890a00f1`.
- 장비창 배경: `client/public/character_concepts/female_ranger.webp`는 시안의 균일한 회색 배경을 가장자리에서 이어지는 영역만 투명하게 만든 RGBA WebP(q85)다. SHA-256 `1ad8af2e3864ef46bf11e4d2eae2e722721d2ffb354a73bb272565e7478c4ca8`.
- 제작: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-11-04. OpenAI 생성 출력물 이용 조건 및 참조 에셋의 기존 이용 조건을 따른다.
- 참조 1·2: 여성 공통 몸체(`female-base.glb`, SHA-256 `3eb7c0a5f3c6b30aec0fbae0b70c8fd925e4cb91772647420007eee795a1e0a5`)의 정면·측면 렌더 `base-front.png`(`80f06c60…2ab5`), `base-side.png`(`aacf5eb0…40a4d`).
- 참조 3: 남성 순찰자 원화 `client/public/character_concepts/ranger.webp`(`a7be7a24…7f71`), 모티프만 참고했다.

사용자 요청: 남성 순찰자와 같은 모티프로, 하의는 짧은 가죽 치마, 신발은 남성 순찰자와 비슷한 길이의 가죽 스틸레토 부츠. 피팅이 맞도록 몸체를 하이힐 A자 자세로 렌더하고 그 위에 정확히 그리게 했다.

베이스 렌더는 모든 본을 바인드 자세로 되돌리고, 부츠와 같은 30° `heel_pose`를 적용한 뒤 발바닥을 바닥에 맞췄다(뒤꿈치 약 10cm). 가운데 본을 좌우 대칭으로 투영하고 좌우 팔다리를 평균 자세로 맞춰 완전히 대칭이다.

```sh
blender -b --python tools/blender-scripts/render_female_concept_base.py -- --out base-front.png --view front
blender -b --python tools/blender-scripts/render_female_concept_base.py -- --out base-side.png --view side
codex exec --sandbox workspace-write -i base-front.png -i base-side.png -i client/public/character_concepts/ranger.webp - < prompt.txt
```

실패 기록:

- GLB의 노드 기본 자세는 바인드 자세가 아니다(엉덩이 10°, 위팔 26°, 좌우 비대칭). 가져온 `idle1` 동작도 렌더 때 자세를 덮어써 발목이 구부러지지 않았다. 동작을 지우고 모든 본을 바인드로 되돌려야 한다.
- 평평한 발이나 비대칭 자세 위에 그린 시안 4장은 부츠 발목이 실제 발목보다 35–50px 높아 정강이가 짧아졌다. 좌표를 프롬프트에 넣어도 고쳐지지 않았고, 이미지를 늘이는 보정은 굽이 낮아졌다. 모두 폐기했다.

### 생성 프롬프트

```text
Use the built-in image generation tool to EDIT Image 1 (not a new composition) and produce one image.

Use case: precise-object-edit. Asset type: front-view outfit design reference that will be turned into a 3D model and fitted onto the exact body in Image 1, so the outfit must line up with that body pixel for pixel.
Image 1 is the OpenMMO female base body, front view, in its symmetric rig bind A-pose with feet already posed for high heels (heels raised, toes down). Keep Image 1's canvas, camera, scale, framing and figure position exactly. Keep her body silhouette, proportions, breast size, waist, hip width, leg spacing, arm angle, hand position, finger pose and foot angle exactly as in Image 1; do not slim, straighten or move any limb. Palms keep facing inward toward the body. Only paint clothing directly on top of this body, fitting closely like real clothes worn by this exact figure.
The legs and feet are the most important constraint. Her feet are already posed exactly as if standing in high heels: heels raised about 10 cm, standing on the balls of the feet. The boots are worn on her existing lower legs and feet and must not change their length, angle or position: each boot's ankle sits exactly at her own ankle, the boot foot wraps her own arched foot, the pointed toe ends at her own toe tips, and the stiletto heel tip runs straight down from under her own raised heel to the floor. Do not raise, lengthen or steepen her feet, and do not shorten her lower legs. Boot tops end just below her knees and each boot follows her own leg's angle and position.
Image 2 is the same body from the side; use it to understand the raised-heel foot angle. The boots' stiletto heels must sit under her raised heels and the soles follow her toes-down feet in Image 1.
Image 3 is the existing OpenMMO male Ranger, used only as the costume motif reference. Dress her in a matching female Ranger outfit with the same materials and palette: a fitted dark brown distressed leather vest with embossed leafy vine tooling and small rivets, worn over an olive-green long-sleeve shirt with a small collar; dark leather fingerless gloves with wrapped leather forearm bracers and small straps and buckles; a brown leather belt with a square brass buckle and small leather pouches. Instead of trousers, a short dark brown distressed leather skirt ending at mid-thigh, in the same leather as the vest, with bare thighs between skirt and boots. Brown laced leather boots with pointed stiletto high heels, reaching just below the knee like the male Ranger's boots, with front lacing and a folded leather cuff at the top.
Long dark brown hair tied back in a low ponytail behind the head, not covering the shoulders or the vest. No weapon, sword, scabbard, quiver, bow, cape or hood.
Same neutral light gray plain background and even studio lighting as Image 1, realistic fantasy game concept rendering. Make garment edges, sleeve ends, glove cuffs, skirt hem, boot tops and skin openings clearly legible for later part segmentation. No text, labels, collage, inset, side view, watermark, or cropped limbs.
```

## Tripo 원본 — 2026-11-04

- 사용자가 위 원화로 Tripo에서 만들고 직접 세그먼트한 `Y:\public\web_downloads\fantasy armor 3d model (1).glb`(2026-11-04 23:12).
  [보관본](../../assets/items/female_ranger_outfit/tripo_segmented_v1/original/model.glb),
  SHA-256 `255e4ab93e253c9b63d1991600cb82973db8a0efaf7264ff366947df0a824c5f`.
  생성일 외 요금제·작업 ID·설정은 전달되지 않았다. [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
- 높이 약 1.0m, 메시 64개, 19,415 triangles, 리그·애니메이션 없음. 모든 재질에 베이스컬러·노멀·거칠기/금속 텍스처가 있다(상의·치마·헤어 1024², 장갑·부츠 512²).
- 사용자 세그먼트: `top` 3,698, `skirt` 1,915, `left_glove` 2,204, `right_glove` 2,219, `left_boots` 1,158, `right_boots` 1,150, `hair` 3,916.
  나머지 `tripo_part_*`는 얼굴·눈·허벅지 피부로 참고용이다. 벨트와 주머니는 치마에 붙어 있다.

## 피팅 v1 — 2026-11-05

사제와 같은 [피팅 스크립트](../../tools/blender-scripts/fit_female_outfit.py)를 `--outfit ranger`로 쓴다. 차림새별 설정은 스크립트의 `OUTFITS`에 있다.

- 배치: 머리끝 높이를 하이힐 자세 베이스와 맞춰 1.86배, 전체를 30° `heel_pose`의 들림만큼(4.9cm) 내리고, 앞뒤 +7cm 옮긴다. Tripo가 모델을 경계 상자 중심에 놓아 몸이 7cm 앞에 있었다(머리 8.3cm, 허벅지 7.1cm). 원본이 하이힐 자세로 만들어져 사제의 부츠 발목 늘이기는 쓰지 않는다.
- 밀어내기 없음(사용자 결정, 2026-11-04). 옷 표면 3mm 안의 피부도 가린다. 표면에 붙은 피부가 광선 검사를 빠져나가 조끼 위에 점으로 비쳤다.
- 반장갑: 원본 장갑의 피부색 손가락(서로 이어진 15면 이상의 피부색 영역)과 그 주변의 떨어진 조각을 지우고 베이스 손가락을 보인다. 낱개 면까지 지우면 가죽 하이라이트에 구멍이 났다. 커프 테두리 평면 너머, 장갑 8mm 안의 아래팔·손 피부를 가린다.
- 부츠: 무릎 아래 맨살이 남도록 정강이는 부츠 입구 1cm 아래만 가리고, 발·발가락은 본 가중치로 가린다.
- 치마: 팬티를 가린다.
- 상의: 소매가 팔꿈치 바로 아래에서 끝나 장갑 커프 안에 들어간다. 소매는 원통 하나로 맞춘다(사용자 요청, 2026-11-05). 어깨 9cm 아래 단면을 중심으로 소매 끝 1.5cm 안쪽 단면 중심이 팔 피부 단면 중심에 오도록 회전하고, 사이는 그 회전을 따른다. 2cm마다 맞추는 방식은 쓰지 않았다.
  저장한 피팅값은 정렬 뒤에 적용해 미리보기와 같게 굽는다. 소매 끝은 텍스처의 피부색 띠 시작점 3mm 위를 평면으로 자르고, 그 너머 팔 피부는 장갑을 벗으면 맨살이라 보인다. 띠를 면 단위로 지우면 끝이 들쭉날쭉했다.
- 헤어: 원본의 갈색 포니테일을 그대로 쓴다. 꼬리는 가장 가까운 뒷목·등 피부의 가중치를 받는다. 게임은 헤어의 피부 가림을 쓰지 않는다.

| 부위 | 출력 | Triangles | 가린 몸체 삼각형 |
| --- | --- | ---: | ---: |
| 상의 | `top/fitted_v1/top_female_ranger.glb` | 3,629 | 5,417 |
| 치마 | `bottom/fitted_v1/bottom_female_ranger.glb` | 1,915 | 1,784 |
| 반장갑 | `gloves/fitted_v1/gloves_female_ranger.glb` | 3,374 | 2,391 |
| 부츠 | `boots/fitted_v1/boots_female_ranger.glb` | 2,308 | 2,476 |
| 포니테일 | `hair/fitted_v1/hair_female_ranger.glb` | 3,916 | (게임 미사용) |

다섯 부위 합계 15,142 triangles이고, 몸체는 따로다. 수치는 각 `fitting.json`에 있다.

사용자가 미리보기에서 맞춘 값(2026-11-05): 상의 앞 1.9cm와 소매 끝(왼쪽 앞뒤 −2.1·좌우 −0.5cm, 오른쪽 −1.6·−1.0cm), 치마 앞 0.6cm, 장갑 왼쪽 바깥 0.8·앞 1.3cm, 오른쪽 바깥 1.1·위 0.4·앞 2.7cm(양쪽 101%), 부츠 양쪽 뒤 0.3cm. 브러시 칠과 함께 `doc/assets/female-ranger-preview-fit.json`에 있다.

```sh
for p in gloves boots bottom hair top; do
  blender -b --python-exit-code 1 --python tools/blender-scripts/fit_female_outfit.py -- --outfit ranger --part $p
done
blender -b --python tools/blender-scripts/review_female_outfit.py -- --outfit ranger --parts gloves boots bottom top hair --out /tmp/review --views full torso --clip walk:8
```

장갑을 먼저 굽는다. 상의의 소매 자르기가 굽힌 장갑의 커프를 읽는다.

사제와 함께 쓰는 처리에서 고친 점: 소매를 자른 뒤 맨살을 되살리는 범위를 아래팔·손 피부로 한정했다. 몸 한쪽 전체를 보던 탓에 아랫배까지 되살아났다. 사제 상의를 다시 구우면 아랫배 가림이 늘어난다. 장갑·상의·부츠의 나머지 결과는 바뀌지 않았다(GLB와 가림 정보 해시 일치 확인).

### 미리보기 조절

여성 미리보기(`?character=female&top=ranger&bottom=ranger&gloves=ranger&boots=ranger&hair=ranger`)에서 `복장 피팅 조절`이 사제와 순찰자 부위를 함께 보여 준다. 저장값은 `doc/assets/female-{outfit}-preview-fit.json`에 차림새별로 남는다.
순찰자 상의를 고르면 소매 끝 슬라이더(좌우 소매마다 앞뒤·좌우, 상완 축 기준, ±5cm)가 나온다. 어깨 9cm 아래에서 0, 소매 끝에서 설정값만큼 선형으로 옮긴다. 상완 축에서 7cm 안의 정점만 움직여 조끼는 그대로다.
변환은 [female-part-fit.ts](../../client/src/previews/female-part-fit.ts)의 `sleeveEndDeformer`와 [female_outfit_adjust.py](../../tools/outfits/female_outfit_adjust.py)의 `sleeve_end`가 같고, 테스트가 두 결과를 맞춘다.
2026-11-05부터 장갑 조절은 아래팔 좌표계를 쓴다(사용자 요청). 원점은 팔꿈치–손목 축 위에서 장갑 중심에 가장 가까운 점이고, 너비는 팔 바깥쪽, 높이는 팔 길이(팔꿈치 쪽 +), 두께는 팔 앞쪽이며 회전도 이 축 기준이다. 저장값에 `frame: "forearm"`이 붙고, 표시가 없는 예전 값(사제·순찰자 장갑)은 캐릭터 축으로 굽는다. 사제·순찰자 장갑을 다시 구워 GLB·가림 해시가 그대로임을 확인했다. 변환은 `partFitMatrix`와 `forearm_matrix`가 같다.

## 게임 시작 복장 — 2026-11-05

여성 순찰자는 공통 여성 몸체에 포니테일을 고정 부위로 더한 `base.glb#ranger`를 쓴다(발키리의 `#valkyrie`와 같은 방식). 투구를 쓰면 다른 여성 캐릭터와 같은 헤어 규칙을 따른다.
지금까지 클라이언트가 순찰자를 남성으로만 만들게 해서 여성 순찰자는 생성할 수 없었다. 이번에 생성을 열고, 서버가 남녀 모두에게 네 장비를 지급·장착한다.

| 아이템 | 슬롯 | 런타임 파츠 |
| --- | --- | --- |
| `worn_ranger_top` | chest | `ranger_top` |
| `worn_ranger_pants` | pants | `ranger_bottom` (짧은 치마) |
| `worn_ranger_gloves` | hands | `ranger_gloves` |
| `worn_ranger_boots` | boots | `ranger_boots` |

- 남성 순찰자의 기존 아이템을 같이 쓰고, 외형은 `worldModel`의 `armor/ranger_` 접두어로 고른다. 128px 아이콘은 브레스트플레이트·사제 아이템처럼 세워서 약간 오른쪽 위에서 본 각도로 다시 찍었다(남성 모델 기준, 2026-11-05). 설명은 짧은 시적 문장으로 5개 언어 모두 바꿨다.
- 부츠 보정 트랙을 통일했다(사용자 결정, 2026-11-05). 모든 하이힐이 기사 판금 부츠의 트랙을 함께 쓴다(서 있을 때 약 6cm, 걸을 때 약 7cm, 부츠 중 가장 높은 쪽). 발키리를 뺀 하이힐은 같은 30° `heel_pose`라 원래 엉덩이 높이만 달랐고, 다른 하이힐은 걸을 때 최대 2–6cm 뜰 수 있지만 게임 시점에서 차이가 보이지 않았다. 36°로 맞춘 발키리 부츠도 30°로 보인다. 발은 부츠 안에서 가려진다.
  변형이 7개에서 1개로 줄었다. 보정을 다시 구울 때 지운 이전 트랙의 키프레임이 파일에 남던 것도 정리해, `base.glb`가 2.97MB에서 2.15MB로 줄고 애니메이션 팩도 함께 작아졌다.
- 기존 여성 순찰자가 있으면 배포 때 서비스를 멈추고 아래 도구로 빠진 장비만 가방에 넣는다(장착 상태는 그대로). 로컬 DB 사본에는 없었다.

```sh
for p in top bottom boots gloves hair; do node tools/prepare-female-knight.mjs --part ranger_$p; done
blender -b --python-exit-code 1 --python tools/blender-scripts/export_outfit_items.py -- --outfit ranger --icons-only --icon-rotation -72 0 0 --turn -30
node tools/prepare-female-knight.mjs --bake-existing
.venv/bin/python tools/grant-female-ranger-starter-outfit-20261105.py data/game_data.db            # 미리보기
.venv/bin/python tools/grant-female-ranger-starter-outfit-20261105.py data/game_data.db --apply --expected-preview preview.json
```

워크트리 서버에서 여성 순찰자로 접속해 캐릭터 선택 화면과 월드 진입을 확인했다. 5,000명 동시 접속 부하 시험은 하지 않았다. 장비 조합은 `femaleCharacter.test.ts`가 모든 방어구 아이템으로 확인한다.
