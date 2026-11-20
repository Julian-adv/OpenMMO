# 여성 기사 하의

2026-11-20. 사용자가 제공한 원화를 기준으로 하의 메시와 재질을 새로 제작했다. 채택본은 **reference_v1/fitted_v1**이다.

허벅지 바깥을 감싸는 판을 좌우 다섯 장씩 겹쳤다. 위쪽 두 단의 사선 이음매, 아래에서 꺾이는 모서리, 판 끝의 리벳을 원화에 맞췄다. 허리 앞에서 교차하는 가죽띠와 부츠 고리까지 내려오는 끈, 둥근 덮개 아래로 가늘어지는 중앙 보호판을 각각 모델링했다. 앞뒤 끈의 끝과 황동 리벳은 부츠 윗단 곡면을 따르며 같은 쪽 허벅지 뼈에 고정했다. 뒤쪽 황동 고리는 윗단 가장자리에 걸치고 둥근 가죽 끝은 그 아래 16mm까지 내려온다. 뒤쪽 가죽끈은 겹판의 곡면을 따라 밀착하고, 골반과 허벅지가 따로 움직이는 부분은 황동 고리 아래에서 겹쳐 연결한다.

허리띠와 옆판의 바깥선을 [사용자가 표시한 곡선](../../assets/items/female_knight_bottom/reference_v1/waist-curve-guide.png)에 맞췄다. 앞 교차띠와 등 뒤 허리띠의 깊이를 줄이고, 앞뒤 세로끈의 윗끝과 리벳을 새 허리띠 곡면에 맞췄다. 위쪽 두 판은 골반에 고정하고, 아래 세 판은 대기 자세에서 윗판 안쪽으로 포개지도록 허벅지 관절을 기준으로 회전시켰다.

앞 가림판의 둥근 덮개를 아랫단 기준으로 4.5도 앞으로 돌리고, 교차띠 중앙과 리벳도 함께 띄워 피부 위로 금속 윗선이 보이게 했다. 앞뒤 중앙 가림판은 윗단을 기준으로 세로로 16% 늘렸다. 앞판 끝은 24.5mm, 뒷판 끝은 22.0mm 내려왔다. 안감은 통째로 18mm 위에 달았다. 판 사이로 보이는 속옷은 기준 몸체의 속옷 1,110삼각형과 관절 가중치를 그대로 사용한다. 허리띠와 같은 갈색 가죽 텍스처와 거칠기·금속성 맵을 공유한다.

- [편집용 Blender](../../assets/items/female_knight_bottom/reference_v1/fitted_v1/female-knight-bottom-fitted.blend) · [착용 GLB](../../assets/items/female_knight_bottom/reference_v1/fitted_v1/bottom_female_knight.glb) · [몸체 가림](../../assets/items/female_knight_bottom/reference_v1/fitted_v1/body-occlusion.json)
- 금속판은 장마다 골반 또는 같은 쪽 허벅지 뼈 하나를 따른다. 걷기·달리기·점프·앉고 일어서기·베기 등 11개 동작의 671개 시점에서 판 가장자리 길이 변화를 검사했다. [게임용 모델 검사 결과](../../assets/items/female_knight_bottom/reference_v1/fitted_v1/runtime-validation.json)
- 게임용 하의는 10,653삼각형, 재질 1개, 468,496바이트다. 기본 여성 몸체·단발·판금 5부위·시작 철검·횃불 조합은 숨김 포함 71,182삼각형, 착용 시 표시 54,281삼각형이다. 얼굴에 4,604삼각형을 사용한다.
- [사용자가 제공한 원화](../../assets/items/female_knight_bottom/reference_v1/user-reference.png)의 원저작자와 라이선스는 미확인이다. 메시와 UV는 로컬에서 제작했다. 은색 판금은 흉갑 앞면에서 채취한 색·거칠기·금속성 텍스처를 사용한다. 추가 생성 비용은 0이다. 몸체와 골격, 가림 인덱스는 기존 여성 기사의 자산을 사용한다.

## 재현

```bash
.venv/bin/python tools/build-female-knight-tassets.py
blender -b --python-exit-code 1 -P tools/blender-scripts/pack_female_knight_plate.py -- bottom
node tools/prepare-female-knight.mjs --part bottom
node tools/validate-female-knight-bottom.mjs
```

골격과 역바인드 행렬은 여성 기준 몸체에서 읽고, 속옷 삼각형은 현재 하의의 몸체 가림 파일에서 읽는다.
