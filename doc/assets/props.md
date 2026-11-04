# Props and Building Assets

## Furniture

- Hearthbound Rug / 귀향의 러그 (2026-10-03) — 벽난로 앞에 놓는 붉은 양모 러그. 집·불꽃 중심 문양, 금빛·크림색 테두리, 짧은 술 장식.
  - 원화·텍스처: [hearthbound_rug.png](../images/props/hearthbound_rug.png), Codex 내장 ImageGen, **ChatGPT Pro 20x**, 생성일 2026-10-03. AI 생성 출력물이며 OpenAI 이용약관 적용. [전체 생성 프롬프트](hearthbound-rug-prompt.txt).
  - 모델·아이콘: 프로젝트 자체 제작 Blender 메시·렌더, 프로젝트와 동일 라이선스. Meshy 미사용. `client/public/models/objects/hearthbound_rug.glb`, `client/public/items/objects/hearthbound_rug.png`.
  - 2.4×1.6m 평면, 2삼각형, 바닥에서 1.2cm 높이, 중심 원점, 상향 법선. 평면 소품이므로 일반 아이템의 약 4,000폴리곤 목표보다 단순하게 제작했다. 1024×683 RGBA 텍스처, alpha MASK 0.5, 양면·거친 직물 재질. 술 사이 투명도 유지, 이동 충돌 없음.
  - 재생성: `blender -b -t 4 -P tools/blender-scripts/build_hearthbound_rug.py`. 소스·생성 기록·미리보기는 `assets/items/hearthbound_rug/`에 저장. 아이콘은 공통 Cycles 직교 렌더 512²→128².
  - 2026-10-28 전송량 최적화: 원본 1024×683 PNG를 같은 크기의 WebP q94로 변환했다. GLB는 1,638,688 → 394,820바이트, gzip level 6은 1,638,376 → 394,145바이트(75.9% 감소)다. 메시 속성과 알파 값은 동일하며 Three.js로 같은 조명의 단독 모델을 비교했다. 게임 화면 비교는 아직 하지 않았다. Blender에서 GLB를 생성한 뒤 `node tools/optimize-hearthbound-rug.mjs client/public/models/objects/hearthbound_rug.glb client/public/models/objects/hearthbound_rug.glb`를 실행한다. 게임용 바이너리는 Hugging Face에 게시하고 `assets.lock`에 고정했다.
  - `hearthbound_rug` 맵 에디터 오브젝트와 `furniture_hearthbound_rug` 실내·야외 배치 아이템 등록. ORKEA 전시물 ID 112와 점원 메뉴에서 2,000동화(20은화)에 판매한다. 바닥 전시는 운영자가 직접 배치했다. 영지에 설치하면 영지 귀환 주문서의 도착 지점이 되며, 설치 위치·층·방향을 사용한다. 여러 개면 먼저 설치한 안전한 러그를 선택한다.

- bed.glb https://sketchfab.com/3d-models/old-wooden-bed-frame-and-dirty-sheets-79c856755e6a44a3bcf19671e5c70d2d
  - `client/public/items/objects/bed.png` — 기존 `bed.glb`를 Blender 5.2.0 LTS로 렌더한 ORKEA 기본 침대 아이콘 (2026-09-19). 원본: effiebop의 Old wooden bed frame and dirty sheets, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). `blender -b -P tools/blender-scripts/render_furniture_icon.py -- bed`로 투명 배경·직교 사선 구도 512²→128² 렌더.

## Objects

- Altar of the Earth Heart / 대지 심장의 제단 (가칭) — 테라클래시를 깨우는 봉헌 제단. [설계](../TIER5_FIELD_RAID.md).
  - OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.162.1), **ChatGPT Pro 20x**, 2026-10-21. OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 없음.
  - [원화](../../assets/objects/terraclash_altar/concept_v3/concept.png) — 아뮬렛·테라클래시 원화를 참고로 시안 3종(사각 블록·원통·바위)을 그려 바위안을 골랐고, 홈 안을 테라클래시 가슴 구멍과 같은 용암빛으로 채운 뒤 바위·이끼·균열·청동판 색을 게임 모델 `terraclash.glb` 가슴 렌더에 맞췄다. 홈의 용암빛이 밝은 돌 위에서 멀리서도 보인다. 중간 시안은 지웠고 프롬프트·해시만 생성 기록에 남겼다. [프롬프트](../../assets/objects/terraclash_altar/concept_v3/prompt.txt) · [생성 기록](../../assets/objects/terraclash_altar/concept_v3/generation.json).
  - 3D: Tripo API 유료 크레딧, 모델 `P2-20260801`, 텍스처 `v3.5-20260815`, detailed PBR, 목표 5,000 faces, seed `102101`, 2026-10-21, **120크레딧**. Tripo 이용 약관 적용.
    원화 한 장을 넣었다. 원본은 `assets/objects/terraclash_altar/tripo_v1/source/model_url.glb`, 요청·작업 기록은 `tripo_v1/build.json`·`generation.json`.
    Tripo는 홈의 하트 모양을 살리지 못하고 윗면에 주황 얼룩만 칠했다.
  - 게임 모델 `client/public/models/objects/terraclash_altar.glb` — `tools/blender-scripts/prepare_terraclash_altar.py`가 정면을 +Z로 돌리고 폭 1.6m·높이 1.0m로 맞춘 뒤(1.54×1.6×1.0m), 원점을 바닥 중심에 둔다.
    `earth_amulet.glb` 펜던트를 위에서 본 실루엣(체인·고리 제외)으로 길이 0.5m, 깊이 0.1m 홈을 boolean으로 파고, 바닥과 벽에 발광 재질 `altar_heart_core`·`altar_heart_wall`을 준다.
    펜던트 끝이 정면을 향한다. 바위 재질은 발광 없음, 텍스처 1024² WebP q90, 5,858 triangles. Blender 파일은 `assets/objects/terraclash_altar/game_v1/`.
- Shop Sign 변형 (`shop_sign_plank`, `shop_sign_oval`, `shop_sign_weathered`) — `client/src/lib/utils/shop-sign.ts`에서 생성하는 각진 판형·타원형·완만한 아치형 메시 (2026-09-06). 메시 출처: 프로젝트 자체 코드, 프로젝트와 동일 라이선스. 텍스처는 기존 Poly Haven `wood_planks`, `dark_wooden_planks`, `weathered_planks` 재사용 (CC0, 아래 House 항목). 간판 문구는 맵에디터에서 수정하며 형태에 맞춰 렌더링한다.
  - `client/public/items/objects/shop_sign.png`, `shop_sign_plank.png`, `shop_sign_oval.png`, `shop_sign_weathered.png` — ORKEA 간판 4종 아이콘 (2026-09-19). 게임의 `buildShopSignBoard`가 만든 메시·UV와 각 스타일의 기존 Poly Haven 나무 텍스처를 그대로 사용해 Blender 5.2.0 LTS에서 투명 128² PNG로 렌더. 메시·렌더 스크립트는 프로젝트와 동일 라이선스, `wood_shutter`, `wood_planks`, `dark_wooden_planks`, `weathered_planks` 텍스처는 CC0. `node tools/export-shop-sign-icons.mjs` 실행 후 `blender -b -P tools/blender-scripts/render_furniture_icon.py -- MODEL --geometry assets/shop-sign-icons/MODEL.json --yaw -10 --tilt -80`으로 재생성한다. 간판 문구는 포함하지 않는다. `assets/shop-sign-icons/`의 중간 메시 JSON 4개는 2026-10-09 삭제했으며, 아이콘 재생성 시 내보내기 도구로 다시 만든다.
- empty_plate.glb — Meshy AI "Sunlit Ceramic Plate" (Pro 요금제, 2026-09-02 생성, 소스 `assets/objects/empty_plate/Meshy_AI_Sunlit_Ceramic_Plate_0902163541_texture.glb`). 완전 소유권·상업 OK (characters.md License 참조). 여관 테이블에서 손님이 다 먹은 접시 — `Meal.eaten`이면 클라이언트가 요리 모델 대신 이걸 그린다 (HUNGER.md "여관 식사"). Blender에서 chicken_rice.glb와 같은 지름 0.35m로 스케일 적용(0.35×0.03×0.35 W×H×D), 원점=바닥 중심, 텍스처 2048→512 축소, 검은 emissive 제거 (2026-09-03). 아이콘 없음(월드 전용)
    - 원화는 ChatGPT 이미지 생성 (ChatGPT Pro 20x, 2026-09-03) ![원화](../images/props/empty_plate.png)
- stone bridge https://sketchfab.com/3d-models/stone-bridge-a5d380cd08654b508b4b643056038605
- bridge wood https://sketchfab.com/3d-models/bridge-wood-20c090db0a7345898e20e2621fc2ba4c
- big bridge https://sketchfab.com/3d-models/bridge-9328bbfc04a84202a6a97bd59408473a
- bridge_wood_long.glb — Alex Gimson의 [Wooden Bridge Deep](https://sketchfab.com/3d-models/wooden-bridge-deep-27b22af7020c4755b5cb788d75db8ee7), [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
  - 2026-09-17: `tools/repack-glb-textures.py`로 base color·normal PNG를 JPEG q92 4:4:4로 변환. 1024² 해상도, 메시·UV·재질 설정·투명 충돌면은 유지. 6,519,420 → 3,651,048 bytes (44.0% 감소), 로컬 Nginx gzip level 6 전송 실측 1,795,527 bytes (원본 무압축 대비 72.5% 감소). Three.js 전체·근접 렌더 비교 및 메시 버퍼 일치 확인. 원본은 최적화 전 `assets.lock`의 SHA-256 `9a4f9a3104d22cc1d0b5eb17f4f8ef54f8aef324339577a876bc5505e7eef189`로 식별.
  - 2026-09-20: `tools/compress-glb-meshes.mjs`로 중복 정리·정점 양자화·`EXT_meshopt_compression` 적용. 삼각형 53,666개, JPEG 2장, 투명 충돌면을 보존했다. 3,651,048 → 1,938,312 bytes; 운영과 같은 Node gzip level 6 기준 1,836,596 → 1,302,813 bytes (29.1% 감소). 게임·GLB 편집기의 Three.js 로더에 내장 MeshoptDecoder를 연결했다. 전체·근접 렌더 비교와 실제 다리 바닥 7,105점 비교에서 통행 가능 영역 변화 없음, 높이 차이 최대 0.574mm.
  - 재생성: `npm install --prefix tools --no-save --package-lock=false @gltf-transform/core@4.4.2 @gltf-transform/extensions@4.4.2 @gltf-transform/functions@4.4.2 meshoptimizer@1.2.0` 후 `node tools/compress-glb-meshes.mjs SOURCE.glb client/public/models/objects/bridge_wood_long.glb`. 압축 전 원본 SHA-256은 `48c36af6d1a1f9eb99205f03a98a4e9bc1e2f98d4d2f641e93c718c7c6b6efb4` (`assets.lock` revision `675ce6a06ddd0742d346c66af18f0a9ca3a6aa97`). 이미 meshopt 압축된 입력은 재양자화를 피하도록 거부한다.
- signpost.glb https://sketchfab.com/3d-models/road-sign-blacksmiths-workshop-assets-3a230f0520034890931c32539955223a
  - `client/public/items/objects/signpost.png`, `signpost_mirrored.png` — 기존 표지판 모델과 좌우 반전 모델의 ORKEA 아이콘 (2026-09-19). 원본: Kyan0s의 Road sign - Blacksmith's workshop assets, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Blender 5.2.0 LTS, 투명 배경·직교 사선 구도 512²→128². `blender -b -P tools/blender-scripts/render_furniture_icon.py -- MODEL`로 재생성한다.
- dungeon objects https://sketchfab.com/3d-models/fps-dungeon-extras-87425249dded42aa891516c31a5b94cf
  - `storage_chest.png` — 기존 `chest_animated.glb`의 닫힌 기본 포즈를 투명 배경의 직교 사선 구도로 렌더한 128×128 영지 보관함 아이콘 (2026-09-07). 기존 게임 모델을 그대로 사용했으며 새 외부 자산 없음
  - `client/public/items/objects/chair.png` — 기존 `chair.glb`를 Blender 5.2.0 LTS로 렌더한 ORKEA 의자 아이콘 (2026-09-19). 원본: DJMaesen의 FPS Dungeon Extras, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). `tools/blender-scripts/render_furniture_icon.py -- chair`와 공용 `icon_render.py`로 투명 배경·직교 사선 구도 512²→128² 렌더. 의자에 임시로 연결했던 상자 아이콘을 대체한다.
  - `client/public/items/objects/crate.png` — 기존 `crate.glb`를 Blender 5.2.0 LTS로 렌더한 ORKEA 나무 상자 아이콘 (2026-09-19). 원본: DJMaesen의 FPS Dungeon Extras, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). `blender -b -P tools/blender-scripts/render_furniture_icon.py -- crate`로 투명 배경·직교 사선 구도 512²→128² 렌더. Crate에 임시로 연결했던 보관함 아이콘을 대체한다.
  - `client/public/items/objects/barrel.png` — 기존 `barrel.glb`를 Blender 5.2.0 LTS로 렌더한 ORKEA 나무통 아이콘 (2026-09-19). 원본: DJMaesen의 FPS Dungeon Extras, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). `blender -b -P tools/blender-scripts/render_furniture_icon.py -- barrel`로 투명 배경·직교 사선 구도 512²→128² 렌더. Barrel에 임시로 연결했던 보관함 아이콘을 대체한다.
- coin_pile_spill.glb https://sketchfab.com/3d-models/coins-7367feabcd4c4b30a7ba64b95b76bee0 (Blender에서 수정 + 쏟아짐 애니메이션 추가; 던전 체스트가 떨어뜨리는 줍기 코인). 아이콘 `client/public/items/objects/coin_pile.png`는 이 GLB를 Blender 헤드리스로 임포트해 스필 마지막 프레임(35)에서 흩어진 코인 28개를 중앙 더미로 다시 모은 뒤 Cycles 직교 측면·위 각도 렌더 512²→128² (2026-08-13). coin_pile은 주우면 지갑으로 바로 들어가 인벤토리를 거치지 않으므로 이 아이콘이 실제로 표시되는 경로는 없음 — 완결성용
- torch_wall.glb https://sketchfab.com/3d-models/torch-238cd6056e2940debb4f67fc24c6df35 (던전 벽에 붙이는 토치)
  - 전송량 최적화 (2026-10-16): `torch_wall.37ca1c79.glb`의 텍스처 5장을 **1024² PNG → 512² WebP**로 축소하고 meshopt 압축을 적용했다. 색상맵은 q90·effort 6, 노멀·금속성/거칠기/AO·반사색·발광 맵은 축소 결과를 무손실로 저장한다. 파일 **2,626,548 → 567,284바이트(78.40% 감소)**, gzip level 6 예상 전송량 **2,548,250 → 518,876바이트(79.64% 감소)**. 다음 빌드 파일명은 `torch_wall.e73b1769.glb`다.
  - 1,746 triangles·1,728정점의 모든 속성·UV, 삼각형 순서·방향, 노드 변환과 경계 치수(0.228055×0.600000×0.257894m)를 유지한다. 발광 색상·강도, `KHR_materials_specular` 설정, IOR 1000과 AO·metallicRoughness 텍스처 공유를 유지한다. 게임의 `loadGLB`로 전후 모델을 불러 같은 조명·거리의 외형과 외부 조명을 끈 발광 상태를 비교했다. 브라우저 오류·경고 0, Khronos glTF Validator 오류 0·기존 접선 생성 경고 1개를 확인했다. 던전 횃불의 광원 위치·강도 코드는 변경하지 않았다.
  - 기존 DJMaesen·Sketchfab 출처와 CC BY 4.0 라이선스를 유지하며 새 생성·유료 호출은 없다. 최적화 전 게임 파일은 `assets/items/torch_wall/torch_wall-before-optimization.glb`에 보존한다. 최적화 파일과 보존 원본을 HF에 게시하고 원격 SHA-256을 확인했으며 `assets.lock`에 고정했다. 운영 배포는 수행하지 않았다. 최적화 파일 SHA-256: `9fa2440279273aeabe3a8a8349c58521c71c43a15a870f7294cce9a89396019a`, 보존 원본: `cbbb0685cf378a488b89d043bc09d4b64240e7ecdce8b874a0620fc9fa2fc1e0`.
  - 재현: `node tools/optimize-character-textures.mjs assets/items/torch_wall/torch_wall-before-optimization.glb client/public/models/objects/torch_wall.glb 512 --all-textures`.
  - 미사용 UV 제거 (2026-10-17): 모든 텍스처가 UV 0만 쓰는데 `TEXCOORD_1~3`이 남아 있었다. meshopt 압축 뒤 속성이 버퍼마다 나뉘어 던전 벽 횃불에서 WebGPU 정점 버퍼 한도(8)를 넘었다. 최적화 도구가 어떤 텍스처도 쓰지 않는 UV를 지우도록 고치고 같은 재현 명령으로 다시 만들었다. 속성 6→3개, 567,284 → 534,404바이트, 텍스처는 이전 결과와 같다. 최적화 파일 SHA-256: `438fa1b6a8067e198cee71977fbb025fe3ff307fa20ea94438196efd38abeb5e`.
  - `client/public/items/objects/torch_wall.png` — 기존 `torch_wall.glb`를 Blender 5.2.0 LTS로 렌더한 ORKEA 벽 횃불 아이콘 (2026-09-19). 원본: DJMaesen의 Torch, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). `blender -b -P tools/blender-scripts/render_furniture_icon.py -- torch_wall`로 투명 배경·직교 사선 구도 512²→128² 렌더.
- `mine_timber_post.glb` — 검은맥 광산 통로 벽에 붙는 지지대 기둥 (2026-11-03). 사용자가 Tripo에서 원화 한 장으로 생성했다(유료 크레딧, Tripo 이용 약관 적용). 원본은 `assets/objects/mine_timber_post/tripo/source.glb`(446 triangles, 2048² PBR 3장). Blender 5.2.0 LTS로 높이 2.6m 균일 스케일 뒤 높이만 던전 벽 높이 3.0m로 늘렸다(0.41×3.00×0.705m). 들보 받침이 +Z(통로 쪽)를 향하고 평평한 뒷면이 z=0에 오게 돌렸다. 클라이언트는 뒷면을 벽 안으로 2cm 묻는다. 원점=바닥·폭 중앙·뒷면, 텍스처 512² WebP q90. 벽 횃불과 같이 local −Z=0 면이 벽에 붙는다.
  - 원화·정면·측면은 OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-03. OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 없음. 이슈 #204 컨셉 시트를 색감 참고로 넣었다. [원화](../../assets/objects/mine_timber_post/concept/concept.png) · [생성 기록](../../assets/objects/mine_timber_post/concept/generation.json). 처음 그린 들보까지 있는 문틀형(`mine_timber_frame`)은 위에서 내려다보는 시점에서 통로를 가려 버렸다.
- `mine_rail.glb` — 검은맥 광산 통로 바닥에 이어 까는 2m 레일 한 구간 (2026-11-04). 사용자가 Tripo에서 원화 한 장으로 생성했다(유료 크레딧, Tripo 이용 약관 적용). 원본은 `assets/objects/mine_rail/tripo/source.glb`(848 triangles, 2048² PBR 3장). 위에서 본 원화라 Tripo가 세워서 만들었으므로, Blender 5.2.0 LTS로 원화를 보던 면이 위로 오게 눕히고 길이 2.0m로 균일 스케일했다(0.70×0.13×2.00m). 길이가 local Z, 원점=바닥 중심, 텍스처 512² WebP q90. 세 조각을 이어 렌더해 레일과 받침목 간격이 이음매에서 끊기지 않는 것을 확인했다.
  - 원화는 OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-04. OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 없음. [원화](../../assets/objects/mine_rail/concept/concept.png) · [생성 기록](../../assets/objects/mine_rail/concept/generation.json). 처음에는 비스듬히 본 원화로 만들었는데 위쪽 레일이 평행하지 않게 생성돼 이음매마다 단차가 생겼다. 그래서 위에서 본 정사영 원화로 다시 만들었다.
- `mine_cart.glb` — 검은맥 광산 6–15층 방 벽을 따라 세워 둔 광석 수레 (2026-11-04). 사용자가 Tripo에서 정면·측면 원화로 생성했다(유료 크레딧, Tripo 이용 약관 적용). 원본은 `assets/objects/mine_cart/tripo/source.glb`(1976 triangles, 2048² PBR 3장). Blender 5.2.0 LTS로 길이 1.3m에 맞춰 균일 스케일했다(0.65×1.30×0.97m). 길이가 local Z, 원점=바닥 중심, 텍스처 512² WebP q90. 두 칸을 막는 고정 소품이다.
  - 원화·정면·측면은 OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-04. OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 없음. [원화](../../assets/objects/mine_cart/concept/concept.png) · [정면](../../assets/objects/mine_cart/concept/front.png) · [측면](../../assets/objects/mine_cart/concept/side.png) · [생성 기록](../../assets/objects/mine_cart/concept/generation.json). 처음 그린 원화는 광석이 테두리 높이에만 있어서 Tripo가 윗면을 평평한 판으로 만들었다. 그래서 광석이 테두리 위로 솟은 지금 원화로 다시 그리고 정면·측면을 뽑았다. 처음 원화와 모델은 지웠다.
- `mine_tool_rack.glb` — 검은맥 광산 1–10층 방의 북·동쪽 벽에 기대 둔 광부 공구 선반 (2026-11-04). 사용자가 Tripo에서 정면·측면 원화로 생성했다(유료 크레딧, Tripo 이용 약관 적용). 원본은 `assets/objects/mine_tool_rack/tripo/source.glb`(2948 triangles, 2048² PBR 3장). Blender 5.2.0 LTS로 폭 1.4m에 맞춰 균일 스케일했다(1.40×1.42×0.63m). 공구 면이 +Z, 원점=바닥·폭 중앙·뒷면, 텍스처 512² WebP q90. 벽 횃불처럼 local −Z=0 면이 벽에 붙고, 벽을 따라 두 칸을 막는 고정 소품이다.
  - 원화·정면·측면은 OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-03(원화)·2026-11-04(정면·측면). OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 없음. [원화](../../assets/objects/mine_tool_rack/concept/concept.png) · [정면](../../assets/objects/mine_tool_rack/concept/front.png) · [측면](../../assets/objects/mine_tool_rack/concept/side.png) · [생성 기록](../../assets/objects/mine_tool_rack/concept/generation.json).
- `mine_ore_heap.glb` — 검은맥 광산 1–20층 방 벽가에 쌓인 광석 더미 (2026-11-04). 사용자가 Tripo에서 정면·측면 원화로 생성했다(유료 크레딧, Tripo 이용 약관 적용). Tripo가 내려 준 파일 이름은 `rock pile 3d model.glb`. 원본은 `assets/objects/mine_ore_heap/tripo/source.glb`(977 triangles, 2048² PBR 3장). Blender 5.2.0 LTS로 폭 1.4m에 맞춰 균일 스케일했다(1.40×1.18×0.54m). 원점=바닥 중심, 텍스처 512² WebP q90. 2×2칸을 막는 고정 소품이고 90° 단위로 무작위로 돌린다.
  - 원화·정면·측면은 OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-03(원화)·2026-11-04(정면·측면). OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 없음. [원화](../../assets/objects/mine_ore_heap/concept/concept.png) · [정면](../../assets/objects/mine_ore_heap/concept/front.png) · [측면](../../assets/objects/mine_ore_heap/concept/side.png) · [생성 기록](../../assets/objects/mine_ore_heap/concept/generation.json).
- `mine_rubble.glb` — 검은맥 광산 11–20층 방 벽가의 무너진 돌무더기와 부러진 버팀목 (2026-11-04). 사용자가 Tripo에서 정면·측면 원화로 생성했다(유료 크레딧, Tripo 이용 약관 적용). 처음 받은 결과는 텍스처가 기본색 한 장뿐이라 PBR로 텍스처를 다시 생성했다. 원본은 `assets/objects/mine_rubble/tripo/source.glb`(833 triangles, 2048² PBR 3장). Blender 5.2.0 LTS로 긴 변 1.8m에 맞춰 균일 스케일했다(1.70×1.80×0.82m). 원점=바닥 중심, 텍스처 512² WebP q90. 2×2칸을 막는 고정 소품이고 90° 단위로 무작위로 돌린다.
  - 원화·정면·측면은 OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-03(원화)·2026-11-04(정면·측면). OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 없음. 정면·측면에서는 원화의 먼지를 뺐다. [원화](../../assets/objects/mine_rubble/concept/concept.png) · [정면](../../assets/objects/mine_rubble/concept/front.png) · [측면](../../assets/objects/mine_rubble/concept/side.png) · [생성 기록](../../assets/objects/mine_rubble/concept/generation.json).
- `mine_crystal_cluster.glb` — 검은맥 광산 6–25층 방 벽가에 자라난 검보라 수정 무리 (2026-11-04). 사용자가 Tripo에서 정면·측면 원화로 생성했다(유료 크레딧, Tripo 이용 약관 적용). Tripo가 내려 준 파일 이름은 `purple crystal cluster 3d model.glb`. 원본은 `assets/objects/mine_crystal_cluster/tripo/source.glb`(978 triangles, 2048² PBR 3장). Blender 5.2.0 LTS로 높이 1.6m에 맞춰 균일 스케일했다(1.12×1.12×1.60m). 원점=바닥 중심, 텍스처 512² WebP q90. 2×2칸을 막는 고정 소품이고, 깊을수록 많아지며 클라이언트가 6층부터 한 층에 2.5%씩 키운다(25층 약 1.48배).
  - 원화·정면·측면은 OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-03(원화)·2026-11-04(정면·측면). OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 없음. [원화](../../assets/objects/mine_crystal_cluster/concept/concept.png) · [정면](../../assets/objects/mine_crystal_cluster/concept/front.png) · [측면](../../assets/objects/mine_crystal_cluster/concept/side.png) · [생성 기록](../../assets/objects/mine_crystal_cluster/concept/generation.json).
- healing potion https://sketchfab.com/3d-models/low-poly-health-potion-dca8a2144a1446fe8391f54cc5f6959e
- scroll https://sketchfab.com/3d-models/scroll-7450e494eb654e9b937bb52724220e77 (scroll_enchant.glb는 같은 모델의 파란 봉인 변형. 소스는 ~/assets_original/scroll.blend (레포 밖 보관) — 두 머티리얼 모두 알파에 Math/ROUND 노드가 들어 있어 glTF 익스포트 시 alphaMode=MASK가 됨. 이 노드를 지우면 BLEND로 나가 봉인이 떠 보이는 문제가 재발하니 유지할 것. scroll_enchant_armor.glb는 scroll_enchant.glb의 알베도만 hue-rotation(파란 봉인 → 초록, -0.27 회전 + 명도 0.8배)해 GLB 바이너리에 되박은 초록 봉인 변형 — 메시·노멀·러프니스와 alphaMode=MASK는 그대로다, 스크립트, 2026-08-15)
- river rock https://sketchfab.com/3d-models/river-rocks-model-2dc354c1f84a43f493343f54e05eaed9
- campfire.glb — Meshy AI "Crimson Ember Stack" (Pro 요금제, 2026-08-02 생성). Meshy 유닛큐브를 지름 0.6m로 스케일 적용, 원점을 바닥 중앙으로 이동. 텍스처 2048→512 축소, metallic 맵은 전부 0이라 제거하고 metallicFactor=0. emissive는 잔불 발광이라 유지
    - 원화는 ChatGPT 이미지 생성 (ChatGPT Pro 20x, 2026-08-03) ![원화](../images/props/campfire.png)
- black_market_table.glb — Meshy AI "Bottles and Scrolls" (Pro 요금제, 2026-08-09 생성, 소스 `assets/items/black_market_table/Meshy_AI_Bottles_and_Scrolls_o_0809151940_texture.glb`). 완전 소유권·상업 OK (characters.md License 참조). 암시장 상인이 앞에 펼쳐 놓는 매대 — 병·두루마리·빵·상자가 놓인 좌판형 테이블. Blender에서 기존 table.glb(W 1.6m) 기준 너비 1.6m로 스케일 적용(1.60×0.96×0.89 W×H×D), 원점=바닥 중심, 텍스처 2048→512 축소, 검은 emissive 제거 (2026-08-10). 상인 `/lay_stall` 좌판으로 사용 (GameSceneStallsLayer)
    - 원화는 ChatGPT 이미지 생성 (ChatGPT Pro 20x, 2026-08-09) ![원화](../images/props/black_market_table.png)
- stone_hearth.glb — Meshy AI "Ancient Stone Hearth" (Pro 요금제, 2026-08-27 생성, 소스 `assets/items/stone_hearth/Meshy_AI_Ancient_Stone_Hearth_0827182508_texture.glb`). 완전 소유권·상업 OK (characters.md License 참조). 집 안에 놓는 돌 벽난로 가구 — 아치 화구·방패 문장 목재 선반·굴뚝. Blender에서 층 높이(DEFAULT_WALL_HEIGHT 3m) 기준 높이 2.9m로 스케일 적용(1.93×2.90×0.70 W×H×D, 벽 붙이는 얕은 형태), 원점=바닥 중심, 텍스처는 베이스 컬러 2048 유지·metallic/roughness·normal 1024, JPEG q85로 export(3.2MB; 층 높이 프롭이라 512는 흐림), emissive 없음 확인. 아이콘은 Cycles 직교 측면·위 각도 렌더 512²→128² `client/public/items/objects/stone_hearth.png` (2026-08-28). items.csv `stone_hearth` furniture, catalog.json solid
    - 원화는 ChatGPT 이미지 생성 (ChatGPT Pro 20x, 2026-08-28) ![원화](../images/props/stone_hearth.png)
    - 2026-10-16 전송량 최적화: `stone_hearth.d3af9635.glb`의 색상맵을 2048² JPEG → **1024² WebP q90·effort 6**으로 축소하고 meshopt 압축을 적용했다. 노멀·metallicRoughness JPEG 바이트는 유지한다. 파일 **3,231,608 → 622,944바이트(80.72% 감소)**, gzip level 6 예상 전송량 **3,183,056 → 604,952바이트(80.99% 감소)**. 다음 빌드 파일명은 `stone_hearth.51453362.glb`다.
    - 3,810 triangles·4,489정점의 모든 속성, 삼각형 순서·방향, 노드 변환, 재질 설정과 경계 치수(1.930569×2.900000×0.696014m)를 유지한다. 실제 게임의 `loadGLB`로 전후 모델을 불러 동일한 조명·거리에서 정면·사선·작은 표시 크기를 비교했다. 작은 크기에서 차이가 작으며, 확대하면 돌의 미세한 질감과 방패 문장이 부드러워진다. 브라우저 오류·경고 0, Khronos glTF Validator 오류 0·기존 접선 생성 경고 1개·meshopt 미지원 안내 1개를 확인했다.
    - 최적화 전 게임 파일은 `assets/items/stone_hearth/stone_hearth-before-textures-1024.glb`에 보존한다. 기존 Meshy Pro 출처·라이선스를 유지하며 새 생성·유료 호출은 없다. 최적화 파일과 보존 원본을 HF에 게시하고 원격 SHA-256을 확인했으며 `assets.lock`에 고정했다. 운영 배포는 수행하지 않았다. 최적화 파일 SHA-256: `858cae99f543c521997a295f105e81953dd398e7f5206c041dd13aaf32e2ecf1`, 보존 원본: `1f8f5e27dc544627fb8e0f75c05e1febdf32d9141f73c5abf018b059ec5d51df`.
    - 재현: `node tools/optimize-character-textures.mjs assets/items/stone_hearth/stone_hearth-before-textures-1024.glb client/public/models/objects/stone_hearth.glb 1024`.
- rustic_bed.glb — Meshy AI "Rustic Wooden Bed" (Pro 요금제, 2026-08-30 생성, 소스 `assets/items/rustic_bed/Meshy_AI_Rustic_Wooden_Bed_0830120934_texture.glb`). 완전 소유권·상업 OK (characters.md License 참조). 집 안에 놓는 소박한 나무 침대 — 둥근 손잡이 기둥 4개·아치 헤드보드·베개·갈색 담요. Blender에서 긴 쪽(길이) 2.2m로 스케일 적용(0.91×0.98×2.20 W×H×D, 기존 bed.glb 2.58m보다 작게), 원점=바닥·발끝(bed.glb처럼 눕는 위치가 원점이라 머리판이 -Z 끝, 발끝이 z 0), 텍스처 2048→1024(2m 가구라 512는 흐림), JPEG q85로 export(0.37MB), 검은 emissive 제거. 아이콘은 Cycles 직교 측면·위 각도 렌더 512²→128² `client/public/items/objects/rustic_bed.png` (2026-08-30). items.csv `rustic_bed` furniture, catalog.json sleep(offset y 0.56)·solid
    - 원화는 ChatGPT 이미지 생성 (ChatGPT Pro 20x, 2026-08-30) ![원화](../images/props/rustic_bed.png)

- 이졸데 텐트 잠자리 후보(2026-10-24): [바닥 매트리스](../../assets/items/ysolde_camp_bed/ground-mattress.glb)·[낮은 목제 야전침대](../../assets/items/ysolde_camp_bed/field-cot.glb). 기존 `rustic_bed`의 매트리스·담요·베개 메시와 텍스처를 분리해 재사용했다. 야전침대의 가로대·세로대·교차 다리는 같은 침대의 목재 기둥 메시와 UV로 로컬 조립했다. 원본 Meshy **Pro**, 생성 2026-08-30, 상업 이용 조건을 유지하며 새로운 AI·유료 생성은 없다.
    - Blender 5.2.0, 실제 미터·오브젝트 배율 1·바닥 중심 원점. 매트리스 0.84×2.10m·높이 0.22m·1,666 triangles, 야전침대 0.91×2.10m·높이 0.45m·2,062 triangles. 야전침대는 [런타임 GLB](../../client/public/models/objects/ysolde_field_cot.glb)(299,996바이트)로 복사해 맵 에디터 Object → **낮은 야전침대**(`ysolde_field_cot`)에 등록했다. 원본과 같은 SHA-256·재질이며 일반 가구처럼 `solid` 충돌과 90° 회전을 사용한다. 미리보기도 같은 런타임 모델을 읽는다. 바닥 매트리스는 미리보기 후보로 유지한다.
    - 클릭하면 기존 `sleep` 동작으로 눕는다. 중앙 원점인 모델에 맞춰 서버가 발끝 기준 위치를 로컬 +Z 0.9m만큼 회전·이동해 승인하고, 클라이언트는 매트리스 높이 0.34m를 더한다. 모델과 기존 배치 좌표는 유지한다. 네 방향의 눕기·일어나기와 방송 위치를 테스트했고, 기사 모델의 수면 자세를 사선·측면에서 렌더해 매트리스 위에 맞는 것을 확인했다. 기존 침대의 휴식 회복 규칙도 적용된다.
    - 실내에서 숨긴 텐트 전면이 침대 클릭을 가로채지 않도록 오브젝트 클릭 검사에서 자신 또는 상위 그룹이 숨겨진 면을 제외한다. 실제 배치 좌표·모델과 기본 게임 카메라로 침대 위 9개 지점을 검사해 모두 수면 요청으로 처리됨을 확인했다.
    - [Blender 원본](../../assets/items/ysolde_camp_bed/bedding.blend), [재생성 소스](../../assets/items/ysolde_camp_bed/build.py), [출처·라이선스·해시](../../assets/items/ysolde_camp_bed/sources.json), [매트리스 렌더](../images/props/ysolde-camp-bedding/ground-mattress.png)·[야전침대 렌더](../images/props/ysolde-camp-bedding/field-cot.png).

## Mounts

- rowboat.glb — `tools/blender-scripts/build_rowboat.py`가 절차적으로 생성하는 노 젓는 배 (2026-09-14,
  Blender 5.2.1 LTS). 메시의 라이선스는 저장소를 따르며, 나무 텍스처 출처와 이용 조건은 아래에 기록한다.
  스크립트와 `assets/items/rowboat/wood_albedo.png`로 재생성한다 (`blender -b -P tools/blender-scripts/build_rowboat.py`).
  선체는 단면 19점 × 길이 37 스테이션을 5cm 판재 두께로 안팎 두 겹 로프팅한 닫힌 셸이다.
  선미는 선체와 이어진 5cm 두께의 평판으로 막고, 안쪽 끝단을 그만큼 앞당겼다.
  선수 끝은 양쪽 꼭짓점을 중심선에서 용접해 틈 없이 만나며, 안쪽 선수는 5cm 뒤에서 닫힌다
  (2026-09-17, Blender 5.2.0 LTS로 모델·원본·아이콘 재생성).
  가로보 3개(`ThwartBow`/`ThwartMid`/`ThwartStern`)와 노 2개(`OarPort`/`OarStarboard`)는 클라이언트가
  따로 돌릴 수 있게 별도 노드다. 정지 자세의 노는 손잡이를 축으로 날 쪽을 9° 들어 올려
  뱃전 위에 걸친다. 노의 기울기는 노드 회전으로 보존한다 (2026-09-17).
  삼각형 3,140개, 머티리얼 2개(`BoatOak`, `BoatTrim`)가
  1024² WebP q90 나무 텍스처 하나를 공유한다. 선체·바닥은 길이 방향, 가로보는 폭 방향,
  노는 자루·날의 길이 방향으로 UV를 배치했다 (2026-09-17). 거칠기는 선체 0.82, 나머지 0.7.
  크기 1.37×3.40m(폭×길이), 용골 -0.15m ~ 뱃전 0.53m.
  **원점은 용골이 아니라 흘수선**(draft 0.15m)이라 샘플링한 수면 높이에 그대로 올리면 된다.
  뱃머리는 Blender -Y = **glTF +Z**. 탑승점은 말과 같은 이름의 빈 오브젝트 `RideSeat`
  (가운데 가로보 위, glTF 기준 `[0, 0.17, 0]`, 선미인 -Z를 향하도록 180° 회전, 2026-09-17).
  아이콘 `client/public/items/objects/rowboat.png`는
  같은 스크립트가 `icon_render` 공용 레시피로 렌더한다.
  말 출처는 [animals.md](animals.md), 게임 규칙은 [MOUNTS.md](../MOUNTS.md).
- `assets/items/rowboat/wood_albedo.png` — 나룻배용 따뜻한 갈색 오크 판재 텍스처 원본, 1254² PNG.
  - 출처: OpenAI 내장 `image_gen`으로 생성, 2026-09-17 (KST).
  - 생성 도구/티어: Codex 내장 이미지 생성; 계정 요금제와 서비스 티어는 도구에서 미노출.
  - 라이선스: OpenAI 생성 출력물에 적용되는 서비스 약관. 별도 제3자 stock/CC 에셋을 사용하지 않음.
  - 프롬프트: [rowboat-wood-prompt.txt](rowboat-wood-prompt.txt). 수평 나뭇결·판재 5줄, 균일한 조명,
    무광의 가벼운 풍화, 좁은 이음새를 요청했다. 생성 원본은 보존하고 빌드에서 1024²로 축소해
    GLB에 내장하며, `assets/items/rowboat/rowboat.blend`에도 텍스처를 포함한다.

## House

Poly Haven에서 받은 .gltf를 Blender에서 .glb로 다시 export

- wood_planks_1k.glb -> https://polyhaven.com/a/wood_planks
- planks_brown_10_1k.glb -> https://polyhaven.com/a/planks_brown_10
- dark_wooden_planks_1k.glb -> https://polyhaven.com/a/dark_wooden_planks
- marble_01_1k.glb -> https://polyhaven.com/a/marble_01
- weathered_planks_1k.glb -> https://polyhaven.com/a/weathered_planks
- wood_trunk_wall_1k.glb -> https://polyhaven.com/a/wood_trunk_wall
- wood_shutter_1k.glb -> https://polyhaven.com/a/wood_shutter
- wood_plank_wall_1k.glb -> https://polyhaven.com/a/wood_plank_wall
- clay_roof_tiles_02_1k.glb -> https://polyhaven.com/a/clay_roof_tiles_02
- clay_roof_tiles_03_1k.glb -> https://polyhaven.com/a/clay_roof_tiles_03
- grey_roof_tiles_02_1k.glb -> https://polyhaven.com/a/grey_roof_tiles_02
- medieval_blocks_03_1k.glb -> https://polyhaven.com/a/medieval_blocks_03
- red_brick_1k.glb -> https://polyhaven.com/a/red_brick
- reed_roof_03_1k.glb -> https://polyhaven.com/a/reed_roof_03
- sandstone_blocks_04_1k.glb -> https://polyhaven.com/a/sandstone_blocks_04
- worn_mossy_plasterwall_1k.glb -> https://polyhaven.com/a/worn_mossy_plasterwall
- beige_wall_001.glb -> https://polyhaven.com/a/beige_wall_001
- rough_linen.glb -> https://polyhaven.com/a/rough_linen
- wooden_garage_door_1k.glb -> https://polyhaven.com/a/wooden_garage_door (던전 입구 문)
- rusty_metal_grid_1k.glb -> https://polyhaven.com/a/rusty_metal_grid (CC0, 2026-08-30) — 열쇠가 있어야 열리는 잠긴 층 문(doc/DUNGEON_REWARD.md). Poly Haven glTF ZIP을 Blender로 임포트해 GLB로 export. 제작용 원본은 `assets/textures-src/dungeon/rusty_metal_grid_1k.glb`에 보관하며, 다운로드 ZIP은 사용자 요청으로 2026-10-10 삭제했다.
- grey_stone_path_1k.glb -> https://polyhaven.com/a/grey_stone_path (던전 바닥/계단)

## Dungeon Textures

하우징·던전 GLB 텍스처도 `tools/repack-material-glbs.py`로 만든 배포본이다(원본 `assets/textures-src/`, 규칙은 environment.md Terrain Textures 참조).

기존 Poly Haven 복도 벽 텍스처. 자연 동굴 세트로 교체(2026-09-17).

- **[미사용]** rock_wall_10 https://polyhaven.com/a/rock_wall_10 — 기존 던전 복도 벽, CC0. 저장된 하우징 텍스처 인덱스 보존을 위해 카탈로그 슬롯 유지.
- damaged_plaster — **[미사용]**
- old_stone_wall — **[미사용]**
- plaster_stone_wall_02 — **[미사용]**
- rabdentse_ruins_wall — **[미사용]**
- rock_wall_05 — **[미사용]**
- rock_wall_08 — **[미사용]**
- rock_wall_13 — **[미사용]**
- rustic_stone_wall — **[미사용]**

### 던전 통로 세트 (2026-09-18)

출처: OpenAI Codex built-in ImageGen. 요금제: workspace-provided tier(정확한 등급은 도구에서 공개되지 않음). 최초 생성일: 2026-09-17, 현재 사용본 편집·생성일: 2026-09-18. 라이선스: 프로젝트용 생성 출력물, OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 지정 없음. [풍화 텍스처 편집 프롬프트](dungeon-weathering-prompts.json) · [현재 석재 블록 벽 생성 프롬프트](dungeon-stone-blocks-prompt.json).

경로: `client/public/textures/dungeon/`. 생성 PNG를 ffmpeg로 1024² WebP q88로 축소·인코딩했다.

| 세트 | 벽 | 바닥 |
| --- | --- | --- |
| 회색 석회암 | `cave-limestone-wall.webp` | `cave-limestone-floor.webp` |
| 이끼 암반 | `cave-moss-wall.webp` | `cave-moss-floor.webp` |
| 회색 석재 블록·어두운 타일 | `cave-masonry-wall.webp` | `cave-masonry-floor.webp` |
| **[미사용]** 갈색 셰일 | `cave-shale-wall.webp` | `cave-shale-floor.webp` |

2026-09-18 석재 블록 벽 뒷면: 기존 `cave-limestone-wall.webp`를 뒷면과 위·옆 단면에 재사용한다. 앞면의 블록·줄눈과 재질을 분리하고 실제 벽 크기에 맞춰 반복해 자연 암반 무늬를 표시한다. 출처·요금제·생성일·라이선스는 위 석회암 텍스처와 동일하며 추가 이미지 생성은 없다. 벽 하단은 바닥까지 수직으로 이어진다.

2026-09-18 공통 벽 데칼: `wall-weathering-decals.webp` — OpenAI Codex built-in ImageGen, workspace-provided tier(정확한 등급 미공개), 생성일 2026-09-18. 프로젝트용 생성 출력물이며 OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 지정 없음. [생성 프롬프트](dungeon-wall-decals-prompt.json). 배경이 투명한 1536×1024 RGBA 이미지에 이끼·균열·물자국을 각각 2종씩 담았다. 원본 PNG의 알파를 보존해 WebP q90으로 인코딩했으며 기존 벽 텍스처는 수정하지 않았다.

세 통로 세트와 던전 방 벽의 앞·뒷면에 독립적인 데칼 메시로 투영한다. 던전·층·벽 위치로 배치를 고정하고 종류·위치·크기·좌우 반전·농도를 달리한다. 이끼·균열에는 회전도 주고, 물자국은 위에서 아래로 내려오며 일부 끝에 이끼를 덧붙인다. 벽의 실제 요철을 따라 잘라 붙이고 벽별로 한 메시로 합친다. 캐릭터를 가리는 벽에서는 데칼을 숨기며 이동 충돌·바닥 클릭에 관여하지 않는다. 메시 배치 코드는 프로젝트 자체 제작, 프로젝트와 동일 라이선스.

2026-09-18 방 벽·균열·거미줄 확장: `wall-cracks-cobwebs.webp` — OpenAI Codex built-in ImageGen, workspace-provided tier(정확한 등급 미공개), 생성일 2026-09-18. 프로젝트용 생성 출력물이며 OpenAI 출력물 이용 조건 적용, 별도 CC 라이선스 지정 없음. [편집·생성 프롬프트](dungeon-wall-details-prompts.json). 기존 데칼 이미지를 참조해 선이 굵고 짙은 균열 2종과 낡은 거미줄 2종을 생성하고, 알파를 보존해 1024×1024 WebP q90으로 인코딩했다. 기존 이끼·물자국 이미지는 그대로 재사용한다. **[미사용]** `wall-weathering-decals.webp` 가운데 열의 가는 균열 2종은 새 균열로 교체했다. 균열은 크기·농도를 높이고, 거미줄은 벽 위쪽 끝에 확률적으로 배치한다. 방 벽의 높이 오프셋과 문 입구를 포함한 실제 벽 범위에 맞춰 투영한다. 기존 얼룩과 새 균열·거미줄은 한 메시의 두 재질 그룹으로 합친다.

균열은 통로·방 모두 벽의 위쪽 또는 아래쪽 접점에서 안쪽으로 뻗도록 배치한다. 데칼 중심을 가장자리 가까이 옮기고 벽 바깥 부분을 잘라 중앙에 떠 있는 모양을 줄였다. 기존 이미지와 다른 데칼의 배치는 유지한다.

[통로 세 세트와 방 벽의 균열·거미줄](../devlog/images/dungeon-room-decals-v12.webp) — 2026-09-18 자체 캡처, 프로젝트 소유 문서 이미지. 위·아래 접점으로 균열을 옮긴 실제 벽 메시와 재질을 같은 카메라·조명으로 비교한 별도 Three.js 렌더이며 게임 플레이 스크린샷은 아니다.

**[미사용·11차 시안]** [세 세트의 데칼 적용 전후](../devlog/images/dungeon-wall-decals-v11.webp) — 2026-09-18 자체 캡처, 프로젝트 소유 문서 이미지. 균열을 강화하고 거미줄·방 벽 적용을 추가하기 전의 별도 Three.js 렌더이며 게임 플레이 스크린샷은 아니다.

2026-09-18 석재 블록 벽 재생성: `cave-masonry-wall.webp`는 큰 돌 면과 짙은 줄눈이 먼저 보이도록 새로 생성했다. 블록 중심부의 얼룩·잔무늬·세로 물자국을 줄이고, 넓게 닳은 모서리와 틈새의 때·소량의 이끼를 남겼다. 새 이미지의 4개 가로줄에서 온전한 블록 8개를 골라 매핑하며, 블록별 좌우 반전과 색조 변화로 반복감을 줄인다. 기존 44×22cm 블록 메시와 22% 불투명도의 평면 가림 처리를 유지한다. **[미사용]** 직전의 얼룩이 많은 회색 벽 텍스처는 같은 경로에서 교체했으며 `dungeon-weathering-prompts.json`의 해당 벽 항목을 미사용으로 표시했다.

[새 석재 블록 벽 비교](../devlog/images/dungeon-stone-blocks-v10.webp) — 2026-09-18 자체 캡처, 프로젝트 소유 문서 이미지. 왼쪽과 가운데는 같은 메시 크기·카메라·조명에서 이전 텍스처와 새 텍스처를 비교하며, 오른쪽은 새 텍스처의 22% 평면 가림 상태다. 자체 캐릭터 마커를 사용한 별도 Three.js 렌더이며 게임 플레이 스크린샷은 아니다.

갈색 셰일은 이끼 암반과 분위기가 비슷하다는 사용자 피드백에 따라 벽돌·타일 세트로 교체했다. 기존 셰일 이미지는 시안으로 보관한다.

2026-09-18: 세 세트의 벽·바닥 6장을 기존 이미지 참조 편집으로 교체했다. 석회암에는 검게 밴 습기와 세로 물자국, 틈새의 흙·소량의 이끼를 추가했고, 이끼 암반에는 짙은 올리브색 이끼와 불규칙한 젖은 얼룩을 늘렸다. 벽돌 세트는 붉은 점토색을 회색 석재 블록으로, 밝은 베이지 타일을 어두운 회색 타일로 바꿨다. 벽의 블록 배열과 바닥의 4×4 격자는 유지하며, 때·광물 침전·물자국은 색상 텍스처에 표현한다. 기존 물웅덩이·물방울 효과와 함께 사용한다.

**[미사용]** 2026-09-17의 깨끗한 석회암·이끼 암반 및 붉은 벽돌·밝은 타일 텍스처: 같은 경로의 현재 사용본으로 교체. [최초 자연 암반 생성 프롬프트](dungeon-cave-prompts.json) · [최초 벽돌·타일 생성 프롬프트](dungeon-masonry-prompts.json)는 이력으로 보관한다.

[풍화 텍스처 적용 미리보기](../devlog/images/dungeon-weathered-corridors.webp) — 2026-09-18 자체 캡처, 프로젝트 소유 문서 이미지. 실제 통로 메시와 재질을 별도 Three.js 장면에서 같은 카메라·조명으로 렌더했다. 게임 플레이 스크린샷은 아니다.

던전 ID와 층 깊이로 세트를 고정 선택한다. 통로 벽·바닥에만 적용하며 방·계단은 기존 재질을 쓴다. 벽 상단은 약 10cm 두께로 시야를 확보한다. 자연 암반 두 세트는 바닥과 만나는 하부 약 16cm를 둥글게 처리하고, 벽돌 세트는 바닥과 직각으로 만난다. 자연 암반 벽은 중간 면이 완만하게 돌출·함몰되며 끝과 상단에서 굴곡이 줄어든다. 벽돌 세트는 대부분 평평하게 엇갈려 쌓고, 내부 벽돌의 약 4%만 1.5–4cm 돌출시키며 약 2%는 1.2cm 후퇴시킨다. 연결부와 맨 위·아래 줄은 평평하다. 벽돌 크기는 기존 텍스처 2배 반복에 맞춘 약 44×22cm이며 원본 이미지의 벽돌 면을 골라 매핑한다. 평평한 벽돌의 내부 옆면과 벽면 뒤의 중복 면을 없애 반투명 상태에서 진하게 겹쳐 보이는 현상을 줄였다.

바닥에는 자체 코드로 만든 낮은 돌판(약 2–4cm)과 작은 자갈(최대 10cm)을 벽 주변 위주로 배치한다. 벽돌 세트는 3.5cm 이하의 타일 파편을 쓴다. 한 메시로 합치고 이동 충돌·바닥 클릭을 방해하지 않으며 문·계단·기물 주변은 비워 둔다. 물웅덩이는 벽까지 이어지는 모양을 바닥 경계에서 잘라 표시한다.

2026-09-18 벽돌 매핑 보정: 벽돌 내부에 옆 줄눈이 세로 띠처럼 반복되던 잘라 쓰기 좌표를 바로잡고, 각 블록의 깨진 가장자리를 포함하도록 UV를 맞췄다. 블록별 좌우 반전으로 물자국 반복을 줄였으며 줄눈 너비는 7mm에서 16mm로 넓혔다. 벽돌 벽의 색상 텍스처 범프 강도는 0.09에서 0.012로 낮춰 물자국 명암이 깊은 홈처럼 보이지 않게 했다. 짧게 잘린 끝 블록은 줄눈 여백을 제한해 면이 뒤집히지 않도록 한다. 메시·재질 수정 출처: 프로젝트 자체 코드, 프로젝트와 동일 라이선스. 추가 AI 이미지 생성 없음.

[벽돌 매핑 수정 비교](../devlog/images/dungeon-masonry-mapping-v8.webp) — 2026-09-18 자체 캡처, 프로젝트 소유 문서 이미지. 실제 벽 메시를 같은 텍스처·카메라·조명으로 렌더해 변경 전, 요철만 완화, 매핑·줄눈까지 수정한 결과를 비교한다. 파란 캐릭터 마커는 자체 코드로 만든 도형이며 게임 플레이 스크린샷은 아니다. 위 풍화 텍스처 적용 미리보기도 현재 매핑으로 갱신했다.

2026-09-18 반투명 시야 보정: 캐릭터를 가리는 석재 블록 벽은 입체 메시를 숨기고 옆면·윗면·뒷면 두께가 없는 평면으로 교체한다. 양쪽에서 보이지만 어느 쪽에서도 한 겹만 그리며 불투명도는 50%에서 22%로 낮췄다. 가림이 해제되면 원래 입체 벽으로 돌아간다. 일반 벽은 블록별로 고정된 작은 밝기·색조 차이를 주어 같은 무늬의 반복감을 줄였다. 모두 프로젝트 자체 코드, 프로젝트와 동일 라이선스이며 추가 이미지 생성은 없다.

[반투명 시야 보정 비교](../devlog/images/dungeon-masonry-ghost-v9.webp) — 2026-09-18 자체 캡처, 프로젝트 소유 문서 이미지. 왼쪽은 기존 입체 벽의 50% 반투명 상태, 가운데는 평면의 22% 반투명 상태이며 두 장면은 같은 카메라·조명을 쓴다. 오른쪽은 안쪽에서 본 일반 벽의 블록별 색조 차이다. 자체 캐릭터 마커를 사용한 별도 Three.js 장면이며 게임 플레이 스크린샷은 아니다.

벽돌 세트의 바닥은 하나로 합친 연속된 평면이며, 온전한 타일과 줄눈은 텍스처로 표현한다. 바닥 범프와 타일별 내부 옆면을 없애 횃불 움직임에 따른 줄눈 음영 변화를 줄였다. 텍스처 줄눈에 맞춘 50cm 타일 단위로 일부가 빠지거나 모서리가 깨지고 갈라진 모습은 바닥 위 6mm의 평평한 흙 패치와 약 1–3cm의 파편으로 덧그린다. 파손 장식은 별도 메시로 합치고 그림자를 드리우거나 바닥 클릭에 관여하지 않는다. 문·계단·기물 주변은 온전하게 유지한다. 흙 재질은 기존 [Poly Haven red_laterite_soil_stones](https://polyhaven.com/a/red_laterite_soil_stones) (CC0, [environment.md](environment.md))을 재사용한다. 각진 벽과 파손 타일 메시 출처: 프로젝트 자체 코드, 프로젝트와 동일 라이선스 (2026-09-18). 추가 이미지 생성 없음.

**[미사용·6차 시안]** [벽돌 돌출 밀도·반투명 가림 비교](../devlog/images/dungeon-masonry-fade-v6.webp) — 2026-09-18 자체 캡처, 프로젝트 소유 문서 이미지. 이전·수정 메시를 같은 카메라와 실제 반투명도 50%로 별도 Three.js 장면에서 비교했다. 캐릭터 대신 코드로 만든 마커를 두었으며 게임 플레이 스크린샷은 아니다. 입체 벽을 그대로 반투명하게 그렸던 이전 방식이다.

**[미사용·5차 시안]** [각진 벽돌·파손 타일 미리보기](../devlog/images/dungeon-masonry-damage-v5.webp) — 2026-09-18 자체 캡처, 프로젝트 소유 문서 이미지. 돌출 벽돌이 많고 반투명 상태에서 내부 면이 겹쳤던 이전 모습.

**[미사용·4차 시안]** [벽 돌출·함몰 미리보기](../devlog/images/dungeon-wall-relief-v4.webp) — 2026-09-18 자체 캡처, 프로젝트 소유 문서 이미지. 벽돌 벽에도 완만한 굴곡을 썼던 이전 모습.

**[미사용·3차 시안]** [수직 벽·둥근 바닥 접점 미리보기](../devlog/images/dungeon-wall-base-v3.webp) — 2026-09-17 자체 캡처, 프로젝트 소유 문서 이미지. 벽 중간에 돌출·함몰을 넣기 전 모습.

**[미사용·2차 시안]** [바닥·물웅덩이 미리보기](../devlog/images/dungeon-floor-detail-v2.webp) — 2026-09-17 자체 캡처, 프로젝트 소유 문서 이미지. 벽 상부가 넓었던 이전 단면.

**[미사용·1차 시안]** [최초 자연 암반 3세트 미리보기](../devlog/images/dungeon-cave-sets.webp) — 2026-09-17 자체 캡처. 셰일 세트 교체와 바닥 장식·물웅덩이 변경 전 모습. 포함된 기존 방 재질은 위 Poly Haven CC0 출처를 따른다.

## Research

Not used right now, but for future reference.

- Procedural Northern European French Town in Geometry nodes Blender 5.0 — **[미사용]** 참고용, 아직 미도입
  https://github.com/IRCSS/Blender-Geometry-Node-French-Houses

## Ysolde's excavation trowel / 이졸데의 발굴용 모종삽 (2026-10-24)

- 출처: 프로젝트에서 직접 만든 Blender 메시와 NumPy 절차적 텍스처. 프로젝트와 같은 라이선스. 로컬 Blender 5.2.0 LTS 사용, 유료 생성 호출 없음.
- 길이 약 **31.8cm**, 나무 손잡이·금속 테·오목하고 뾰족한 철제 삽날. 손잡이 중앙이 원점이며 길이 방향은 glTF +X다. **792 triangles**, 256² 색상 텍스처 3개를 JPEG q92로 패킹했다.
- 원본: `assets/items/excavation_trowel/excavation_trowel.blend`, 제작 기록 `provenance.json`. 게임 파일: `client/public/models/props/excavation_trowel.glb`, 약 75KB. 별도 판매 아이템이 아닌 이졸데의 작업 소품이다.
- `tools/blender-scripts/build_excavation_trowel.py`로 재현한 뒤 `.venv/bin/python tools/repack-glb-textures.py client/public/models/props/excavation_trowel.glb`로 압축한다. 기존 캐릭터 메시·텍스처는 변경하지 않는다.
- 오른손에 부착하고 기존 여성 캐릭터의 강철 장검 그립을 이식한 발굴 클립의 손가락 자세로 쥐게 한다. 손바닥 관통을 줄이도록 손가락 쪽으로 옮긴 부착 위치는 `RightHand` 로컬 `(0, 0.1, 0.03)m`다(2026-10-25). 손목을 꺾어 삽끝을 땅에 맞추지 않는다(2026-10-24 사용자 지정). 삽날은 주먹의 위쪽·앞쪽을 향하며 넓은 면이 바깥쪽에서 보이도록 자루 축으로 90° 돌렸다. 게임과 이졸데 미리보기는 같은 부착 함수를 쓴다. 대기·이동·발굴 때 보이며 수면·사망·다른 상호작용 때 숨긴다. Khronos glTF Validator 오류·경고 0개.
