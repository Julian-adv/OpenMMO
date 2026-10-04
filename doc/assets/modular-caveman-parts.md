# Modular Caveman — Tripo용 부위 원화

2026-10-03 제작. [부츠를 신긴 전체 원화 v2](modular-caveman-concept.md)를 참고해
상의·손·하의·부츠의 정면과 보조 시점을 각각 한 장씩, 총 8장으로 다시 그렸다.
2026-10-04에는 기존 앞·뒷면을 참조한 왼쪽 부츠 외측면 1장을 추가했다.
사용자 요청으로 상의 보조 시점을 뒷면에서 오른쪽 사선 측면으로 변경했다.
측면 v2는 어깨가 들어가는 둥근 빈 공간을 아래로 열고, 정면처럼 펠트를 화면 왼쪽에 배치했다.
**[미사용]** 게임 적용 전 원화다. 사용자가 [Tripo 상의 원본](modular-caveman-tripo-top.md)을 전달했고,
2,110 triangles와 어깨 안쪽 공간을 확인했다. 2026-10-04 상의를 현재 몸체에 피팅하고 기존 65본에 연결했다.
사용자가 선택한 착용 형태를 저장했으며 게임 등록과 다른 부위 모델 제작은 후속 작업이다.
같은 날 [Tripo 하의 원본](modular-caveman-tripo-pants.md)을 전달받아
4,960 triangles와 열린 허리 입구를 확인했다. 현재 몸체에 피팅하고 앞·뒤·양옆 모피를
분리해 골반에 고정했다. 모피는 독립적으로 흔들리며 아래쪽으로 완만하게 휘어진다.
같은 날 [Tripo 부츠](modular-caveman-tripo-boots.md) 한쪽 2,059 triangles를 전달받아
발·종아리에 피팅하고 반대쪽을 대칭 제작했다. 양쪽 5,178 triangles를 공통 리그에 연결해
상의·하의와 함께 미리보기에서 착용한다. 게임 장비 등록은 후속 작업이다.
같은 날 [Tripo 손목 보호대 원본](modular-caveman-tripo-bracer.md)도 전달받았다.
한쪽 1,814 triangles이며 모피 입구와 가죽 통·세 줄 끈을 확인했다. 실제 팔 단면에 피팅해
양쪽 3,628 triangles로 대칭 제작하고 공통 ForeArm 본에 연결했다. 원시전사 미리보기에서
상의·하의·부츠와 함께 착용하며 손과 손가락은 노출한다. 7개 동작을 검사했고 사용자가 착용 결과를 승인했다.

## 생성용 파일

| 파트 | 앞면 | 보조 시점 | 포함 범위 |
| --- | --- | --- | --- |
| 상의 | [앞면](../images/characters/modular_human_male_01/parts/caveman/top_caveman-front-v1.png) | [사선 측면 v2](../images/characters/modular_human_male_01/parts/caveman/top_caveman-side-v2.png) | 착용자 오른쪽 모피 어깨 장식, 뼈 장식, 목끈·목걸이 |
| 손 | [앞면](../images/characters/modular_human_male_01/parts/caveman/bracer_caveman_right-front-v1.png) | [뒷면](../images/characters/modular_human_male_01/parts/caveman/bracer_caveman_right-back-v1.png) | 오른쪽 손목·전완 보호대 1개, 양끝 모피와 가죽 끈 |
| 하의 | [앞면](../images/characters/modular_human_male_01/parts/caveman/pants_caveman-front-v1.png) | [뒷면](../images/characters/modular_human_male_01/parts/caveman/pants_caveman-back-v1.png) | 허리띠·뼈 장식·앞뒤 가죽 패널·옆 모피 |
| 부츠 | [앞면](../images/characters/modular_human_male_01/parts/caveman/boot_caveman_left-front-v1.png) | [뒷면](../images/characters/modular_human_male_01/parts/caveman/boot_caveman_left-back-v1.png) · [외측면](../images/characters/modular_human_male_01/parts/caveman/boot_caveman_left-side-v1.png) | 왼쪽 부츠 1개, 종아리 모피 커프·가죽 끈·밑창 |

[선택한 8장 묶음 ZIP v3](../images/characters/modular_human_male_01/parts/caveman/caveman-tripo-inputs-v3.zip).
추가 부츠 측면은 별도 이미지이며 기존 ZIP에는 포함되지 않는다.
각 이미지에는 같은 파트의 한 시점만 담았다. 인체·다른 장비·문자는 제외했다.
손목 보호대와 부츠는 한쪽을 생성한 뒤 반대쪽을 대칭 제작하고 실제 몸체에 맞춘다.
손 장비는 장갑이 아니며 손과 손가락이 노출된다.

- 상의: 정면 1222×1287, 측면 1254×1254 RGBA. 하의: 앞뒤 1145×1374 RGBA. 모두 투명 배경이다.
- 손목 보호대와 부츠: 1024×1536 RGB, 앞뒤 및 추가 부츠 측면 모두 흰 배경이다.
  초기 투명화 결과의 갈색 배경 번짐 때문에 단색 배경으로 수정했다.
- 부츠 첫 후면은 앞코가 보이는 오류로 미채택했다. 선택 후면은 뒤꿈치와 뒷축 봉제선을 보여준다.
- 초기 배경 수정용 3장과 빈 어깨 공간을 수정한 중간 측면 1장은 중간 파일 정리 요청으로 삭제했다.
- **[미사용]** 기존 상의 후면은 형태가 어색해 측면으로 대체했고, 측면 v1은 어깨 공간과 좌우 배치를 수정했다.
  두 미채택 이미지와 ZIP v1·v2는 삭제하고 생성 프롬프트·해시·교체 사유를 기록했다.
  최신 ZIP v3에는 정면·측면 v2와 나머지 부위의 앞뒤 원화를 보관한다.

## 조립 시 확인

오른쪽 어깨 장식은 정면과 선택한 측면 모두 화면 왼쪽에 보인다. 목걸이는 측면 화면 오른쪽이다.
펠트는 얇은 가죽 안감 위에 모피를 얹은 곡면 덮개다. 옆에서 보면 아래로 열린 ∩자 형태이며,
중앙의 둥근 오목한 공간에 어깨가 들어간다. 삼각형 가죽 판으로 내부를 막지 않는다.
상의는 정면의 장식 배치와 측면 v2의 빈 공간·깊이를 기준으로 하나의 메시 구조를 확정한다.
기존 후면의 추가 목 뒤 장식은 이번 선택 원화에 강제하지 않는다.
하의·손목 보호대·부츠의 앞뒤 봉제선·끈 연결은 모델링 때 하나로 맞춘다.
허리 장식은 하의, 종아리 모피는 부츠 소속이다.
원화의 픽셀을 실제 치수로 사용하지 않으며 [공통 연결 규칙](modular-outfit-connections.md)을 따라
입구·피부 노출·겹침·가중치를 검토한다.

## 출처

- 생성·수정: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-10-03. 추가 부츠 측면: 같은 도구·등급, 2026-10-04.
- OpenAI 생성 출력물 이용 조건과 [입력 원화의 출처](modular-caveman-concept-sources.json)를 따른다.
- [모든 실제 프롬프트·참조·선택/미채택 기록·파일 해시](modular-caveman-parts-sources.json).
