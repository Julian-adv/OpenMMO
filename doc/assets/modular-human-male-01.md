# Modular Human Male 01 — 최종 보관 파일

2026-09-29 사용자 요청으로 중간 작업물과 검증·테스트 파일을 정리했다.
기본 파츠, 편집용 원본, 애니메이션과 검 그립 설정 11파일을 남겼다.
같은 날 [기사 판금 세트](modular-knight-plate.md)의 파츠·입력 원본·편집 파일 11개를 추가했다.
게임과 미리보기에서 사용하는 기존 경로는 유지한다.

## 파일 구성

| 경로 (`assets/modular_human_male_01/` 기준) | 용도 |
| --- | --- |
| `parts/fitted/base.glb` | 영역별로 분리한 최종 몸체 |
| `parts/fitted/hair_crop.glb` | 짧은 크롭 헤어 |
| `parts/fitted/hair_sidepart.glb` | 짧은 옆가르마 헤어 |
| `parts/fitted/top_linen.glb` | 천 셔츠와 갑옷용 목깃·앞트임 안감 |
| `parts/fitted/top_leather.glb` | 가죽 갑옷 |
| `parts/fitted/pants_cloth.glb` | 천 바지와 부츠 착용용 끝단 |
| `parts/fitted/gloves_leather.glb` | 가죽 장갑 |
| `parts/fitted/boots_leather.glb` | 가죽 부츠 |
| `parts/fitted/character_parts.blend` | 최종 파츠 편집 원본; 24개 이미지 내장, 외부 라이브러리 없음 |
| `rigged_hand_tuned/animations.glb` | 손가락·어깨·대기 자세·접지 보정을 저장한 7동작 |
| `rigged_hand_tuned/hand-grips.json` | 동작별 검 부착 위치·회전과 손 자세 프로파일 |
| `parts/fitted/*_plate.glb` | 기사 판금 갑옷·바지·장갑·신발·헬멧 5종 |
| `parts/fitted/plate_parts.blend` | 한 리그와 내장 텍스처를 사용하는 판금 편집 원본 |
| `parts/plate_sources/*_plate.glb` | 판금 재가공용 Meshy 입력 GLB 5종 |

최종 파츠는 `human_male_01_mixamo_candidate_v2`의 같은 65본과 기준 자세를 사용한다.
몸체는 13,520 triangles, 얼굴 지정 영역은 1,505 triangles다.
가죽 갑옷 착용 시 안쪽 셔츠의 겨드랑이·소매·목깃·앞트임 안감을 표시하고, 부츠 착탈에 따라 바지 끝단을 바꾼다.
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

게임용 압축 파츠 10개와 동작 9팩은 `client/public/models/characters/modular_male/`에 있다.
재생성에는 위 최종 파일과 기존 공용 애니메이션 팩을 사용한다.

```sh
node tools/prepare-modular-character.mjs
```

프로젝트 `.venv`의 Pillow·NumPy와 클라이언트 Node 의존성이 필요하다.
출력과 입력 해시는 [게임 manifest](../../client/public/models/characters/modular_male/manifest.json)에 기록한다.
완성된 전용 애니메이션은 다시 리타게팅하거나 접지·손가락 보정을 중복 적용하지 않는다.
검 부착 트랙은 `hand-grips.json`에서 가져와 캐릭터 믹서에서 함께 혼합한다.

## 검 그립 보정 — 2026-09-29

대기·걷기·달리기·점프·전투 대기·공격의 오른손을 같은 손잡이 둘레에 맞췄다.
검지·새끼의 과도한 접힘을 풀고 중지·약지의 끝마디를 조정했으며,
엄지는 검지 쪽 손잡이를 감싸도록 바꿨다. 손잡이 중심을 손바닥 앞쪽으로
약 11mm, 손바닥 면에서 바깥쪽으로 약 14mm 옮겨 검지 안에 파묻히던 위치를 조정했다.
기존 일반 대기의 별도 위치 보정은 새 공통 그립으로 대체했다.

걷기의 검 부착 회전은 오른손목 트랙으로 옮겨 손 안에서 검이 따로 기울지 않게 했다.
검의 기존 월드 회전과의 최대 차이는 241시점에서 0.006도 미만이다.
최종 손가락 회전은 `animations.glb`에 저장하고 `hand-grips.json`의
`sword_grip`에 기록했다. 게임용 9팩도 재생성했으며 추가 AI 생성·크레딧 사용은 없다.

오른손 보정 이외의 원본 트랙 363개가 유지되는 것을 확인했다.
애니메이션 GLB 10개는 glTF Validator 지적 0, 게임 로더는 51클립·255포즈 검사 통과다.
제작 미리보기의 6동작·18포즈와 맨손·판금 장갑 확대를 확인했으며,
관련 테스트 36개, `npm run check`, `npm run lint`도 통과했다.

## 판금 바지용 셔츠 밑단 보정 — 2026-09-29

천 바지보다 앞쪽 허리선이 안으로 들어간 판금 바지에서 셔츠가 벨트를 관통하던 부분을 수정했다.
셔츠 몸통의 높이 1.165m 아래를 부드럽게 좁혀 밑단이 두 바지의 허리밴드 안으로 들어가게 했다.
삼각형 수·UV·텍스처·스킨 가중치와 소매·목깃은 유지했다. 제작용·게임용 `top_linen.glb`와
`character_parts.blend`를 갱신했으며 추가 AI 생성·크레딧 사용은 없다.

천·판금 바지의 정면·측면·후면과 걷기·달리기를 비교했다. 게임용 압축본은
대기·걷기·달리기·점프·전투 대기·공격의 12포즈·36시점에서 허리 경계를 확인했다.
GLB 2종의 검증 오류와 브라우저 오류는 0이며, GLB별 기존 경고 8개는 변경 전과 같다.
Blender 원본과 제작 GLB의 셔츠 정점 위치도 일치한다.

## 가죽 갑옷 길이·허리 보정 — 2026-09-29

가죽 갑옷의 앞자락 최저점을 0.96m에서 1.10m로 올려 허리띠 윗부분을 살짝 덮도록 줄였다.
가슴·어깨 형상은 유지하고, 높이 1.30m 아래에서 허리로 내려갈수록 폭과 두께를 최대 14% 좁혔다.
앞트임·버클·테두리와 기존 UV·텍스처·스킨 가중치는 유지하며, 가죽 갑옷은 기존 **3,757 triangles**다.

짧아진 앞트임 뒤에는 기존 셔츠 면 135 triangles를 안쪽으로 넣어 목깃 영역에 추가했다.
셔츠 단독 착용에서는 숨기며, 셔츠 파츠의 숨김 포함 합계는 **4,744 triangles**다.
가죽 갑옷과 천 바지를 조합할 때는 바지 허리 영역도 표시해 밑단 아래 빈틈을 막는다.
제작 GLB 2종·게임용 셔츠·`character_parts.blend`를 갱신했다. 추가 AI 생성·크레딧 사용은 없다.

천·판금 바지의 정면·측면·후면과 6동작·12포즈·36시점에서 허리 경계를 확인했다.
GLB 오류·브라우저 오류 0, 기존 GLB 경고는 유지된다. 관련 테스트 20개와
`npm run check`, `npm run lint`를 통과했다.

후속 겨드랑이 보정: 아래 테두리에 섞인 팔 가중치 때문에 전투 자세에서 뾰족하게
끌려 나오던 부분을 수정했다. 양쪽 겨드랑이 아래는 몸통 가중치를 따르게 하고 위쪽으로
부드럽게 전환했다. 정점 위치·삼각형·UV·텍스처와 앞서 맞춘 길이·허리 형상은 유지한다.
제작 GLB와 Blender 원본에 반영했으며, 6동작의 양쪽 겨드랑이를 36시점에서 확인했다.
GLB 오류·브라우저 오류 0, GLB의 기존 경고 2개는 동일하다. 추가 생성 비용은 없다.

## 출처와 정리 범위

- 원화와 등 텍스처 보정: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-09-28.
  OpenAI 생성 출력물 이용 조건 적용. 원화는 `doc/images/characters/modular_human_male_01/`에 유지한다.
- 형상·PBR: **Meshy Premium**, `meshy-7.1`, 2026-09-28. 몸체 35크레딧, 파츠 7종 총 245크레딧.
  [Meshy 유료 생성물 이용 조건](https://help.meshy.ai/en/articles/10137554-what-is-the-ownership-of-the-generated-models) 적용.
- 리그: 사용자 제공 Adobe Mixamo `Idle (11).fbx`, 2026-09-28 확인, 무료 서비스.
  원본 SHA-256은 `f774a2ca19699412a1ce45720c7d00b618ff3e168d6ca77a626feffa98da9a8c`다.
  라이선스는 [캐릭터 출처 기록](characters.md#license)을 따른다.
- 실제 프롬프트·작업 ID·설정·원본 해시는 [출처 기록](modular-human-male-01-sources.json)에 모았다.
- 추가 판금의 원화·Meshy 출처와 비용은 [판금 출처 기록](modular-knight-plate-sources.json)에 분리했다.
- **[미사용]** Meshy 24본 리그 시험(5크레딧), 이전 정면 손바닥 리그, 재리깅 전 몸체,
  중복 GLB·FBX·OBJ·텍스처·ZIP, 비교 애니메이션, 렌더·스크린샷, 검증 JSON·로그,
  일회성 제작·검사 스크립트와 Blender 백업은 현재 폴더와 `assets.lock`에서 제거했다.

삭제 전 기록은 HF `jake-song-openmmo/onlinerpg-assets`의
`6f15946b4d61d860c9037775a6fbb3e6c303e1be` 리비전에 남아 있다.
정리 당시 `assets.lock`은 기본 최종 11파일만 복원하도록 갱신했으며, 삭제한 중간 작업물은 복원 대상이 아니다.
