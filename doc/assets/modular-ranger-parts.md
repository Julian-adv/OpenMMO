# Modular Ranger — 상의·바지 제작 원화

2026-10-05. 사용자가 선택한 기존 [순찰자 원화](../../client/public/character_concepts/ranger.webp)를 바탕으로 상의 앞면과 뒷면을 제작했다.
상의는 올리브색 리넨 셔츠와 갈색 가죽 조끼를 한 장비로 구성한다. 등판은 기존 원화에 보이지 않아 새로 설계한 후보이며, 사용자 검수 전이다.

- [앞면](../images/characters/modular_human_male_01/parts/ranger/top_ranger-front-v1.png)
- [뒷면](../images/characters/modular_human_male_01/parts/ranger/top_ranger-back-v1.png)

각 파일은 1536×1024 RGBA PNG이고 실제 투명 배경을 포함한다. 한 파일에 한 시점만 넣었으며, Tripo 다중 시점 입력에 각각 제출할 수 있다.
앞면의 나뭇가지 가죽 장식, 올리브색 셔츠, 팔꿈치 아래로 걷은 소매와 짧은 조끼 밑단을 유지했다.
뒷면에는 단순한 상단 나뭇가지 장식, 중앙 봉제선, 허리 곡선 봉제선과 짧은 중앙 밑단 트임을 추가했다.

## 바지 제작 원화

기존 순찰자 원화와 상의 앞면을 참고해 갈색 가죽 바지의 앞면을 그린 뒤, 그 앞면을 기준으로 뒷면을 제작했다.

- [바지 앞면](../images/characters/modular_human_male_01/parts/ranger/pants_ranger-front-v1.png)
- [바지 뒷면](../images/characters/modular_human_male_01/parts/ranger/pants_ranger-back-v1.png)

두 파일은 각각 **1024×1536 RGBA PNG**이며 실제 투명 배경을 포함한다. 한 파일에 한 시점만 담았다.
원화의 짙은 갈색 가죽, 허벅지·무릎의 봉제선과 주름, 갈색 벨트, 원형 문양이 있는 황동 버클과 작은 허리 주머니를 유지했다.
원본에서 보이지 않는 등쪽은 단순한 허리 절개와 중앙 봉제선으로, 부츠 안쪽의 바짓단은 좁은 발목 밑단으로 새로 설계한 후보다.
몸체·상의·조끼 밑단·부츠·단검·칼집은 포함하지 않는다. 벨트와 주머니는 바지 파츠에 속한다.

허리 윗단은 낮게 구성해 바깥에 입는 조끼 아래에 들어가도록 의도했다. 실제 생성 모델에서는 조끼와 허리 주머니의 간섭을 함께 검수한다.

2026-10-07: 판금 상의와 조합할 때만 바지 허리를 런타임에서 절단한다.
기준 자세 앞쪽 Y=1.06m를 유지하고, 뒤쪽은 판금 밑단의 V자 형태를 따라 중앙 Y=0.97m까지 내린다.
UV와 스킨 가중치를 보간하며, 다른 상의로 바꾸면 원본 형상을 복원한다. 원본 GLB는 유지한다.
발목 밑단은 별도 부츠 안에 넣는다. 현행 슬림 종아리 몸체와 `interfaces/v1`에 맞춰 피팅하며, 그림의 실루엣을 치수나 여유 검증으로 취급하지 않는다.
앞뒤는 같은 바지의 참조 이미지지만 봉제선·주머니 위치의 정확한 연결은 3D에서 조정해야 한다. 바지 Tripo 생성 요청은 제출하지 않았다.
이후 사용자가 Tripo 바지 모델을 전달해 [바지 피팅·리깅 후보와 동작 검수](modular-ranger-tripo-pants.md)를 만들었다.

## 파츠 소속과 후속 제작

- 상의: 셔츠·접힌 옷깃·걷은 소매·가죽 조끼·조끼 밑단.
- 하의: 가죽 바지·벨트·버클·허리 주머니.
- 발 장비 별도: 부츠.
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
