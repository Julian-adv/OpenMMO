# 여성 기사 판금 부츠

2026-11-20. [제공받은 원화](../../assets/items/female_knight_boots/reference_v2/user-reference.png)를 보고 허벅지부터 발끝까지 새로 모델링했다. 허벅지의 기울어진 이중 테두리, 무릎을 감싸는 겹판과 양옆 날개, 무릎 뒤에 드러나는 검은 천, 정강이 솔기와 가죽 버클을 형상으로 만들었다. 신발은 두 번째 [측면 원화](../../assets/items/female_knight_boots/reference_v2/shoe-side-reference.png)의 앞코와 밑창 높이, 발볼에서 굽 안쪽까지 이어지는 곡선에 맞춰 모델링했다. 앞코에는 얇은 밑창 테두리를 두고 아치와 바닥은 [밑창 원화](../../assets/items/female_knight_boots/reference_v2/shoe-sole-reference.png)의 짙은 붉은색을 사용했다. 원화의 원저작자와 라이선스는 미확인이다. 메시와 재질은 로컬에서 제작했으며 추가 생성 비용은 없다.

- [게임용 GLB](../../client/public/models/characters/modular_female/boots.glb) · [Blender 편집본](../../assets/items/female_knight_boots/reference_v2/fitted_v1/female-knight-boots-fitted.blend) · [피부 가림](../../assets/items/female_knight_boots/reference_v2/fitted_v1/body-occlusion.json)
- [착용 측면 스케치](../../assets/items/female_knight_boots/reference_v2/ankle-profile-guide.png)에 맞춰 앞코를 발목 쪽으로 약 5cm 당기고 굽 끝을 약 4.3cm 뒤로 옮겼다. 측면 원화와 착용 렌더의 앞코와 굽 끝을 맞춰 발등과 아치를 비교했다. 발등 윗부분의 판 세 장은 옆면을 감싸 밑창 경계에서 끝난다. 맨 위 판은 뒤꿈치 판과 비스듬히 맞물리고, 앞코 판은 부채꼴로 감긴다.
- [뒤축과 앞코의 수정선](../../assets/items/female_knight_boots/reference_v2/heel-profile-guide.png)에 맞춰 뒤판을 높이고 굽의 뒤쪽 곡선을 벌렸다. 발볼에서 코끝까지 밑창 높이 차이는 3mm다. 뒤꿈치의 은색 판과 정강이 사이로 검은 안감이 비스듬히 드러나고, 붉은 아치는 뒤쪽에서 금색 굽으로 이어진다. 기사 부츠는 이 밑창으로 동작별 접지 높이를 계산한다. 다른 굽 신발은 [공용 접지 표본](../../assets/modular_human_female_01/rig_v1/heel-ground-samples.glb)의 발 위치와 관절 가중치로 높이를 계산한다. 이 표본과 [샌들 기준 높이](../../assets/modular_human_female_01/rig_v1/heel-ground.json)는 2026-10-13 Tripo 판금 부츠에서 추출했다. [생성 기록](../../assets/items/female_knight_boots/tripo_v1/generation.json) 뒤꿈치 판의 옆면과 뒷면은 한 곡면에서 이어지며 금속 테두리와 리벳은 별도로 음영을 계산한다.
- 허벅지 피부는 좌우 부츠 윗단 안쪽의 경사면에서 잘라낸다. 경계를 가로지르는 피부 삼각형은 면에서 나누고, 부츠를 벗으면 원래 피부가 복원된다.
- 허벅지부터 발등까지 은색 판은 흉갑 앞면과 같은 색·거칠기·금속성 텍스처를 쓴다.
- 한 쌍은 23,352삼각형이며 좌우 메시가 재질 하나를 공유한다. 기본 여성 몸체·단발·판금 5부위·시작 철검·횃불을 합하면 숨김 포함 71,182삼각형, 착용 시 표시 54,281삼각형이다. 얼굴 영역은 4,604삼각형이다.
- 무릎 앞판과 옆 날개를 아래쪽 가장자리를 축으로 허벅지 쪽으로 16도 돌렸다. 윗겹판은 허벅지를, 아랫겹판은 정강이를 따른다. 좌우 무릎 보호대에는 무릎과 같은 위치의 전용 본을 하나씩 쓰며 허벅지 기준 종아리 회전의 절반을 적용해 굽힘 여유가 양쪽에 분산된다. 각 금속판은 뼈 하나를 따르고 무릎과 발목의 천에만 가중치를 섞었다. 기존 65본과 30도 굽 자세를 사용한다. 14개 동작에서 동작당 61개 시점의 판금 변형과 접지를 검사했다. [검사 기록](../../assets/items/female_knight_boots/reference_v2/fitted_v1/runtime-validation.json)

## 재현

```bash
.venv/bin/python tools/build-female-knight-boots.py
node tools/prepare-female-knight.mjs --part boots
node tools/prepare-female-knight.mjs --bake-existing
node tools/validate-female-knight-boots.mjs
blender -b --python-exit-code 1 -P tools/blender-scripts/review_female_knight_boots.py -- assets/items/female_knight_boots/reference_v2/fitted_v1 --pack-only
```
