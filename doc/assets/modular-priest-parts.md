# 남성 사제 상의 — 부위 원화

2026-10-08. 기존 [남성 사제 원화](../../client/public/character_concepts/priest.webp)에서
상의 한 벌을 분리해 정면·후면을 각각 생성했다. 현재 원화는 **v2**다.
사용자 요청으로 허리 아래 천 사이에 보이던 사슬을 앞뒤 모두 제거했다. **[미사용]** 검토용 원화이며 3D 제작·피팅·게임 등록 전이다.

| 시점 | 파일 | 상태 |
| --- | --- | --- |
| 정면 v2 | [원화](../images/characters/modular_human_male_01/parts/priest/top_priest-front-v2.png) | 기존 원화의 디자인을 기준으로 분리 |
| 후면 v2 | [원화](../images/characters/modular_human_male_01/parts/priest/top_priest-back-v2.png) | 정면을 참조한 후면 제안; 원본 후면 자료 없음 |

두 이미지 모두 1024×1536 RGB PNG, 흰 배경이다. 각 이미지에 한 시점만 담았다.

같은 날 사용자가 Tripo 상의 메시를 전달했다. **3,913 triangles** 원본을 보관하고
현재 공통 몸체·65본에 착용 후보를 만들었다. [원본 검수·피팅과 남은 천 물리 작업](modular-priest-tripo-top.md).

## 파츠 소속과 연결

- 제안 슬롯: `chest`, 제작 식별자 `top_priest`.
- 포함: 무릎 위까지 내려오는 흰색·금색 성의 전체, 긴 사슬 소매·옷깃,
  은색 목걸이, 갈색 이중 벨트와 은색 장식·붉은 끈.
- 허리 아래 성의도 상의 소속이다. 허리선에서 외형을 자르지 않는다.
- 제외: 사제관, 손·장갑, 사슬 바지, 샌들, 인체.
- 기준 몸체: `assets/modular_human_male_01/fitted/base.glb`,
  SHA-256 `0e629865af6c3feac3a4444e0cb2d5f2d3861bf2350d643cbff0f9858b83535a`.
  리그는 `human_male_01_mixamo_candidate_v2`, 기존 65본이다.
  몸체 메시를 이미지 생성 입력으로 사용하지 않았으므로 그림의 비율은 실제 피팅 결과가 아니다.
- `interfaces/v1`은 현재 몸체와 기록된 해시가 다르다. 피팅 전 최신 몸체의 목·허리·손목 단면과 대조한다.
- 바지 허리는 성의 안쪽에 들어간다. 소매 끝은 `glove_short` 기준으로 손목 장비와 겹침을 검토한다.
  겹침 깊이·피부 가림·안쪽 의상 가림·대체 옷깃·혼합 조합은 아직 미확정·미검증이다.

## 옷자락 제작 방향

몸통·소매는 기존 본을 따라 변형하고, 허리 아래는 위쪽을 고정한 작은 천 격자로 움직이는 방식을 제안한다.
앞트임과 후면 제안의 짧은 트임을 활용해 다리 움직임의 여유를 둔다.
[현재 원시전사 하의](modular-caveman-tripo-pants.md)는 야만용사 런타임을 공유하며
앞뒤는 허리 힌지로 회전하고 양옆만 99입자 천으로 계산한다. 사제의 앞뒤까지 같은 판 회전으로
처리하면 뻣뻣해 보일 수 있어 천의 굽힘과 조각 사이 연결을 별도로 검토해야 한다.
기존 모피 물리는 출발점이며 연속 로브에 그대로 적용한 완성 구현은 아니다.

v2는 허리 아래 사슬 셔츠 밑단을 없애고 흰 옷자락만 남긴다. 앞뒤 트임의 흰 공간은
배경이 보이는 열린 틈이며 흰 안감으로 메우지 않는다. 소매·목깃의 사슬과 벨트 장식용 체인은 유지한다.
허리 고정 구간의 연결과 다리 충돌,
대기·걷기·달리기·점프·공격·앉기, 다른 바지·장갑과의 조합, 여러 캐릭터의 CPU 비용을 확인해야 한다.
현재 원화로 동작·성능을 검증하지 않았다.

## 원화 검토와 남은 작업

정면의 흰 성의·금색 테두리·사슬 소매·은색 장식과 빈 목·손목 입구를 확인했다.
후면은 단순한 흰 천과 봉제선, 벨트 연결부를 제안한다. 앞면의 가슴 장식을 뒤에 중복하지 않았다.
후면의 금색 옷깃과 앞면의 옷깃 연결 형태는 모델링 전에 하나로 확정해야 한다.
**[미사용]** v1 앞뒤 원화 파일은 중간 산출물 정리 요청에 따라 삭제하고 프롬프트·해시만 출처 기록에 남겼다. v2에서 앞뒤 트임의 사슬 제거와
소매·목깃 사슬 및 벨트 체인 유지를 시각 확인했다.
원화 검토 후 조립 캐릭터의 15,000–20,000 triangles 목표 안에서 몸체·얼굴·다른 장비와 예산을 배분한다.
3D 생성, 피팅, 물리, 가림과 게임 호환 검토는 [공통 워크플로우](modular-outfit-workflow.md)를 따른다.

## 출처

OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-10-08.
OpenAI 생성 출력물 이용 조건과 [입력 원화 출처](characters.md#other-classes)를 따른다.
[실제 생성 프롬프트·참조·해시·검토 기록](modular-priest-parts-sources.json).

## 시작 장비 연결 — 2026-10-09

남성 사제 생성 시 `worn_priest_top`, `worn_priest_pants`, `worn_priest_boots`를
각각 상의·바지·부츠 슬롯에 착용한 상태로 지급한다. 생성 화면도 같은 장비를 표시한다.
기존 시작 방어구처럼 각 Guard 1, 가격 없음, 거래 불가, 던전 상자 드롭 없음으로 등록했다.
여성 사제는 기존 단일 모델을 사용한다.

착용 모델의 출처와 이용 조건은 [상의](modular-priest-tripo-top.md),
[바지](modular-priest-tripo-pants.md), [부츠](modular-priest-tripo-boots.md)의
사용자 제공 Tripo 모델 기록을 따른다. 새 AI 생성은 없으며 원본 구독 등급·생성일의
미확인 상태도 그대로 유지한다.

- `tools/prepare-modular-character.mjs --priest-only`로 기존 피팅 GLB를 압축해
  `client/public/models/characters/modular_male/{top,pants,boots}_priest.glb`를 만든다.
  입력·출력 SHA-256은 같은 디렉터리의 `manifest.json`에 기록한다.
- `blender -b -t 6 --python-exit-code 1 --python tools/blender-scripts/export_priest_items.py`로
  같은 피팅 모델에서 정적 아이템 모델과 투명 128×128 아이콘을 만든다.
  출력은 `client/public/models/armor/priest_{top,pants,boots}.glb` 및
  `client/public/items/armor/priest_{top,pants,boots}.png`다.
  텍스처는 최대 512px, 편집본은 `assets/items/priest_{top,pants,boots}/`에 보관한다.
- 기존 사제 복장 가림·옷자락·혼합 장비 처리를 게임 장비 경로에서도 사용하며,
  사제 부츠의 지면 보정값은 내보낸 모델에서 측정한다.
