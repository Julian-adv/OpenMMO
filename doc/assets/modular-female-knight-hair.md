# 여성 나이트 헤어

여성 모듈러 캐릭터(`rig_v1/female-base.glb`, 리그 `human_female_01_local_v1`)의 헤어 파츠.
채택본은 턱 아래 길이의 금발 단발 `tripo_knight_bob_v1/fitted_v1`이며 여성 미리보기의 기본 헤어다.
`tools/prepare-female-knight.mjs --part hair`가 이 GLB를 `client/public/models/characters/modular_female/hair.glb`로 패키징한다.
긴 금발 `tripo_knight_v1`은 **[미사용 · 보관]**이다. 2026-11-05 미리보기 헤어 메뉴에서도 뺐다(사용자 요청). 파일은 보관한다.

## 최종 파일

단발 (2,436삼각형, 메시 1개, 4K 컬러·노멀·ORM):

- [리깅 GLB](../../assets/modular_human_female_01/hair/tripo_knight_bob_v1/fitted_v1/hair_female_knight_bob.glb), [리깅 입력 GLB](../../assets/modular_human_female_01/hair/tripo_knight_bob_v1/fitted_v1/hair-fitted-unskinned.glb)
- [Blender 편집본](../../assets/modular_human_female_01/hair/tripo_knight_bob_v1/fitted_v1/female-knight-bob-fitted.blend) (몸체·텍스처·9개 동작 포함)
- [피팅·리깅 기록](modular-female-knight-bob-fitting-v1.json), [애니메이션 검사](modular-female-knight-bob-animation-v1.json), [브라우저 검사](modular-female-knight-bob-browser-v1.json), [glTF 검사](modular-female-knight-bob-gltf-validation-v1.json)

여성 리그 65개 본의 bind 행렬을 그대로 쓰고 헤어 전체를 `Head` 가중치 1로 연결한다. 흔들림 본·물리는 없다.

**[미사용 · 보관]** 긴 헤어 (4,386삼각형, 메시 2개):

- [리깅 GLB](../../assets/modular_human_female_01/hair/tripo_knight_v1/fitted_v1/hair_female_knight.glb), [리깅 입력 GLB](../../assets/modular_human_female_01/hair/tripo_knight_v1/fitted_v1/hair-fitted-unskinned.glb)
- [Blender 편집본](../../assets/modular_human_female_01/hair/tripo_knight_v1/fitted_v1/female-knight-hair-fitted.blend)
- [피팅·리깅 기록](modular-female-knight-hair-fitting-v1.json), [애니메이션 검사](modular-female-knight-hair-animation-v1.json), [브라우저 검사](modular-female-knight-hair-browser-v1.json)
- 피팅 입력인 뒷머리 보정본(4,835삼각형 = 원본 2,819 + 추가 2,016): [GLB](../../assets/modular_human_female_01/hair/tripo_knight_v1/rear_filled_v1/hair_female_knight-rear-filled-v1.glb), [Blender](../../assets/modular_human_female_01/hair/tripo_knight_v1/rear_filled_v1/hair_female_knight-rear-filled-v1.blend), [2K 노멀](../../assets/modular_human_female_01/hair/tripo_knight_v1/rear_filled_v1/rear-strand-normal.png), [기록](modular-female-knight-hair-rear-filled-v1.json)

미리보기는 [로딩 최적화 파생본](modular-female-preview.md)을 쓴다.

## 재현

단발:

```bash
blender -b -t 6 --python-exit-code 1 --python tools/blender-scripts/fit_female_knight_bob.py
.venv/bin/python tools/rig-female-knight-bob.py
node tools/validate-female-knight-bob.mjs
blender -b -t 6 --python-exit-code 1 --python tools/blender-scripts/review_female_knight_bob.py
```

긴 헤어 (뒷머리 보정본부터):

```bash
blender -b -t 6 --python-exit-code 1 --python tools/blender-scripts/fill_female_knight_hair.py
blender -b -t 6 --python-exit-code 1 --python tools/blender-scripts/fit_female_knight_hair.py
.venv/bin/python tools/rig-female-knight-hair.py
node tools/validate-female-knight-hair.mjs
blender -b -t 6 --python-exit-code 1 --python tools/blender-scripts/review_female_knight_hair.py
```

검토 스크립트는 검증 명령이 만드는 `animation-snapshots.json`을 입력으로 쓴다.

## 출처·라이선스

원화: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-10-12. OpenAI 생성 출력물 이용 조건과
[여성 나이트 원화 기록](characters.md#female-knight)(ComfyUI jibMixZIT_v10 → Qwen Image Edit A포즈 → ChatGPT Pro 20x 투명화)을 따른다.
[기존 여성 나이트 원화](../../client/public/character_concepts/female_knight.webp)에서 헤어만 분리했고, 얼굴·몸·갑옷은 포함하지 않는다.

- 단발 v2: [정면](../images/characters/modular_human_female_01/parts/knight/hair_female_knight-bob-front-v2.png) ([프롬프트·해시](modular-female-knight-bob-front-v2-sources.json)), [측면](../images/characters/modular_human_female_01/parts/knight/hair_female_knight-bob-side-v2.png)·[후면](../images/characters/modular_human_female_01/parts/knight/hair_female_knight-bob-back-v2.png) ([프롬프트·해시](modular-female-knight-bob-views-v2-sources.json)). 정면 v2는 v1의 가운데 투명 구멍을 머릿결로 채운 보정본이다.
- **[미사용]** 단발 [정면 v1](../images/characters/modular_human_female_01/parts/knight/hair_female_knight-bob-front-v1.png) ([생성 기록](modular-female-knight-bob-sources.json)) — 정면 v2의 편집 원본.
- 긴 헤어: [정면](../images/characters/modular_human_female_01/parts/knight/hair_female_knight-front-v1.png)·[측면](../images/characters/modular_human_female_01/parts/knight/hair_female_knight-side-v1.png)·[후면](../images/characters/modular_human_female_01/parts/knight/hair_female_knight-back-v1.png), 1024×1536 RGBA ([프롬프트·해시](modular-female-knight-hair-sources.json)).
- 측면은 정면을, 후면은 정면·측면을 참고해 생성했다. 시점 간 직교 기하 일치는 보장하지 않는다.

메시: Tripo API, 유료 API 크레딧, [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
[다중 시점 API](https://developers.tripo3d.ai/en/docs/generation-multiview-to-model/p)의 `front`/`right`/`back` 슬롯(화면 왼쪽을 보는 측면을 `right`에 넣었다. 2026-11-11 확인: 그 그림은 `left` 칸이 맞다. [긴 생머리 기록](modular-female-long-straight-hair.md) 참조),
`P2-20260801`, `v3.5-20260815`, PBR detailed 텍스처.

| | 단발 | 긴 헤어 |
|---|---|---|
| 생성일 | 2026-10-12 | 2026-10-12 |
| 작업 ID | `fb994e78-cb1a-4fab-aa0f-2bea9e599fc5` | `c10df22d-f0a9-461d-b87e-a8b42409eae2` |
| 목표 / 결과 | 2,500 / 2,491삼각형, `quad=false` | 3,000 / 2,819삼각형 |
| 크레딧 | 120 사용, 잔액 520 | 120 사용, 잔액 640 |
| 원본 | [model_url.glb](../../assets/modular_human_female_01/hair/tripo_knight_bob_v1/model_url.glb) | [model_url.glb](../../assets/modular_human_female_01/hair/tripo_knight_v1/model_url.glb) |
| 기록 | [제작 명세](modular-female-knight-bob-tripo-v1-build.json), [요청·작업·비용](modular-female-knight-bob-tripo-v1-sources.json), [원본 검사](modular-female-knight-bob-tripo-v1-review.json) | [제작 명세](modular-female-knight-hair-tripo-v1-build.json), [요청·작업·비용](modular-female-knight-hair-tripo-v1-sources.json), [원본 검사](modular-female-knight-hair-tripo-v1-review.json) |

피팅·뒷머리 보정·리깅은 로컬 Blender 5.2.0과 Python/Three.js 작업이며 추가 API 비용은 없다.
몸체 출처는 [여성 베이스](modular-human-female-01.md)를 따른다.

## 제한

- 몸체와 조립하면 22,175 + 2,436 = 24,611삼각형(단발)으로 조립 목표 15,000–20,000을 넘는다. 몸체·얼굴은 감축하지 않았다.
- 단발은 측면·뒤쪽 일부 다발에 각진 경계와 좁은 틈이 있다.
- 긴 헤어의 뒷머리 추가 면은 양면 표면이며 원본과 용접한 솔리드가 아니다. 쓰러짐 동작에서 일부 모서리가 최대 약 2.28배 늘어난다.
- 동작 검사는 전신 충돌 검사가 아니다. 갑옷·높은 옷깃·투구와의 조합은 검증하지 않았다.
