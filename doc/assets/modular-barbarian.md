# Modular Barbarian — 남성 바바리안 복장

2026-09-30 제작. [기존 남성 원화](../../client/public/character_concepts/barbarian.webp)를
기준으로 기존 모듈형 남성의 65본 리그에 맞춘 다섯 교체 파츠다.
맨가슴·허벅지·손·발을 유지하며, 투구를 착용하면 헤어를 숨긴다.

## 파일과 미리보기

제작용은 `assets/modular_human_male_01/parts/fitted/`, 게임용은
`client/public/models/characters/modular_male/`에 같은 이름으로 저장한다.

| 파일 | 구성 | Triangles |
| --- | --- | ---: |
| `helmet_barbarian.glb` | 뿔·붉은 보석·볼 보호대가 있는 열린 투구 | 1,453 |
| `top_barbarian.glb` | 양쪽 견갑·가죽 연결끈·가슴 장식 | 1,860 |
| `pants_barbarian.glb` | 벨트·앞뒤 모피·곡면 옆 모피·가죽 끈, 반바지 안감 제거 | 3,121 |
| `boots_barbarian.glb` | 양쪽 정강이 보호대·모피 커프·가죽 안감, 발은 노출 | 2,704 |
| `gloves_barbarian.glb` | 양쪽 손목 보호대·가죽 커프, 손은 노출 | 2,098 |

- 편집 원본: `assets/modular_human_male_01/parts/fitted/barbarian_parts.blend`.
  몸체와 다섯 파츠가 한 리그를 공유하며 텍스처를 내장한다.
- Meshy 입력 모델: `assets/modular_human_male_01/parts/barbarian_sources/`.
- 파츠 원화: `doc/images/characters/modular_human_male_01/parts/*_barbarian.png`.
- 추가 소재 원본: 같은 폴더의 `barbarian_leather.png`, `barbarian_fur.png`.
  생성 원본을 보존하고 GLB에는 Lanczos로 축소한 1024² 텍스처를 내장한다.
- [프롬프트·설정·작업 ID·비용·해시](modular-barbarian-sources.json).
- [게임용 입력·출력 해시](../../client/public/models/characters/modular_male/manifest.json).

개발 서버의 `/modular-character-preview.html?outfit=barbarian`으로 바로 착용한 모습을 본다.
각 슬롯을 따로 교체하거나 `바바리안 세트 입기` 버튼으로 전체를 적용한다.
이번 연결은 제작 미리보기와 게임용 자산 준비까지다. 신규 캐릭터의 아이템 지급,
`modularEquipment.ts`의 아이템 매핑과 실제 게임 기본 복장 적용은 아직 포함하지 않았다.

![바바리안 복장 전후측면](../images/characters/modular_human_male_01/barbarian-outfit-preview.png)

## 체형 맞춤

투구는 Head 본에 고정하고, 견갑과 가슴 연결끈에는 몸체의 가중치를 옮겼다.
앞자락에서 뒷자락을 복제하고 뒤 벨트를 추가했다.
2026-09-30 수정에서 갈색 반바지처럼 보이던 안감 1,035 triangles를 제거했다.
몸체의 기존 `covered_skin` 영역과 재질은 유지한다.
옆 모피 두 장과 가죽 끈 네 개는 실제 허리띠 표면에서 윗부분을 샘플링하고
1.5mm 안쪽으로 넣어 연결했다. 옆 모피는 두께 6mm의 곡면, 끈은 두께 3mm로 만들었다.
모두 Hips 본을 따르며, 윗부분 두 줄의 위치와 법선은 물리 계산 후에도 고정한다.
기존 모피·가죽 텍스처를 재사용한다.
앞뒤 모피는 원래 UV와 윤곽을 유지한 얇은 판이며 기존 허리 고정축에서 움직인다.
각 메시의 `pelt_physics`에 종류·고정 본·회전축 위치·바깥 방향·길이·충돌체와 격자 구성을 기록한다.
`pelt-rig.ts`가 전체 갱신과 충돌을 맡고, `pelt-cloth.ts`가 옆 모피와 끈의 변형을 계산한다.
옆 모피는 각각 9열 × 11행, 끈은 각각 3열 × 11행으로 총 330개 제어점을 사용한다.
1/120초 고정 간격의 Verlet 계산에 길이·전단·굽힘 제약을 적용한다.
옆 모피와 끈은 모두 휘어지되 감쇠·굽힘 저항을 높이고 바람 반응을 줄여 묵직하게 움직인다.
2026-09-30 사용자가 확인한 옆 모피와 끈의 움직임 값을 출처 JSON에 기록했다.
앞뒤 모피는 기존 1/60초 진자 계산과 최대 약 77도의 회전 제한을 유지한다.
충돌은 골반·다리의 타원체로 근사한다. 끈과 옆 모피의 세로 구간은 최대 4%까지만
늘어나도록 제한하며, 접촉 시 압축과 접힘은 허용한다.
고정된 윗줄을 제외한 법선과 안쪽 면은 변형된 격자에 맞춰 갱신한다.
캐릭터마다 변형용 메시를 복제하고 텍스처·재질과 몸체의 65본 스켈레톤은 공유한다.
순간이동·장비 재착용·미리보기 시점 변경 시 초기화하고 해제 시 원래 메시를 복구한다.
제작 미리보기·캐릭터 선택 미리보기·게임의 갱신 및 해제 경로에 연결했다.
일반 GLB 뷰어나 Blender에서는 기본 형상·가중치만 적용된다.

손목 보호대와 정강이 보호대는 한쪽을 맞춘 뒤 반대쪽을 대칭 제작했다.
각각 ForeArm·Leg 본을 따르며, 몸체 단면에서 필요한 여유를 계산한다.
Meshy 결과에서 부족했던 가죽 둘레·안감과 모피 커프는 로컬에서 만들었다.
추가 소재의 UV는 전용 가죽·모피 텍스처를 반복 사용한다.
머리·얼굴·기존 몸체의 형상과 가중치, 기존 애니메이션은 유지한다.

몸체와 다섯 부위는 숨김 포함 **25,127**, 표시 **24,756 triangles**다.
철검 302 triangles를 포함하면 **25,429 / 표시 25,058**이며,
기본 절차형 망토까지 추가하면 각각 120이 늘어난다.
얼굴 지정 영역 **1,505 triangles**와 몸체의 4096px 텍스처는 그대로다.
전체 목표 15,000–20,000보다 많으며, 뒷자락·곡면 옆 모피·가죽 끈·보호대 둘레를 포함한 수치다.
목표 수치만 맞추기 위한 일괄 감축은 하지 않았다.

## 재생성

저장소 루트에서 실행한다. 프로젝트 `.venv`의 NumPy·SciPy·Pillow,
FFmpeg·Blender와 클라이언트 Node 의존성을 사용한다.

```sh
.venv/bin/python tools/fit-modular-barbarian.py
blender -b --python-exit-code 1 -P tools/blender-scripts/pack_modular_plate.py -- --outfit barbarian
node tools/prepare-modular-character.mjs --parts-only
```

Windows에서는 `.venv/Scripts/python.exe`를 사용한다.
`--parts-only`는 기존 동작 팩을 유지하고 파츠와 manifest의 해당 해시만 갱신한다.
필요한 생성 원본과 제작·게임용 모델은 `assets.lock`에 고정한 Hugging Face 자산을 사용한다.
일회성 API 생성 스크립트와 임시 검토 파일은 최종본 정리 때 삭제했다.

## 출처와 라이선스

- 참고 원화는 [캐릭터 출처 기록](characters.md#other-classes)의 남성 바바리안이다.
- 파츠 원화·보완 원화·소재 텍스처: OpenAI Codex built-in ImageGen,
  **ChatGPT Pro 20x**, 2026-09-30. OpenAI 생성 출력물 이용 조건 적용.
  실제 프롬프트와 참조 이미지는 출처 JSON에 기록한다.
- 3D·PBR: Meshy Image to 3D API, **Meshy Premium**, `meshy-7.1`, 2026-09-30.
  Ultra 4K 형상, 2K PBR, triangle remesh. 목표는 투구 1,400, 상의·하의 각 1,800,
  한쪽 정강이 보호대 900, 한쪽 손목 보호대 700이다.
  채택 5종 **175크레딧**, 최초 보호대 2종을 포함한 총 **245크레딧**.
  [Meshy 유료 생성물 이용 조건](https://help.meshy.ai/en/articles/10137554-what-is-the-ownership-of-the-generated-models)을 따르며 Community에 게시하지 않았다.
- **[미사용]** `*-unused-v1.png`, `*-unused-v1.glb`: 최초 손목·정강이 보호대.
  모피 누락과 길쭉한 손목 형상 때문에 교체했다. 중간 파일 4개는 삭제하고
  프롬프트·작업 ID·비용·해시와 당시 경로는 출처 JSON에 보존했다.
- 몸체·Mixamo 리그·동작은
  [모듈형 남성 출처](modular-human-male-01.md)를 따른다.
- 착용 미리보기 PNG는 위 게임용 자산을 Three.js로 렌더한 프로젝트 제작 이미지다.

## 검증

- 게임용 GLB 5종: Khronos glTF Validator 오류·경고·정보·힌트 0.
- 게임용 파츠의 65본·본 순서·기준 행렬이 몸체와 일치하며 한 스켈레톤을 공유한다.
- 기본 자세·대기·걷기·달리기·전투 대기·공격·점프를 게임용 압축본으로 확인했다.
- 옆 모피와 끈의 고정점 60개를 실제 허리띠 메시와 비교했다. 모두 표면에서 1.5mm 이내다.
- 6개 동작을 1,080프레임 연속 재생했다. 고정 정점 288개의 최대 위치 오차는 약 0.000000045m,
  자유 구간의 최대 늘어남은 약 4.0004%였으며 유한 정점 검사와 브라우저 오류 검사도 통과했다.
- 정지 후 600프레임 안정화하고 120프레임을 비교했을 때 정점 위치 변화는 0이었다.
- 로컬 헤드리스 Chrome에서 한 캐릭터의 물리·메시 갱신 평균 약 0.69ms를 측정했다.
  WebGL 그림자 렌더링도 확인했다. 군중·모바일 성능은 측정하지 않았다.
- 펠트·망토·모듈형 복장·캐릭터 로더·동작 테스트 66개와 `npm run check`, `npm run lint` 통과.

확대하면 모피의 메시 윤곽과 반복 텍스처가 보인다. 충돌체는 몸체의 근사 형상이며,
자락끼리의 충돌과 지면 충돌은 계산하지 않는다. 앞뒤 모피는 제한된 회전축을 사용하며,
극단적인 자세와 모든 장비 조합에서의 관통 방지를 보장하지는 않는다.
