# 남성 사제 앵클부츠 — 왼발 부위 원화

2026-10-09. 사용자가 사슬 바지와 샌들 조합을 재검토하고,
발끝·뒤꿈치를 덮는 갈색 가죽 앵클부츠와 은색 버클 하나로 변경하는 방향을 승인했다.
[이전 샌들](modular-priest-sandals.md)을 대체하는 확정 원화다.
원화는 아래 Tripo 부츠의 디자인 참조로 사용했다. 게임 출시 장비 등록은 아직 하지 않았다.

같은 날 사용자 제공 Tripo 부츠 원본을 회수했다. 한 짝 1,112 triangles이며
입구 안쪽을 보정한 뒤 좌우 피팅·리깅과 미리보기 연결을 마쳤다. [원본 검수와 현재 피팅](modular-priest-tripo-boots.md).

| 시점 | 파일 | 크기 |
| --- | --- | --- |
| 정면 | [원화](../images/characters/modular_human_male_01/parts/priest/boot_priest_left-front-v1.png) | 1024×1536 |
| 왼발 바깥쪽 측면 | [원화](../images/characters/modular_human_male_01/parts/priest/boot_priest_left-side-v1.png) | 1536×1024 |

흰 배경 RGB PNG이며 한 장에 한 시점만 담았다. 기존 사제 원화는 갈색 가죽·은색 금속의
참조로 사용했고, 부츠 형태는 승인된 새 디자인이다. 측면은 생성된 정면을 참조했다.
둥근 막힌 앞코·가죽 갑피·짧은 부츠목·낮은 굽을 두고, 버클은 착용자 기준 왼발 바깥쪽에 하나만 둔다.
정면은 입구가 조금 보이는 각도이며 치수용 정투영 도면은 아니다.

## 제작과 연결

제안 슬롯은 `feet`, 제작 식별자는 `boot_priest_left`다.
왼발 한 짝을 제작하고 오른발은 3D에서 대칭 복제한다.
복제 시 발·발가락 본 대응과 면 방향·노멀을 보정한다.

기준 몸체는 `assets/modular_human_male_01/fitted/base.glb`,
SHA-256 `0e629865af6c3feac3a4444e0cb2d5f2d3861bf2350d643cbff0f9858b83535a`,
리그는 `human_male_01_mixamo_candidate_v2`다.
몸체는 이미지 생성 입력에 포함하지 않았다. 부츠목과 발바닥의 실제 비율·봉제선 연결은
모델링에서 두 시점을 대조해 확정한다.

발목의 `shoe_ankle` 연결 영역과 최신 몸체·`interfaces/v1`을 대조한다.
사슬 바지 밑단은 부츠 안쪽에 넣으며, 입구 안쪽 여유와 겹침·피부/바지 가림 깊이는
실제 피팅에서 정한다. 원화에는 발·바지를 넣지 않았다.
착탈·표본 동작 검사를 마쳤고, 사용자가 모든 바지와의 피팅을 확인했다.
상세 검수는 [Tripo 부츠 기록](modular-priest-tripo-boots.md)을 따른다.
후속 제작은 [공통 워크플로우](modular-outfit-workflow.md)를 따른다.

## 출처

OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-10-09.
OpenAI 생성 출력물 이용 조건과 [입력 원화 출처](characters.md#other-classes)를 따른다.
[실제 생성 프롬프트·참조·해시·검토 기록](modular-priest-boots-sources.json).
