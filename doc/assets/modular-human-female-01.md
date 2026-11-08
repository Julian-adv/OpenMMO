# 여성 모듈형 공통 몸체

여성 모듈형 캐릭터의 맨몸 기준 몸체다. 리그 ID `human_female_01_local_v1`(65본), 원본은 미터 단위(키 약 1.8m)다.
채택본은 `rig_v1/`이며, 게임용 `client/public/models/characters/modular_female/base.glb`는
`tools/prepare-female-knight.mjs`가 만든다([여성 기사 게임 적용](female-knight-runtime.md)). 헤어는 [여성 기사 헤어](modular-female-knight-hair.md)에 있다.

## 최종 파일

- [`rig_v1/female-base.glb`](../../assets/modular_human_female_01/rig_v1/female-base.glb): 리깅·동작을 포함한 최종본이자 편집 원본.
- [`rig_v1/female-base.blend`](../../assets/modular_human_female_01/rig_v1/female-base.blend): 같은 리그·9개 액션·텍스처를 내장한 Blender 편집본. A 포즈로 열리며 액션을 고르거나 NLA 트랙 음소거를 풀어 재생한다.
- [`rig_v1/catwalk-source.glb`](../../assets/modular_human_female_01/rig_v1/catwalk-source.glb): `walk` 리타깃 입력. `assets/all_animation.blend`의 Mixamo `walk_cat`에서 추출했다.
- 미리보기: `/modular-character-preview.html?character=female`.

## 구성

- **22,175삼각형**, 메시 1개. 몸체와 흰색 기본 상·하의(속옷)가 한 메시 안의 분리된 표면이다. 장비의 가림 명세가 속옷과 몸 일부를 숨긴다.
- 텍스처: Tripo의 base-color·normal·ORM 4096²에 눈·코·입술·눈썹·귀·속옷 자국을 국소 보정해 구웠다.
- 속눈썹: 곡선 리본 메시 86가닥, 불투명 짙은 갈색 재질. 얼굴 상부와 속눈썹은 Head 본에 고정한다.
- 동작 9개: `idle1`, `idle_armed`, `walk`(여성 캣워크), `run`, `jump`, `combat_idle`, `slash1`, `dying`, `sit_idle`. 60 Hz 접지 보정을 구웠고, `sit_idle`은 게임 `Chair`(`objects/chair.glb`)와 카탈로그 상호작용 높이에 맞춘다.
  - 게임용 `base.glb`에는 `idle_armed`를 빼고 8개만 싣는다(2026-11-08). 내보내기가 `idle1`·`walk` 등의 오른손에 `idle_armed`의 장검 파지를 구워 넣어 두 대기 동작이 같아지기 때문이다. 파지 값은 `tools/prepare-female-knight.mjs`가 원본 리그의 `idle_armed`에서 읽는다.
- 기본 상의 옆면은 피부와 같은 보간 웨이트를 써서 대기 동작에서 피부가 뚫고 나오지 않는다.

### 피부 텍스처의 속옷 자국 제거 — 2026-10-14

기본 상의와 허리 뒤 리본이 덮는 피부의 흰 자국을 주변 피부색으로 메웠다(색·ORM, 노멀맵은 평평하게). 하의 속옷 아래는 대상이 아니다.
가슴은 세부 없는 매끈한 피부다. 허리 뒤 중앙에 1–2mm 세로 음영 몇 개가 남는데, 기존 메시 노멀 이음선이다.

## 재현

`female-base.glb`를 원본에서 다시 만드는 도구는 없으므로 GLB가 편집 원본이다. 남은 도구:

```sh
blender -b -t 4 --python tools/blender-scripts/save_female_rig.py
blender -b -t 4 --python tools/blender-scripts/export_female_catwalk.py
node tools/validate-female-seated.mjs
node tools/animate-female-base.mjs
```

- `save_female_rig.py`: GLB에서 `female-base.blend`를 다시 만든다.
- `export_female_catwalk.py`: `catwalk-source.glb`를 추출한다.
- `validate-female-seated.mjs`: 61시점의 손·무릎 접촉과 좌석 눌림을 검사해 `rig_v1/seated-validation.json`에 쓴다.
- `animate-female-base.mjs`(`tools/lib/female-catwalk.mjs`, `female-seated.mjs` 사용): 9개 동작을 `rig_v1/clips.json`으로 다시 계산한다. GLB에 굽는 단계는 없다.

## 출처·라이선스

**원화.** 사용자 제공 ComfyUI / Qwen Image 2.1, 1024×1536 PNG, 제공·기준 지정일 2026-10-11. 정확한 생성일·등급·프롬프트·워크플로우와 Qwen 이용 조건은 미확인이다.
정면은 Tripo 입력, 측면은 캐릭터 왼쪽을 보는 참고 원화다. 참고 원화: `client/public/character_concepts/female_rogue.webp`, `female_knight.webp`.
[원본 경로·해시·편집 이력](modular-human-female-01-sources.json).

![정면 원화](../images/characters/modular_human_female_01/Qwen_image_2.1_00012.png)

![측면 원화](../images/characters/modular_human_female_01/Qwen_image_2.1_00025.png)

- **[미사용]** [`base-front-v1.png`](../images/characters/modular_human_female_01/base-front-v1.png): 회색 상의·반바지 초안. OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-10-11.
- **[미사용]** [`base-front-v2.png`](../images/characters/modular_human_female_01/base-front-v2.png): 묶은 머리·흰색 기본 속옷. 사용자 제공 ComfyUI / Qwen Image 2.1, 조건 미확인.
- **[미사용]** [`base-front-v3.png`](../images/characters/modular_human_female_01/base-front-v3.png): 이전 대머리 정면(`Qwen_image_2.1_00002.png`). 사용자 제공 ComfyUI / Qwen Image 2.1, 조건 미확인.

**3D 생성.** Tripo API 유료 크레딧(사용자 구매 1,000크레딧), 2026-10-11, [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
작업 ID `073e85e0-595b-4a12-9030-2dc3e30566e9`, image_to_model 정면 한 장, `P2-20260801`, `face_limit=20000`, `quad=false`,
PBR·detailed 텍스처 `v3.5-20260815`, 시드 101101, **120크레딧**. 결과 20,927삼각형, base-color·normal·ORM 각 4096².

- 원본: [`tripo_api_v3/model_url.glb`](../../assets/modular_human_female_01/tripo_api_v3/model_url.glb). 검토 렌더 스크립트 [`review_source.py`](../../assets/modular_human_female_01/tripo_api_v3/review_source.py)가 같은 폴더에 있다.
- [제출 설정](modular-human-female-01-tripo-api-v3-build.json), [작업 ID·비용·다운로드 해시](modular-human-female-01-tripo-api-v3-sources.json), [검토·기하 진단](modular-human-female-01-tripo-api-v3-review.json).
- 채택하지 않은 생성(파일 삭제, 기록만 보존):
  - Tripo API v1: 작업 ID `5acba197-8120-45d0-bf58-31486ae80d58`, v3 대머리 정면, 6,000쿼드 목표, **120크레딧**, 12,208삼각형. [설정](modular-human-female-01-tripo-api-v1-build.json), [작업·비용](modular-human-female-01-tripo-api-v1-sources.json), [검토](modular-human-female-01-tripo-api-v1-review.json).
  - Tripo API v2: 정면·측면 multiview, 15,000쿼드 목표. HTTP 400 코드 2008(콘텐츠 정책) 거절, 요청 ID `08b866b0-e515-4e77-b6d5-757d44d6bd3f`, 0크레딧. [설정](modular-human-female-01-tripo-api-v2-build.json), [응답·잔액](modular-human-female-01-tripo-api-v2-sources.json).
  - 사용자 제공 Tripo GLB(11,493삼각형)·Meshy AI GLB(12,282삼각형): 등급·생성일·이용 조건 미확인. [Tripo](modular-human-female-01-tripo-sources.json), [Meshy](modular-human-female-01-meshy-sources.json).

**AI 이미지 편집.** 모두 OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-10-11, OpenAI 생성 출력물 이용 조건. 전체 프롬프트는 각 기록에 있다.

- 눈 알베도 편집 v2·v3: 최종 텍스처에 구워졌다. [v2](modular-human-female-01-eyes-v2-sources.json), [v3](modular-human-female-01-eyes-v3-sources.json).
  v3 편집 출력 [`eyes_v3/face-albedo-edited.png`](../../assets/modular_human_female_01/eyes_v3/face-albedo-edited.png)을 눈 전체 UV에 다시 매핑한 것이 최종 눈이다([매핑 기록](modular-human-female-01-eyes-v6-sources.json)).
- **[미사용]** 속눈썹 알파 텍스처(2172×724 RGBA): 제작 원본 GLB에 `Natural eyelash fringe alpha`로 남아 있지만 현재 가닥 메시는 쓰지 않는다. 게임용 베이스에서는 2026-10-25 다운로드 최적화로 제거했다. [프롬프트](modular-human-female-01-eyelashes-v1-sources.json).
- **[미사용]** 눈 보정 v1: 채택하지 않은 Tripo API v1 모델 대상. [프롬프트](modular-human-female-01-eyes-v1-sources.json).
- 출력 없음: 비키니 속옷 편집과 대머리·맨발 접지 편집은 안전 필터에 차단됐다. [속옷 요청](modular-human-female-01-sources.json), [대머리·맨발 요청](modular-human-female-01-bald-flatfeet-attempt.json).

나머지 얼굴 보정(코·입술·눈썹·귀 노멀, 오른쪽 눈 정점 노멀, 속눈썹 가닥 형상)과 속옷 자국 제거는 로컬 Python·Blender 편집이며 생성 AI나 과금이 없다.

**리깅·동작.** 남성 모듈형 몸체에서 본 이름·계층·회전축만 참조하고, 관절·가중치(Blender bone-heat)는 여성 몸체에 새로 만들었다.
동작은 프로젝트의 Mixamo 동작(`assets/modular_human_male_01/animations/animations.glb`, `client/public/models/characters/modular_male/animations/social.glb`, `assets/all_animation.blend`의 `walk_cat`)을 리타깃했다. 로컬 작업이며 추가 구독·과금은 없다.
[리깅](modular-human-female-01-rig-v1.json), [캣워크](modular-human-female-01-catwalk-v1.json), [앉기](modular-human-female-01-seated-v1.json) 기록.

## 추가 얼굴 피팅

2026-11-07 사용자 제공 전신 GLB의 머리를 분리해 이 몸체에 연결한
[새 얼굴 시안](modular-female-custom-face.md)을 만들었다. 원본 몸체는 보존하며,
게임 외형 목록에는 아직 등록하지 않았다.

## 제한

- `female-base.blend`는 미터 전환 전 제작 단위(높이 1)이고 속옷 자국 제거 전 텍스처를 담는다. `catwalk-source.glb`는 Mixamo 자체 단위다.
- 표정·눈 깜빡임 리그가 없다. 눈 텍스처는 원래 4K 아틀라스 해상도라 초근접 확대에서 선명도에 한계가 있다.
- 몸체 GLB를 바꾸면 장비 가림 명세의 `body_sha256`과 `fit-female-knight-{gloves,helmet}.py`의 몸체 해시 검사를 갱신해야 한다.
