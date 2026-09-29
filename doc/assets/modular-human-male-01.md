# Modular Human Male 01 — 최종 보관 파일

2026-09-29 사용자 요청으로 중간 작업물과 검증·테스트 파일을 정리했다.
`assets/modular_human_male_01/`에는 현재 채택한 파츠, 편집용 원본, 애니메이션과
검 그립 설정 11파일만 보관한다. 게임과 미리보기에서 사용하는 경로는 유지한다.

## 파일 구성

| 경로 (`assets/modular_human_male_01/` 기준) | 용도 |
| --- | --- |
| `parts/fitted/base.glb` | 영역별로 분리한 최종 몸체 |
| `parts/fitted/hair_crop.glb` | 짧은 크롭 헤어 |
| `parts/fitted/hair_sidepart.glb` | 짧은 옆가르마 헤어 |
| `parts/fitted/top_linen.glb` | 천 셔츠와 갑옷용 목깃 |
| `parts/fitted/top_leather.glb` | 가죽 갑옷 |
| `parts/fitted/pants_cloth.glb` | 천 바지와 부츠 착용용 끝단 |
| `parts/fitted/gloves_leather.glb` | 가죽 장갑 |
| `parts/fitted/boots_leather.glb` | 가죽 부츠 |
| `parts/fitted/character_parts.blend` | 최종 파츠 편집 원본; 24개 이미지 내장, 외부 라이브러리 없음 |
| `rigged_hand_tuned/animations.glb` | 손가락·어깨·대기 자세·접지 보정을 저장한 7동작 |
| `rigged_hand_tuned/hand-grips.json` | 동작별 검 부착 위치·회전과 손 자세 프로파일 |

최종 파츠는 `human_male_01_mixamo_candidate_v2`의 같은 65본과 기준 자세를 사용한다.
몸체는 13,520 triangles, 얼굴 지정 영역은 1,505 triangles다.
갑옷 착용 시 안쪽 셔츠의 겨드랑이·소매·목깃을 표시하고, 부츠 착탈에 따라 바지 끝단을 바꾼다.
천 셔츠 밑단은 허리밴드 안으로 들어간다. 가리는 몸체·의상 영역은 런타임에서 숨긴다.

여기서 최종은 현재 보관·사용하는 버전을 뜻한다. 장갑 겹침, 의상·헤어의 시각 품질과
숨긴 형상을 포함한 조합 폴리곤 예산 검토는 남아 있다.

## 사용과 복원

저장소 루트에서 최종 제작 에셋만 복원한다.

```sh
bash tools/fetch-assets.sh assets/modular_human_male_01/
```

개발 서버의 `/modular-character-preview.html`에서 헤어·복장·색상과 동작을 확인한다.
미리보기는 확정 애니메이션만 사용하며, 삭제한 이전 손 자세와의 비교 기능은 제공하지 않는다.

게임용 압축 파츠 5개와 동작 9팩은 `client/public/models/characters/modular_male/`에 있다.
재생성에는 위 최종 파일과 기존 공용 애니메이션 팩을 사용한다.

```sh
node tools/prepare-modular-character.mjs
```

프로젝트 `.venv`의 Pillow·NumPy와 클라이언트 Node 의존성이 필요하다.
출력과 입력 해시는 [게임 manifest](../../client/public/models/characters/modular_male/manifest.json)에 기록한다.
완성된 전용 애니메이션은 다시 리타게팅하거나 접지·손가락 보정을 중복 적용하지 않는다.
검 부착 트랙은 `hand-grips.json`에서 가져와 캐릭터 믹서에서 함께 혼합한다.

## 출처와 정리 범위

- 원화와 등 텍스처 보정: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-09-28.
  OpenAI 생성 출력물 이용 조건 적용. 원화는 `doc/images/characters/modular_human_male_01/`에 유지한다.
- 형상·PBR: **Meshy Premium**, `meshy-7.1`, 2026-09-28. 몸체 35크레딧, 파츠 7종 총 245크레딧.
  [Meshy 유료 생성물 이용 조건](https://help.meshy.ai/en/articles/10137554-what-is-the-ownership-of-the-generated-models) 적용.
- 리그: 사용자 제공 Adobe Mixamo `Idle (11).fbx`, 2026-09-28 확인, 무료 서비스.
  원본 SHA-256은 `f774a2ca19699412a1ce45720c7d00b618ff3e168d6ca77a626feffa98da9a8c`다.
  라이선스는 [캐릭터 출처 기록](characters.md#license)을 따른다.
- 실제 프롬프트·작업 ID·설정·원본 해시는 [출처 기록](modular-human-male-01-sources.json)에 모았다.
- **[미사용]** Meshy 24본 리그 시험(5크레딧), 이전 정면 손바닥 리그, 재리깅 전 몸체,
  중복 GLB·FBX·OBJ·텍스처·ZIP, 비교 애니메이션, 렌더·스크린샷, 검증 JSON·로그,
  일회성 제작·검사 스크립트와 Blender 백업은 현재 폴더와 `assets.lock`에서 제거했다.

삭제 전 기록은 HF `jake-song-openmmo/onlinerpg-assets`의
`6f15946b4d61d860c9037775a6fbb3e6c303e1be` 리비전에 남아 있다.
현재 `assets.lock`은 최종 11파일만 복원하므로 중간 작업물이 다시 내려오지 않는다.
