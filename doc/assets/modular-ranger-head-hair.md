# 순찰자 얼굴·헤어 제작 원화

2026-10-07. 기존 [순찰자 원화](../../client/public/character_concepts/ranger.webp)의 얼굴과 헤어를
Tripo 제작용으로 분리했다. 한 파일에 한 파츠의 한 시점만 담았다.
모든 파일은 **1024×1536 RGBA PNG**이며 실제 투명 배경을 포함한다.

| 파츠 | 시점 | 입력 파일 |
| --- | --- | --- |
| 얼굴·두상 | 정면, 우선 시점 | [head_ranger-front-v1.png](../images/characters/modular_human_male_01/parts/ranger/head_ranger-front-v1.png) |
| 얼굴·두상 | 화면 왼쪽을 보는 측면 | [head_ranger-side-v1.png](../images/characters/modular_human_male_01/parts/ranger/head_ranger-side-v1.png) |
| 헤어 | 정면, 우선 시점 | [hair_ranger-front-v1.png](../images/characters/modular_human_male_01/parts/ranger/hair_ranger-front-v1.png) |
| 헤어 | 화면 왼쪽을 보는 측면 | [hair_ranger-side-v1.png](../images/characters/modular_human_male_01/parts/ranger/hair_ranger-side-v1.png) |
| 헤어 | 뒷면 | [hair_ranger-back-v1.png](../images/characters/modular_human_male_01/parts/ranger/hair_ranger-back-v1.png) |

얼굴은 각진 턱과 광대, 회녹색 눈, 짙은 눈썹, 옅은 콧수염·턱수염을 유지했다.
헤어를 별도로 제작할 수 있도록 두피의 머리카락을 제거하고 귀·두상·짧은 목까지 그렸다.
몸통·어깨·의상은 포함하지 않는다.

헤어는 짙은 밤색, 중앙에 가까운 가르마, 뒤로 넘긴 정수리 볼륨, 얼굴 양옆으로 내려오는 가는 머리 다발과
목 아래까지 이어지는 느슨한 웨이브를 유지했다. 얼굴·피부·귀·목·마네킹은 포함하지 않는다.
원화에 보이지 않는 두상과 측면, 헤어 뒷면은 정면을 바탕으로 보완한 후보이며 사용자 검수 전이다.

Tripo에서는 얼굴 정면·측면을 한 제작 묶음으로, 헤어 정면·측면·뒷면을 별도 묶음으로 사용한다.
정면을 주 기준으로 삼는다. 시점은 생성 원화이므로 정확한 직교 투영이나 실제 치수를 보장하지 않는다.
후속 3D 작업에서 현재 남성 몸체의 목 연결부, 헤어의 두상 여유, 귀 주변과 옷깃·뒷머리 간섭을 맞춘다.
가는 모발 표현은 큰 머리 다발의 표면 디테일로 처리하고, 토폴로지·리깅·동작 호환은 별도로 검수한다.
원화 제작 단계에서는 Tripo 생성 요청, 3D 피팅, 게임 모델 교체를 진행하지 않았다.
2026-10-08 사용자 전달 헤어를 [기존 두상에 피팅해 제작 미리보기](modular-ranger-tripo-hair.md)에 연결했다.

생성 도구는 OpenAI Codex built-in ImageGen, 등급은 **ChatGPT Pro 20x**다.
기존 원화는 Gemini 생성 후 ChatGPT Pro 20x로 배경을 투명화한 2026-08-28 기록을 따른다.
OpenAI 생성 출력물 이용 조건과 [기존 캐릭터 원화의 출처·이용 조건](characters.md)을 따른다.
실제 프롬프트·입력 목록·SHA-256·투명도 검수는 [출처 기록](modular-ranger-head-hair-sources.json)에 있다.
