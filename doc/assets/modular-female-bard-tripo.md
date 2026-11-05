# 여성 바드 복장

여성 바드는 지금 모듈형이 아닌 통짜 모델 `female_bard.glb`를 쓴다([캐릭터 기록](characters.md#bard)). 이 문서는 여성 공통 몸체에 입힐 모듈형 바드 복장 기록이다.
여성 공통 몸체는 [여성 몸체 기록](modular-human-female-01.md), 제작 절차는 [공통 워크플로우](modular-outfit-workflow.md)와 [연결 기준](modular-outfit-connections.md)을 따른다.

## 원화 — 2026-11-05

![여성 바드 원화](../../assets/items/female_bard_outfit/concepts/front-v1.png)

- 시안: `assets/items/female_bard_outfit/concepts/front-v1.png`, 1024×1536 RGB PNG, SHA-256 `56a3ccf0cbd09b14e842afa56232b54fe122097260df60fbe91a85a036e789d4`.
- 제작: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-11-05. OpenAI 생성 출력물 이용 조건 및 참조 에셋의 기존 이용 조건을 따른다.
- 참조 1·2: 순찰자 때 렌더한 하이힐 A자 베이스 `assets/items/female_ranger_outfit/concepts/base-front.png`(`80f06c60…2ab5`), `base-side.png`(`aacf5eb0…40a4d`). 같은 명령으로 다시 렌더하면 픽셀 차이가 1 이내다.
- 참조 3: 기존 여성 바드 원화 `client/public/character_concepts/female_bard.webp`(`c6ca05b8…1aa6`)를 회색 배경에 합성한 것. 모티프만 참고했다.
- 참조 4: 여성 순찰자 원화 `assets/items/female_ranger_outfit/concepts/front-v1.png`(`2c7a2c13…00f1`). 다리·부츠 위치 기준으로만 썼다.

사용자 요청: 순찰자처럼 하이힐 A자 베이스 위에 바드 복장을 그려 비율과 모양을 맞춘다. 하의는 바지 대신 짧은 치마(사용자 요청).
모든 여성 하이힐이 판금 부츠의 보정 트랙 하나를 같이 쓰므로, 부츠는 같은 30° `heel_pose` 베이스 위에 그렸다. 실제 접지 깊이는 피팅 때 판금 부츠와 맞춘다.

사용자 승인(2026-11-05). 세그먼트가 필요해 Tripo 생성·세그먼트는 사용자가 웹에서 직접 한다(API 미사용).

머리카락·모자·귀는 그리지 않았다. 헤어는 생성 화면의 선택(단발·긴 생머리·포니테일)을 쓴다.

검수: 발끝 최저점 1475px(베이스 1476px), 무릎·발목 높이의 다리 폭 차이 2–11px. 망토·퍼프 소매는 몸 밖으로 나온다.

실패 기록:

- v1(바지): 참조 1–3만 넣었더니 부츠 발끝이 베이스보다 33px 높아 정강이가 짧아졌고, 왼쪽 다리가 바깥으로 25px 벌어졌다. 순찰자 원화를 다리·부츠 위치 참조로 더해 해결했다. 폐기.
- v2(바지, 참조 4 추가)는 사용자가 짧은 치마로 바꾸기로 해 생성 중에 중단했다.

### 생성 프롬프트

```sh
codex exec --skip-git-repo-check --sandbox workspace-write -i base-front.png -i base-side.png -i female_bard.png -i ranger-front-v1.png - < prompt.txt
```

```text
Use case: precise-object-edit. Asset type: front-view outfit design reference that will be turned into a 3D model and fitted onto the exact body in Image 1, so the outfit must line up with that body pixel for pixel.
Image 1 is the OpenMMO female base body, front view, in its symmetric rig bind A-pose with feet already posed for high heels (heels raised, toes down). Keep Image 1's canvas, camera, scale, framing and figure position exactly. Keep her body silhouette, proportions, breast size, waist, hip width, leg spacing, arm angle, hand position, finger pose and foot angle exactly as in Image 1; do not slim, straighten or move any limb. Palms keep facing inward toward the body. Only paint clothing directly on top of this body, fitting closely like real clothes worn by this exact figure.
The legs and feet are the most important constraint. Her feet are already posed exactly as if standing in high heels: heels raised about 10 cm, standing on the balls of the feet. The boots are worn on her existing lower legs and feet and must not change their length, angle or position: each boot's ankle sits exactly at her own ankle, the boot foot wraps her own arched foot, the pointed toe ends at her own toe tips, and the stiletto heel tip runs straight down from under her own raised heel to the floor. Do not raise, lengthen or steepen her feet, and do not shorten her lower legs. Boot tops end just below her knees and each boot follows her own leg's angle and position.
Image 2 is the same body from the side; use it to understand the raised-heel foot angle. The boots' stiletto heels must sit under her raised heels and the soles follow her toes-down feet in Image 1.
Image 4 is the same body already dressed in a different outfit whose stiletto boots are placed correctly; it is the reference for boot placement only. Match its legs and feet exactly: the same leg spacing, the same lower-leg length, the boot toe tips ending at exactly the same height and position as Image 4's boot toe tips and Image 1's toe tips, and the heel tips touching the floor at the same height. Ignore Image 4's clothing, colors and hair; only its legs, boots and feet placement matter.
Image 3 is the existing OpenMMO female Bard, used only as the costume motif reference (ignore its T-pose, body and proportions). Dress her in the same Bard outfit with the same materials and palette: a short dark green capelet over the shoulders with a round bronze clasp at the collarbone; an ivory and dark green vertically striped shirt with full puffed long sleeves gathered at the wrists; a fitted reddish-brown tooled leather corset bodice with front lacing and a low sweetheart neckline; a brown leather belt with a square brass buckle and a second diagonal leather belt across the hips; instead of trousers, a short dark green pleated skirt ending at mid-thigh with a thin gold embroidered hem, matching the tunic panels of Image 3, with bare thighs between the skirt and the boots; brown leather fingerless gloves with laced leather forearm bracers. Reddish-brown leather boots with pointed stiletto high heels, reaching just below the knee, with front lacing, small buckles and a folded cuff with a bronze diamond plate at the top. No thigh straps, long hanging straps or dangling belt ends.
Do not add hair, a hat, a hood or ears; keep her head, face and neck exactly as in Image 1. No lute, instrument, weapon, bag or other held item.
Same neutral light gray plain background and even studio lighting as Image 1, realistic fantasy game concept rendering. Make garment edges, capelet hem, sleeve ends, glove cuffs, skirt hem, boot tops and skin openings clearly legible for later part segmentation. No text, labels, collage, inset, side view, watermark, or cropped limbs.
```

## Tripo 원본 — 2026-11-05

- 사용자가 위 원화로 Tripo에서 만들고 직접 세그먼트한 `Y:\public\web_downloads\fantasy female warrior 3d model.glb`(2026-11-05 20:05).
  [보관본](../../assets/items/female_bard_outfit/tripo_segmented_v1/original/model.glb),
  SHA-256 `f256e239c69c6924c3e418a97c92f692c5ebe9f93b963c79750bc30807e5984d`.
  생성일 외 요금제·작업 ID·설정은 전달되지 않았다. [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
- 높이 약 1.0m, 메시 18개, 20,277 triangles, 리그·애니메이션 없음. 모든 재질에 베이스컬러·노멀·거칠기/금속 텍스처가 있다(상의·치마 1024², 장갑·부츠 512²).
- 사용자 세그먼트: `left_glove` 2,442, `right_glove` 2,422, `left_boots` 1,738, `right_boots` 1,740. 상의(`tripo_part_0`, 5,714)와 치마(`tripo_part_2`, 1,961, 벨트 포함)는 Tripo 번호 그대로라 피팅 설정의 `rename`으로 이름을 붙인다.
  나머지 `tripo_part_*`는 머리·가슴 피부와 눈, 허벅지 피부로 참고용이다.

## 피팅 v1 — 2026-11-05

순찰자와 같은 [피팅 스크립트](../../tools/blender-scripts/fit_female_outfit.py)를 `--outfit bard`로 쓴다.

- 배치: 순찰자와 같은 1.86배, 30° `heel_pose` 들림만큼 내림. 머리 앞뒤 위치가 베이스와 0.15cm 안이라 앞뒤 보정은 없다.
- 상의·치마·부츠 처리는 순찰자 설정과 같다. 장갑은 손가락을 남긴다(아래 "반장갑 손가락"). 퍼프 소매는 팔 보호대 안에서 자른다. 순찰자의 소매 끝 피부색 띠 자르기는 쓰지 않는다.

| 부위 | 출력 | Triangles | 가린 몸체 삼각형 |
| --- | --- | ---: | ---: |
| 상의 | `top/fitted_v1/top_female_bard.glb` | 5,714 | 5,409 |
| 치마 | `bottom/fitted_v1/bottom_female_bard.glb` | 1,961 | 1,751 |
| 반장갑 | `gloves/fitted_v1/gloves_female_bard.glb` | 4,864 | 4,103 |
| 부츠 | `boots/fitted_v1/boots_female_bard.glb` | 3,478 | 2,531 |

네 부위 합계 16,017 triangles이고, 몸체는 따로다.

```sh
for p in gloves boots bottom top; do
  blender -b --python-exit-code 1 --python tools/blender-scripts/fit_female_outfit.py -- --outfit bard --part $p
done
blender -b --python tools/blender-scripts/review_female_outfit.py -- --outfit bard --parts gloves boots bottom top --out /tmp/review --views full torso --clip walk:8
```

여성 미리보기 `/modular-character-preview.html?character=female&top=bard&bottom=bard&gloves=bard&boots=bard`에서 네 부위를 입히고, `복장 피팅 조절`로 맞춘 값은 `doc/assets/female-bard-preview-fit.json`에 남는다.

### 부츠 접지

모든 여성 하이힐은 판금 부츠의 보정 트랙을 같이 쓴다. 부츠마다 따로 구웠을 때의 엉덩이 들림(`tools/compare-female-boot-lift.mjs`, 61 샘플, m):

| 부츠 | idle1 | walk 최소–최대 | run 최대 |
| --- | ---: | ---: | ---: |
| 판금(공용 트랙) | .0595 | .0695–.0742 | .0836 |
| 순찰자 | .0546 | .0144–.0245 | .0786 |
| 바드 | .0534 | .0138–.0241 | .0662 |

바드 부츠는 순찰자 부츠와 거의 같다. 공용 트랙에서는 서 있을 때 약 6mm, 걸을 때 약 5cm 뜬다.

여성 미리보기는 게임과 같이 모든 하이힐을 판금 부츠의 공용 트랙으로 띄운다(사용자 결정, 2026-11-05: 부츠별 접지로 보는 옵션은 실수로 끈 채 작업할 수 있어 두지 않는다). 신발 아래에 발바닥 최저점과 지면의 간격(지금·이 동작 최대)을 mm로 보인다.
바드 부츠는 idle1 최대 6mm, walk 최대 약 6–7cm 뜬다.
`발 옆모습 확대`(기본 켜짐)는 미리보기 창 아래 별도 칸에 발을 옆에서 정사영으로 그리고, 부츠 뒤 배경에 1cm 눈금(5cm마다 밝게, 지면은 빨강)을 둔다.
서 있을 때 판금·순찰자 부츠는 굽 끝이 지면에 닿지만, 바드 부츠는 앞꿈치만 닿고 굽 끝이 약 3cm 떠 있다(사제 장화는 약 1cm). 굽이 30° 자세에 비해 짧다.

### 굽 연장 — 2026-11-05

Tripo 원본에서 앞코가 바닥에 닿을 때 굽 끝이 1.37cm(피팅 배율로 약 2.5cm) 떠 있었다. 원화에서는 굽이 바닥까지 왔으므로 Tripo 생성의 오차다(순찰자는 0.34cm).
사용자가 미리보기에서 부츠를 앞으로 4° 돌리고 0.3cm 내려 앞꿈치를 지면에 맞췄다(양쪽 같음). 그다음 부츠 설정의 `extend_heel`이 발마다 굽 끝을 그 부츠의 가장 낮은 밑창까지 늘린다.
하이힐 자세에서 굽 끝 위 6cm 구간을 아래쪽일수록 많이 늘리고, idle에서 발가락이 굽어 남는 차이(왼쪽 2.9mm, 오른쪽 4.8mm, 게임 idle1에서 측정)를 더했다. 끝까지 늘리면(왼쪽 1.53cm, 오른쪽 1.65cm) 굽이 너무 길어 보여, 그 절반(0.76cm, 0.83cm)만 쓴다(사용자 결정). 서 있을 때 굽 끝은 밑창 최저점보다 약 0.8cm 위에 남는다. 공용 트랙에서 부츠 전체는 2–4mm 뜬다.
굽 기둥은 굽 끝이 위쪽보다 약 1cm 뒤로 나가 있었다. `extend_heel`의 `upright`(9cm)가 굽 끝에서 9cm 위(굽 컵과 만나는 곳)를 축으로 기둥을 강체 회전해 서 있는 자세에서 굽 끝이 수직선보다 0.5cm(끝 두께의 절반) 발 앞쪽에 오게 세우고(왼쪽 8.4°, 오른쪽 6.9°), 그다음 늘린다. 축 아래 1cm는 회전을 줄여 굽 컵과 잇는다(사용자 요청). 단면마다 중심을 옮기는 방식은 기둥 가운데가 부풀어 버렸다.

### 반장갑 손가락 — 2026-11-05

처음에는 순찰자처럼 생성 손가락을 지우고(`drop_skin`) 몸체 손가락을 보였다. 장갑 손가락 구멍 간격이 몸체 손가락보다 약 12% 넓고 손등 쪽으로 몇 mm 치우쳐 검지·엄지 구멍이 비었고, 손가락 밑동의 가림 경계가 톱니 조각으로 남았다.
지운 생성 손가락 네 개의 중심과 몸체 손가락 뼈 위치를 아래팔 좌표계에서 비교해 장갑 조절값을 두께(손가락이 늘어선 방향) 왼쪽 88%·오른쪽 87%, 앞쪽 이동 0.5·0.3cm(`frame: forearm`)로 저장했다(구멍 차이 2.5mm 안). 경계 면을 더 지우고 가림 경계를 다듬어도 조각이 남았고, 손가락 가림을 풀면 장갑 옆면으로 손가락이 뚫고 나왔다.
그래서 Tripo 손가락을 그대로 쓴다(사용자 결정). 사제 장갑처럼 손·손가락 피부를 모두 가리고, 생성 손가락은 가장 가까운 몸체 손가락의 가중치를 받는다. 위 조절값이 생성 손가락을 몸체 손가락 위에 맞춰 둔다. 장갑 4,864 triangles, 가린 몸체 삼각형 4,103. 대기·장검 쥐기에서 손가락이 손을 따라 움직이는 것을 미리보기로 확인했다.

### 금색 밑창 — 2026-11-05

사용자 요청으로 로그 부츠의 빨간 밑창처럼 밑창에 색을 넣었다. 원본 텍스처에 그런 부분이 없어, 부츠 설정의 `sole_colour`가 하이힐 자세에서 면을 골라 별도 재질을 준다. 아래를 향하는 면(밑창·아치·굽 끝, 밑창 최저점에서 11cm 안)과 굽 기둥에서 발 앞쪽을 향하는 면이 대상이고(233면), 굽 옆·뒤는 가죽 그대로다.
색 재질은 가죽 재질을 복제해 색만 바꿨다. 가죽의 거칠기·노멀 맵을 그대로 두면 흰·분홍 반사 얼룩이 생겨, 거칠기 0.6·금속 0·노멀 맵 없음으로 둔다. 아치 안쪽 일부 면은 아래를 덜 향해 가죽색으로 남는다.
처음엔 빨강이었으나 빨간색을 많이 써 식상하다는 의견으로 빨강·아이보리·짙은 녹색·금색을 구워 비교했고, 금색(sRGB 0.72, 0.55, 0.22)으로 정했다(사용자 결정).
옆에서 아치 가장자리의 금색 띠가 보여(사용자 지적), 아래를 향해도 좌우로 많이 기운 면(법선 x 성분 0.4 이상)은 칠하지 않는다(233 → 188면).

미리보기의 `복장 피팅 조절`에서 바드 부츠를 고르면 `밑창 색 칠하기`가 나온다. 켜면 부츠 위에 반경 1cm 원형 커서가 보이고, 짧게 클릭하면 중심이 원 안에 있는 삼각형이 밑창 색이나 가죽으로 바뀐다. 색 선택기로 밑창 색을 고른다. 저장값은 `female-bard-preview-fit.json`의 `boots.sole`(클릭한 삼각형의 바인드 공간 중심, 색 hex)이고, 피팅 스크립트가 중심에서 4mm 안의 면을 찾아 같은 색을 준다.

## 게임 시작 복장 — 2026-11-06

여성 바드는 예전 통짜 모델(`female_bard.glb`) 대신 공통 여성 몸체(`base.glb`)를 쓰고, 생성 화면에서 헤어·머리색·눈색을 고른다. 기본 헤어는 단발이다. 서버가 남녀 모두에게 바드 장비를 지급·장착하고, 여성은 장갑을 더 받는다.

| 아이템 | 슬롯 | 여성 런타임 파츠 |
| --- | --- | --- |
| `worn_bard_helmet` | head | 아직 없음(모자 피팅 전까지 보이지 않고 단발이 보임) |
| `worn_bard_top` | chest | `bard_top` |
| `worn_bard_pants` | pants | `bard_bottom` (짧은 주름치마) |
| `worn_bard_gloves` | hands | `bard_gloves` (새 아이템, 여성 전용 지급) |
| `worn_bard_boots` | boots | `bard_boots` |

- 모든 시작 장비는 거래·판매가 안 된다(`untradeable`, 가격 없음). 남성 바드의 기존 아이템을 같이 쓰고, 외형은 `worldModel`의 `armor/bard_` 접두어로 고른다.
- `worn_bard_gloves` 아이콘과 바닥 모델은 여성 장갑에서 `export_outfit_items.py --outfit bard --parts gloves --icon-rotation -72 0 0 --turn -30`으로 만들었다(`client/public/items/armor/bard_gloves.png`, SHA-256 `e8ce03fc…9c29`). 설명은 다른 바드 아이템처럼 짧게 썼다.
- 장비창 배경 `client/public/character_concepts/female_bard.webp`를 모자 원화(`hat-v1.png`)로 바꿨다. 가장자리에서 이어지는 회색 배경만 투명하게 한 RGBA WebP(q85), SHA-256 `555027e2…0cc2`. 예전 원화는 [캐릭터 기록](characters.md#bard)에 남는다.
- 기존 여성 바드는 배포 때 서비스를 멈추고 아래 도구로 빠진 장비만 가방에 넣는다(장착 상태는 그대로). 워크트리 DB 사본에서는 1명, 5품목이었다.

```sh
for p in top bottom boots gloves; do node tools/prepare-female-knight.mjs --part bard_$p; done
blender -b --python-exit-code 1 --python tools/blender-scripts/export_outfit_items.py -- --outfit bard --parts gloves --icon-rotation -72 0 0 --turn -30
.venv/bin/python tools/grant-female-bard-starter-outfit-20261106.py data/game_data.db            # 미리보기
.venv/bin/python tools/grant-female-bard-starter-outfit-20261106.py data/game_data.db --apply --expected-preview preview.json
```

워크트리 서버에서 여성 바드를 새로 만들어 생성 화면·캐릭터 선택·월드 진입을 확인했다. 만돌린 연주 동작과 다른 플레이어에게 보이는 모습은 아직 확인하지 않았다.

## 모자 원화 — 2026-11-06

![여성 바드 모자 원화](../../assets/items/female_bard_outfit/concepts/hat-v1.png)

- 시안: `assets/items/female_bard_outfit/concepts/hat-v1.png`, 1024×1536 RGB PNG. [프롬프트](../../assets/items/female_bard_outfit/concepts/hat-v1-prompt.txt).
- 제작: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-11-06. OpenAI 생성 출력물 이용 조건 및 참조 에셋의 기존 이용 조건을 따른다.
- 참조: 바드 원화 `front-v1.png`, 단발 썸네일 `client/src/assets/character-appearance/female-hair-bob.png`, 남성 바드 모자 원화 `assets/items/bard_starter/concepts/hat/hat-bard-front-v1.png`.
- 사용자 요청: 여성용 바드 모자를 원화부터 만든다. 투구를 쓰면 단발이 보이는 규칙(사제관과 같음)에 맞춰 단발 위에 쓴 모습으로 그렸다.
- 짙은 녹색 펠트 모자, 접어 올린 챙, 청동 장식의 가죽 띠, 아이보리·녹색 깃털.
- 두 번 모두 Codex가 인물을 33px 아래로 옮겨 다시 그렸다(크기는 같음). 몸 비율은 그대로라 Tripo 입력으로 쓰고, 피팅 배치는 키 대신 몸 기준점으로 잰다.
