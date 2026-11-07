# 여성 기사 게임 적용 — 2026-10-14

캐릭터 생성에서 여성 기사를 선택하면 새 모듈형 여성 베이스와 기사 투구·상의·하의·장갑·부츠를 표시한다.
서버는 기존 `worn_plate_helmet`, `worn_breastplate`, `worn_plate_greaves`,
`worn_plate_gauntlets`, `worn_plate_boots`를 각 슬롯에 지급·장착한다.
기존 남성 기사와 같은 성능·거래 불가 기본 아이템이며 여성용 외형을 선택한다.
기존 여성 기사에게는 배포 중 서버와 NPC를 멈춘 뒤 `tools/grant-female-knight-starter-armor-20261014.py`로 1회 지급한다.
미리보기 후 `--apply --expected-preview`로 실행하며, 인벤토리에 없는 품목만 가방에 추가하고 기존 장착·강화는 보존한다. NPC 및 다른 여성 직업의 모델은 유지한다.

## 제작과 재현

`node tools/prepare-female-knight.mjs`로 `client/public/models/characters/modular_female/`을 만든다.
[입출력 해시](female-knight-runtime-sources.json)는 원본과 게임용 GLB를 연결한다.
보정 동작 `preview/animations.json`은 로컬에서 생성하므로 해시 대신 생성 입력을 기록한다.
기존 형상·UV·65본·유효 스킨 가중치와 가림 인덱스를 유지하고 텍스처를 WebP로 변환한다.
가중치가 0인 스킨 슬롯의 사용되지 않는 본 인덱스는 0으로 정리한다.
컬러는 최대 2048·WebP q94, 기타 텍스처는 최대 1024·WebP q90이며 사용 중인 속눈썹 텍스처는 보존한다.
`bottom`·`rogue_bottom`·`rogue_top`·`barbarian_helmet`과 원시전사·사제·순찰자·음유시인 부위는 컬러도 1024다(barbarian_helmet은 2026-11-07 전송량 1위라 1.29 → 0.68MB).
원본 피팅 파일에 남은 참조 없는 accessor는 제거한다(2026-10-15, 7개 GLB 26.5 → 13.7MB).
원본이 미터 단위라서 좌표 변환 없이 그대로 쓴다(아래 [원본 미터 단위 전환](#원본-미터-단위-전환--2026-10-17)).
남성 모듈러와 같이 게임 루트 배율은 `(1, 1, 1)`이며 기존 표시 키를 유지한다. 손 소품에는 역배율을 적용하지 않는다.
기존 승인 동작을 우선 사용하고 나머지 게임 애니메이션은 같은 여성 리그에 미리 리타기팅한다.
리타기팅·접지 대상은 게임용 `base.glb`다. 2026-10-17 이전에는 제작 단위 리그에 미터 동작을 리타기팅한 뒤 1.8배를 다시 적용해, 리타기팅 동작의 Hips 이동이 1.8배였다(침대에 누운 몸이 0.93m 밀려나고, 승마 시 Hips가 0.9m 낮았다).
부츠 발 자세·접지 보정과 검 쥔 손가락 자세는 이 도구가 완성된 게임용 GLB를 다시 읽어 굽는다.
부츠 값은 `<클립>|femaleBoots` 보조 애니메이션(발 4본 회전, Hips 이동)에 넣고, 클라이언트는 이를 맨발 트랙과 짝지어 캐릭터마다 부츠 착용 여부로 하나를 고른다.
검 쥔 손가락 자세는 남성 모듈러처럼 무기와 관계없이 대기·걷기·달리기·전투 대기 클립에 넣는다.
앉기 보정과 피부·소매 가림은 개발 미리보기와 게임이 같은 함수를 사용한다.

이 작업은 기존 에셋의 로컬 변환이며 추가 유료 생성이나 새 원화를 사용하지 않았다.
원본 라이선스·구독 등급은 [여성 베이스](modular-human-female-01.md),
[헬멧](modular-female-knight-helmet.md), [상의](modular-female-knight-top.md),
[하의](modular-female-knight-bottom.md), [장갑](modular-female-knight-gloves.md),
[부츠](modular-female-knight-boots.md)의 출처 기록을 따른다.
헬멧·장갑 원화의 ImageGen 등급은 ChatGPT Pro 20x이며 Tripo 유료 API 결과를 재사용한다.

### 베이스 다운로드 최적화 — 2026-10-25

게임용 `base.glb`에서 미사용 속눈썹 재질과 1,170,347바이트 PNG를 제거하고 Meshopt 무손실 압축을 적용했다.
GLB는 4,552,916 → 2,107,164바이트, gzip level 6은 2,953,392 → 1,572,907바이트(46.7% 감소)다.
제작 원본은 보존하며, `prepare-female-knight.mjs`도 베이스를 쓸 때 같은 정리·압축을 적용한다.
공용 headless 로더는 압축된 런타임 베이스를 읽을 수 있도록 Meshopt를 기본 활성화한다.

기존 비압축 런타임 파일만 변환하려면 다음 명령을 사용한다.

```sh
node tools/compress-glb-meshes.mjs SOURCE.glb OUTPUT.glb --lossless
```

`--lossless`는 정점 병합·재배열·양자화와 텍스처 재인코딩을 하지 않는다.
의상 가림에 쓰는 정점 번호와 삼각형 순서·방향은 유지한다(삼각형 내부 인덱스의 순환 회전만 허용).
Three.js로 해독한 정점·스킨·본 변환·27개 애니메이션의 642개 트랙 및 사용 텍스처 바이트가 기존과 같았다.
[입출력 기록의 `base_optimization`](female-knight-runtime-sources.json)에 상세 결과가 있다.

### 소셜 동작 다운로드 최적화 — 2026-10-28

`animations/social.glb`의 동작 데이터를 양자화 없이 Meshopt로 압축한다.
GLB는 2,584,864 → 1,689,720바이트, gzip level 6은 1,379,542 → 962,593바이트
(30.2% 감소)다. `node tools/prepare-female-knight.mjs --pack social`로 재생성한다.
Three.js 디코더로 비교한 80개 클립의 980개 트랙·124,579개 키프레임과 노드 변환이 동일하다.
게임의 로더 경로를 이용한 앉기 동작 테스트도 수행한다.
게임용 바이너리는 Hugging Face에 게시하고 `assets.lock`에 고정했다.

### 동작 팩 다운로드 최적화 — 2026-10-29

소셜과 같은 무손실 Meshopt 압축을 `animations/` 10팩 모두에 적용하고, 중복 시간·값 배열을 공유한다.
GLB 합계는 6,841,524 → 2,954,612바이트, gzip level 6은 3,113,681 → 2,044,808바이트
(34.3% 감소)다. `locomotion.glb`는 817,303 → 198,934, `social.glb`는 992,569 → 950,927바이트(gzip)다.
Three.js 디코더로 비교한 224개 클립의 3,624개 트랙·354,042개 키프레임, 노드 변환과 장면 메타데이터가 비트 단위로 같다.
`prepare-female-knight.mjs`는 모든 팩에 같은 압축을 적용한다. 이번에는 재생성하지 않고 현재 파일을 직접 압축했다.

## 장비 교체와 검증

투구를 해제하면 단발 헤어를 복원하고, 장갑을 해제하면 소매를 복원한다.
피부 가림은 캐릭터별 복사본에 적용하여 다른 플레이어나 캐릭터 선택 슬롯에 영향을 주지 않는다.
갑옷을 전부 해제하면 원래 베이스로 돌아온다. 장비별 대체 외형은 아래 야만용사 복장 규칙을 따른다.
외형 커스터마이즈 UI는 기존 지원 범위를 유지한다.

기사 세트의 숨김 포함 삼각형은 66,017, 실제 표시 51,538이며 얼굴 4,604면을 유지한다.
숨긴 단발 헤어와 검·횃불·망토는 이 합계에 포함하지 않는다. 원본 예산 초과 상태에서 임의 감축하지 않았다.

- 서버 테스트: 여성 기사 기본 장비 5종의 수량·장착 슬롯·캐릭터 목록 장비 반환, 기존 남성 기사 지급.
- 런타임 테스트: 착탈과 가림 복원, 캐릭터 간 격리, 동작별 유한 변형·투구 강체 유지·부츠 보정.
- 배율 적용 전후 51개 동작의 256개 자세에서 메시 표본·전체 본 위치를 비교했다. 최대 월드 좌표 오차는 약 `1.8e-7 m`이며 키와 접지를 유지한다. 루트·손 본 배율 1과 손 소품 크기도 검사한다.
- 브라우저: 실제 CharacterCreateScene과 CharacterPreview의 여성 기사 표시, 성별 전환, 착탈 복원. 실제 PlayerModel에서 기본 검과 장비 착용 표시도 확인했다.
- 프런트엔드 타입·린트, Rust 형식·컴파일 검사.

게임용 GLB 16개 검사는 오류 0개다. 기존 몸체·부츠의 탄젠트 자동 생성 경고 3개는 유지하며 실제 WebGPU 표시를 확인했다.

## 여성 야만용사 적용 — 2026-10-16

여성 야만용사도 같은 모듈형 여성 베이스를 쓴다. 서버는 남성과 같은 `worn_barbarian_*` 5종을 지급·장착하고,
캐릭터 생성 화면은 뿔 투구까지 입힌 모습을 보여준다.
`prepare-female-knight.mjs`가 `barbarian_{top,bottom,boots,gloves,helmet,hair}.glb`를 함께 만든다.
`barbarian_hair`는 뿔 투구 아래에 맞춘 단발이며 투구를 쓰면 기본 단발 대신 표시된다.
부위마다 판금·야만용사 외형을 따로 고르므로 섞어 입을 수 있다. 야만용사 전용 장비와 가죽 소재 장비는 야만용사 외형으로 표시한다. 일반 가죽 방어구, 로그·순찰자·원시인 복장, 사제·음유시인의 가죽 부츠가 여기에 해당한다. 나머지 금속·천 장비는 판금 외형으로 대체한다. 남성의 장비 외형 선택에는 영향을 주지 않는다.

샌들과 기사 부츠는 발 회전은 같지만 밑창이 달라 접지 높이가 달리기에서 최대 약 4.8cm 차이 난다.
그래서 보정 동작을 부츠 부위별 `<클립>|femaleBoots:<부위>`로 굽고, 클라이언트는 [맨발, 기사 부츠, 샌들] 값 중 하나를 고른다.

모피 치마 4장의 `pelt_physics`는 미터 값과 `unit: 1.8`을 원본에 담는다.
`pelt-rig`의 내장 거리·중력은 튜닝 당시 1/1.8 크기 리그 기준이라 `unit`만큼 키운다. 게임은 `robe_physics`를 쓴다.
모피 파츠는 정점 위치를 직접 고쳐 쓰므로 인터리브하지 않고 저장한다.

원본 출처·라이선스는 [상의](modular-female-barbarian-top.md), [하의](modular-female-barbarian-bottom.md),
[샌들](modular-female-barbarian-boots.md), [손목 보호대](modular-female-barbarian-bracers.md),
[투구·헤어](modular-female-barbarian-helmet.md) 기록을 따른다. 추가 유료 생성은 없다.

기존 여성 야만용사에게는 배포 중 서버와 NPC를 멈춘 뒤 `tools/grant-female-barbarian-starter-armor-20261016.py`로 1회 지급한다.
사용법은 기사 지급 스크립트와 같다(미리보기 후 `--apply --expected-preview`, 가방에 없는 품목만 추가).

## 원본 미터 단위 전환 — 2026-10-17

여성 모듈러 원본은 미터 단위다(키 약 1.8m, Hips 높이 약 1.02m). 제작 단위(리그 높이 1, Hips 0.568)의 원본을 1.8배해 고정했고,
`prepare-female-knight.mjs`의 1.8배 변환(`bakeDimensions`), 읽을 때의 동작 변환과 `meters_per_source_unit` 메타데이터를 없앴다.

- 변환한 원본: 리그 `rig_v1/female-base.glb`, 단발·긴 헤어와 두 헤어의 리깅 입력 `hair-fitted-unskinned.glb`,
  기사 상의 v3·하의 v42·장갑 v3·투구 v1·부츠 v4·v5, 야만용사 상의·하의·샌들·손목 보호대·투구·투구용 헤어 2개.
  좌표가 든 보조 데이터(`heel-pose.json`, `heel-animation-offsets.json`, `fit-data.npz`, `sleeve-adjustment.npz`,
  `hair-cheek-coverage-anchors.json`, 피팅 기록의 높이 값)도 1.8배했다. Tripo 원본 좌표 `source`와 삼각형 인덱스인 가림 JSON은 그대로다.
- 게임용 GLB 23개를 다시 만들어 전환 전과 비교했다. 메시·스킨·역바인드·애니메이션 트랙이 비트 단위로 같고,
  `animations/offhand.glb` 횃불 동작의 Hips 이동만 부동소수 오차(최대 1.19e-7m)만큼 다르다.
- 미터 기준 도구: 피팅 `fit-female-barbarian-{bracers,helmet,boots}.py`, `fit-female-knight-{boots,gloves,helmet}.py`,
  `shape-female-knight-boots.py`, `trim-female-barbarian-helmet-hair.py`,
  `outfits/{fit_female_knight_top_surface,repair_sandal_panel,female_boot_material,female_barbarian_top_occlusion}.py`,
  Blender `fit_female_knight_{bob,hair}.py`·`fit_female_barbarian_top.py`·`rig-female-knight-{bob,hair}.py`·`review_female_*.py`,
  동작 `lib/female-seated.mjs`·`animate-female-base.mjs`, 검사 `validate-female-*.{mjs,py}`·`export-female-barbarian-boots-poses.mjs`.
  전환 전후 같은 명령의 통과·실패가 같았다(샌들 끈 관통 검사 3개는 전환 전부터 실패).
- 전환하지 않은 파일: `.blend` 편집본(Blender에서 열면 제작 단위, 검수 스크립트를 다시 실행하면 미터로 다시 저장),
  `catwalk-source.glb`(Mixamo 자체 단위, Hips 높이 비율로 리타기팅).

2026-10-18 정리에서 제작 단위에서만 재현되던 단계 파일과 도구를 삭제했다. 리깅 이전 얼굴 보정 단계, 기사 하의 v1–v41, 상의 v2,
야만용사 하의 수리 단계와 1회용 변환 스크립트가 여기에 해당한다. 필요하면 git `d0e3f3de`와 HF 리비전 `8b01d1fb`에서 복원한다.
