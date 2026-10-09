# 남성 사제 하의 — 부위 원화

2026-10-08. 기존 [남성 사제 원화](../../client/public/character_concepts/priest.webp)와
[상의 원화](modular-priest-parts.md)에 맞춘 사슬 바지 정면·후면 v1이다.
**[미사용]** 검토용 원화이며 3D 제작·피팅·게임 등록 전이다.

| 시점 | 파일 |
| --- | --- |
| 정면 v1 | [원화](../images/characters/modular_human_male_01/parts/priest/pants_priest-front-v1.png) |
| 후면 v1 | [원화](../images/characters/modular_human_male_01/parts/priest/pants_priest-back-v1.png) |

각각 1024×1536 RGB PNG, 흰 배경에 한 시점만 담았다. 후면은 정면을 참조한 제작 제안이다.

같은 날 사용자가 Tripo 하의를 전달했다. [원본 보관·피팅과 제작실 검수](modular-priest-tripo-pants.md).

## 파츠와 연결

- 제안 슬롯: `legs`, 제작 식별자: `pants_priest`.
- 허리부터 발목까지 짙은 은색 사슬 바지, 얇은 어두운 허리 마감과 작은 앞 여밈을 포함한다.
- 흰 성의·늘어지는 옷자락·외부 벨트·성물 장식은 상의 소속이다. 신발·발·인체는 포함하지 않는다.
- 허리는 성의 안쪽에 들어간다. 발목은 열린 좁은 끝단이며 발등을 덮지 않는다.
  원본 캐릭터의 사슬 발등 부분은 이번 바지 원화에 포함하지 않았다.
- 향후 피팅 기준은 현재 `assets/modular_human_male_01/parts/fitted/base.glb`,
  `human_male_01_mixamo_candidate_v2` 65본이다.
  몸체는 이미지 생성 입력이 아니며, `interfaces/v1`과 현재 몸체의 단면을 피팅 전에 대조한다.
- 겹침 깊이·가림 범위·다른 상의 및 신발과의 호환은 미검증이다.
  실제 치수와 앞뒤 링 크기·허리 높이·끝단 폭은 3D에서 통일한다.

정면의 두 바지통·허리 앞 여밈·빈 발목 입구와 후면의 엉덩이·무릎 뒤 주름을 확인했다.
후면에는 정면 여밈을 중복하지 않았다. 제작은 [공통 워크플로우](modular-outfit-workflow.md)를 따른다.

## 출처

OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-10-08.
OpenAI 생성 출력물 이용 조건과 [입력 원화 출처](characters.md#other-classes)를 따른다.
[실제 프롬프트·참조·해시](modular-priest-pants-sources.json).
