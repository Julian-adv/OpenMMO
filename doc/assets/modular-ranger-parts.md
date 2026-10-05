# Modular Ranger — 상의 제작 원화

2026-10-05. 사용자가 선택한 기존 [순찰자 원화](../../client/public/character_concepts/ranger.webp)를 바탕으로 상의 앞면과 뒷면을 제작했다.
상의는 올리브색 리넨 셔츠와 갈색 가죽 조끼를 한 장비로 구성한다. 등판은 기존 원화에 보이지 않아 새로 설계한 후보이며, 사용자 검수 전이다.

- [앞면](../images/characters/modular_human_male_01/parts/ranger/top_ranger-front-v1.png)
- [뒷면](../images/characters/modular_human_male_01/parts/ranger/top_ranger-back-v1.png)

각 파일은 1536×1024 RGBA PNG이고 실제 투명 배경을 포함한다. 한 파일에 한 시점만 넣었으며, Tripo 다중 시점 입력에 각각 제출할 수 있다.
앞면의 나뭇가지 가죽 장식, 올리브색 셔츠, 팔꿈치 아래로 걷은 소매와 짧은 조끼 밑단을 유지했다.
뒷면에는 단순한 상단 나뭇가지 장식, 중앙 봉제선, 허리 곡선 봉제선과 짧은 중앙 밑단 트임을 추가했다.

## 파츠 소속과 후속 제작

- 상의: 셔츠·접힌 옷깃·걷은 소매·가죽 조끼·조끼 밑단.
- 하의 설계에 예약: 벨트·버클·허리 주머니.
- 손 장비 설계에 예약: 가죽 팔 보호대·반장갑.
- 무기 별도: 단검·칼집.

몸체와 손은 생성 입력에서 제외했다. 소매와 팔 보호대 사이의 피부 노출을 유지한다.
현재 기준은 `assets/modular_human_male_01/parts/fitted/base.glb`, `interfaces/v1`, 리그 `human_male_01_mixamo_candidate_v2`다.
[제작 워크플로우](modular-outfit-workflow.md)와 [연결 규칙](modular-outfit-connections.md)에 따라 실제 몸체에서 목·소매·허리 여유를 검수한다.
그림을 치수나 피팅 합격 증거로 사용하지 않는다. 상의 생성 목표는 **2,500쿼드, 약 5,000트라이앵글**로 제안한다.
소매·옷깃·밑단에 형상을 배분하고 가죽 결·얕은 나뭇가지 장식은 주로 텍스처와 노멀맵으로 표현한다.
실제 출력과 한 착용 캐릭터 전체의 15,000–20,000트라이앵글 예산을 실측해야 한다.
사용자가 Tripo 생성 모델을 전달해 [상의 피팅 후보와 표본 동작 검수](modular-ranger-tripo-top.md)를 만들었다.
게임의 피부 가림과 최종 혼합 장비 검증은 아직 진행하지 않았다.

## Tripo 기능 확인

2026-10-05 [공식 Segmentation v2 안내](https://www.tripo3d.ai/blog/tripo-segmentation-v2)에서 Model 탭의 **Generate in Parts**와 기존 모델의 **Segment** 기능을 확인했다.
[공식 기능 안내](https://www.tripo3d.ai/help/getting-started/what-features-does-tripo-have)는 2–4장 다중 시점 입력을 설명한다.
생성된 부위의 경계와 옷 내부 형상은 검토가 필요하다. 사용자 계정의 UI·이용 가능 여부는 확인하지 않았으며, Tripo 생성 요청은 제출하지 않았다.

## 출처와 이용 조건

OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-10-05.
OpenAI 생성 출력물 이용 조건과 기존 [캐릭터 원화 출처·이용 조건](characters.md)을 따른다.
원본 ranger 원화는 Gemini 생성 후 ChatGPT Pro 20x로 배경을 투명화한 기존 프로젝트 자료다.
[실제 프롬프트·입력·출력 해시·검수 기록](modular-ranger-parts-sources.json)에 앞뒤 생성 내용을 보관했다.
