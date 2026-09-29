# Character Assets

## Modular Human Male 01 — 첫 교체 파츠 (2026-09-28)

- `d6c71e90` 이후 제작 작업. 사용자 요청에 따라 몸체 추가 보완보다 기본 복장과 교체 파츠를
  먼저 제작한다. [전체 목록·원화·완료 기준](../../assets/modular_human_male_01/parts/README.md).
- 짧은 크롭·옆가르마 헤어, 기본 천 셔츠·가죽 갑옷, 천 바지·가죽 장갑·가죽 부츠 한 세트.
  원화는 OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-09-28 생성.
  OpenAI 생성 출력물 이용 조건 적용. [실제 프롬프트·원화 해시](../../assets/modular_human_male_01/parts/concepts.json).
- 3D는 **Meshy Premium**, Image to 3D API, `meshy-7.1`, Ultra 2K 형상·2K PBR 텍스처,
  2026-09-28 생성. [Meshy 유료 생성물 이용 조건](https://help.meshy.ai/en/articles/10137554-what-is-the-ownership-of-the-generated-models)
  적용. Meshy Community에는 게시하지 않는다. 파츠별 폴더의 `generation.json`에 입력·설정·작업 ID·비용·파일 해시를 보존한다.
- 7종 생성·원본 다운로드 완료. 각 35, 총 **245 API 크레딧** 사용(5,790 → 5,545).
  크롭 933·옆가르마 1,111·천 셔츠 2,087·가죽 상의 2,586·바지 1,553·장갑 한 쌍 1,033·부츠 한 쌍 1,455 triangles.
  [원본 검사](../../assets/modular_human_male_01/parts/source-validation.json)와
  [작업 ID·비용](../../assets/modular_human_male_01/parts/generation-summary.json)을 보존한다.
- 생성 원본은 보존하고, Blender로 [1차 착용본](../../assets/modular_human_male_01/parts/fitted/README.md)을
  만들었다. 기존 `human_male_01_mixamo_candidate_v2`의 65본·bind pose를 사용한다.
  머리 형상·옷깃·소매·손가락 위치를 맞추고 몸체 가중치를 옮겼으며 가림 영역을 분리했다.
  옷깃·손목 경계 보강과 겨드랑이 영역 분리 후 천 셔츠는 **3,321**, 가죽 갑옷은 **3,757 triangles**다.
  헤어·장갑·부츠는 원본 수를 유지하며, 바지의 후속 끝단 변형은 아래에 기록한다.
- 2026-09-29 셔츠 소매가 손과 어긋나는 문제를 수정했다. 전완 중심에 소매를 맞추고,
  손가락으로 잘못 옮겨진 가중치를 제거했다. 소매 끝을 안감·겉감 간격이 있는 열린 커프로
  다시 만들고 전완·손목 가중치로 연결했다. 기존 몸체·다른 파츠 GLB·동작·검 그립은 동일하다.
  Blender 가공이며 원본 출처·라이선스를 유지하고 추가 생성 비용은 없다.
  [손목 검사](../../assets/modular_human_male_01/parts/fitted/sleeve-validation.json)는
  7동작 × 25시점 × 양손에서 커프 중심과 손목 관절의 최대 거리가 **7.36mm 미만**이며
  소매의 손가락 가중치는 0이다. 대기·전투 대기·공격·걷기와 장갑 착용 확대 화면도 남겼다.
  전체 의상 관통이나 소매 표면 품질을 최종 승인한 것은 아니다. 이 보정본은 개발 미리보기용 제작 에셋이다.
- 같은 날 후속 팔꿈치 처짐을 수정했다. 셔츠 밑단의 골반 가중치 규칙이 소매 가중치를 덮어쓰지
  않게 분리하고, 팔꿈치 안쪽 정점도 소매 단면 맞춤에 포함했다. 소매의 골반 가중치는 0이며
  이전 에셋에서 실패하는 회귀 검사를 추가했다. 손목 350표본·기존 700포즈 조립 검사를 통과했고,
  전체 삼각형 수와 다른 파츠는 유지한다. [수정 전 뒤쪽](../../assets/modular_human_male_01/parts/fitted/elbow-before-combat_idle-back.png)·
  [수정 후 뒤쪽](../../assets/modular_human_male_01/parts/fitted/elbow-after-combat_idle-back.png) 비교를 남겼다.
- 개발 미리보기 `/modular-character-preview.html`에서 헤어 2종·상의 2종 교체, 장갑·부츠 착탈,
  머리색·눈색 변경을 연결했다. 머리 재질을 복제하고 홍채만 셰이더 마스크로 염색한다.
  기존 전용 애니메이션과 검 부착 프로파일은 변경하지 않았다. 추가 생성 비용 없음.
- 2026-09-29 바지가 부츠 밖으로 나오는 문제를 수정했다. 부츠용 끝단은 입구 안쪽에
  겹치고, 부츠를 벗으면 원래 폭의 끝단을 표시한다. 바지에는 두 형상과 후속 허리 분할을 포함해 **2,556 triangles**가
  들어 있다. Blender로 기존 Meshy 원본을 가공했으며 출처·라이선스와 추가 생성 비용 없음은 동일하다.
  후속 요청에 따라 발목 폭을 부츠 굵기에 가깝게 넓히고, 좁히기 시작하는 높이를 38cm에서 32cm로
  낮췄다. 입구 부근 24cm에 단면을 추가하고 부츠 표면에 맞춰 안으로 들어가는 부분만 보정했다.
  [부츠 검사](../../assets/modular_human_male_01/parts/fitted/boots-validation.json)는 7동작 × 25시점의
  양발 4개 높이의 단면 517표본에서 최소 2.6mm의 안쪽 여유, 종아리 연결 경계 오차 0, 착탈 20회의 재생 연속성을 확인했다.
  [수정 전](../../assets/modular_human_male_01/parts/fitted/boots-before-walk-side.png)·
  [수정 후](../../assets/modular_human_male_01/parts/fitted/boots-after-walk-side.png)와 맨발 화면을 기록했다.
  발목 폭 보정은 [넓히기 전](../../assets/modular_human_male_01/parts/fitted/cuff-width-before-idle1-front.png)·
  [넓힌 후](../../assets/modular_human_male_01/parts/fitted/boots-after-idle1-front.png) 정면에서도 비교할 수 있다.
- `6388ecda` 이후 허리 보정: 상의를 벗으면 다리 가림 영역이 허리 피부까지 지우던 문제를 수정했다.
  몸체와 바지를 기준 자세 높이 1.055m에서 나누고, 안쪽 피부를 바지 표면보다 안으로 맞췄다.
  허리밴드는 골반 본을 따르며 아래쪽은 기존 가중치로 이어진다. 안쪽 피부도 바지 표면 가중치에 맞춘다.
  몸체는 **12,600**, 바지는 **2,556 triangles**이며 얼굴 지정 영역 **1,505**는 유지한다.
  [허리 검사](../../assets/modular_human_male_01/parts/fitted/waist-validation.json)는 경계와 안쪽 피부의
  490표본 × 175포즈에서 바지까지 최소 **9.4mm**의 안쪽 여유와 상의 교체 20회의 연속성을 확인했다.
  이전 몸체는 허리 가림 경계 검사에서 실패한다. [수정 전](../../assets/modular_human_male_01/parts/fitted/waist-before-combat_idle-front.png)·
  [수정 후](../../assets/modular_human_male_01/parts/fitted/waist-after-combat_idle-front.png)와 뒤쪽 비교를 남겼다.
  [보존 검사](../../assets/modular_human_male_01/parts/fitted/waist-preservation.json)에서 머리·목·팔·손·발과
  바지 끝단 두 변형의 형상·UV·가중치는 동일하다. Blender 가공이며 기존 출처·라이선스를 유지하고 추가 생성 비용은 없다.
- `beb1d6f2` 이후 겨드랑이 보정: 갑옷 착용 시 셔츠 몸통 전체를 숨겨 옆면이 비던 문제를 수정했다.
  기존 셔츠에서 겨드랑이 영역 632 triangles를 분리해 소매와 함께 표시하고, 몸통 중앙만 가린다.
  Blender에서 기존 Meshy 원본을 가공했으며 출처·라이선스를 유지하고 추가 생성 비용은 없다.
  [겨드랑이 검사](../../assets/modular_human_male_01/parts/fitted/armhole-validation.json)는
  7동작 × 25시점 × 양쪽 6개 광선, 총 2,100표본의 가림을 확인했다. 뒤어깨 보정 후 새 영역을 숨기면 그중 696개가 빈 공간을 통과한다.
  상의 교체 30회에도 재생을 유지한다. 표본 검사이며 전체 의상 충돌을 보증하지 않는다.
  [수정 전](../../assets/modular_human_male_01/parts/fitted/armhole-before-combat_idle-left.png)·
  [수정 후](../../assets/modular_human_male_01/parts/fitted/armhole-after-combat_idle-left.png)와 반대쪽·정면·뒤쪽 비교를 남겼다.
- 같은 날 목 보정: 목 피부를 삼각형 중심으로 분류하면서 옷깃 위에 드러나던 가림 경계를 수정했다.
  목·몸통 경계를 앞쪽은 낮고 뒤쪽은 높게 분할하고, 뒷목 옆면은 좁혀 셔츠 등판으로 나오는 피부를 줄였다.
  몸체는 **13,520 triangles**, 얼굴 지정 영역은 **1,505**다. Blender 가공이며 기존 원본·출처·라이선스를 유지하고 추가 생성 비용은 없다.
  [목 검사](../../assets/modular_human_male_01/parts/fitted/neck-validation.json)는 셔츠·갑옷 각각
  7동작 × 25시점 × 목 주변 38표본, 총 **13,300표본**의 가림과 상의 교체 30회의 연속성을 확인했다.
  수정 전 몸체는 동일 검사에서 실패한다. 피부 뒤 10mm 이내의 의상도 가림으로 인정하며, 관통·간격 전체 검사는 아니다.
  [보존 검사](../../assets/modular_human_male_01/parts/fitted/neck-preservation.json)에서 머리·손·전완·발·다리의
  형상·노멀·UV·가중치와 기존 몸체 정점의 위치·UV·가중치를 확인했다. 경계에 새 정점만 보간한다.
  [수정 전](../../assets/modular_human_male_01/parts/fitted/neck-before-leather-combat_idle-front.png)·
  [수정 후](../../assets/modular_human_male_01/parts/fitted/neck-after-leather-combat_idle-front.png)와 셔츠·상의 없는 앞뒤 비교를 남겼다.
- 후속 뒤어깨 보정: 갑옷 어깨판 밖으로 나오던 셔츠를 안쪽으로 맞추고, 해당 위치의 갑옷 가중치를
  주변 셔츠 가중치와 부드럽게 섞었다. Blender로 셔츠 정점 189개를 보정했으며 기존 출처·라이선스를 유지하고 추가 생성 비용은 없다.
  [보존 검사](../../assets/modular_human_male_01/parts/fitted/shoulder-preservation.json)에서 몸체·다른 파츠 GLB 해시,
  셔츠의 삼각형 수 **3,321**·UV와 손목을 포함한 하부 소매의 위치·가중치를 확인했다.
  목·겨드랑이·손목 검사와 기존 700포즈 조립 검사를 다시 통과했다. 어깨의 시각 검토는 대기·전투 대기·걷기·공격 비교이며 전체 의상 충돌 검사는 아니다.
  [수정 전](../../assets/modular_human_male_01/parts/fitted/shoulder-before-leather-combat_idle-left.png)·
  [수정 후](../../assets/modular_human_male_01/parts/fitted/shoulder-after-leather-combat_idle-left.png)와 반대쪽·등·셔츠 단독 착용 비교를 남겼다.
- 2026-09-29 셔츠 밑단을 바지 안으로 넣었다. 기존 Meshy 셔츠의 하부 정점 220개를 Blender로
  짧고 좁게 맞추고 허리밴드와 같은 골반 가중치로 연결했다. 기존 출처·라이선스를 따르며 추가 생성 비용은 없다.
  천 셔츠 착용 시 바지 허리 부분을 표시하고 갑옷 착용 시에는 숨긴다.
  [수정 전](../../assets/modular_human_male_01/parts/fitted/tuck-before-idle1-front.png)·
  [첫 수정 후](../../assets/modular_human_male_01/parts/fitted/tuck-after-idle1-front.png) 비교를 남겼다.
- 후속 옆선 보정은 사용자 제공 [ASCII 스케치](../../assets/modular_human_male_01/parts/shirt-silhouette-reference.txt)를
  참고했다. 등판의 부피를 줄이고 앞뒤가 허리선 근처까지 곧게 내려온 뒤 약 4cm 구간에서 접히도록
  형상·가중치를 조정했다. 허리선 기울기를 따르는 단면 3개를 보강해 셔츠는 **3,957 triangles**다.
  동일한 Meshy 원본의 Blender 가공이며 기존 출처·라이선스와 추가 생성 비용 없음은 유지한다.
  [밑단 검사](../../assets/modular_human_male_01/parts/fitted/tuck-validation.json)는 175포즈 × 224표본에서
  최소 3.84mm의 안쪽 여유와 상의 교체 30회의 재생 연속성을 확인했다.
  [보존 검사](../../assets/modular_human_male_01/parts/fitted/tuck-fold-preservation.json)에서 다른 파츠 해시,
  기존 셔츠 UV 좌표·총 UV 면적과 소매·상부 셔츠 2,117개 삼각형의 위치·UV·가중치가 유지됐다.
  [옆선 수정 전](../../assets/modular_human_male_01/parts/fitted/tuck-fold-before-combat_idle-side.png)·
  [수정 후](../../assets/modular_human_male_01/parts/fitted/tuck-fold-after-combat_idle-side.png) 및 대기·걷기·공격의 앞뒤·옆 비교를 남겼다.
  2026-09-29 사용자 확인 후 모델·작업 파일·재현 스크립트·스케치·검증 기록을 HF·`assets.lock`에 보존했다.
- 장갑·부츠·검 302를 포함한 조합 수치는 다음과 같다. 숨긴 몸체와 안쪽 의상도 전체에 포함한다.

  | 조합 | 전체 triangles | 표시 triangles | 얼굴 지정 영역 |
  | --- | ---: | ---: | ---: |
  | 크롭 + 천 셔츠 | 23,756 | 12,862 | 1,505 |
  | 옆가르마 + 천 셔츠 | 23,934 | 13,040 | 1,505 |
  | 크롭 + 가죽 갑옷 + 안쪽 셔츠 | 27,513 | 14,357 | 1,505 |
  | 옆가르마 + 가죽 갑옷 + 안쪽 셔츠 | 27,691 | 14,535 | 1,505 |

- 4조합 × 7동작 × 25시점(700포즈), 교체 20회, 동작 전환 7회에서 유한한 좌표·공유 골격·재생 연속성을
  확인했다. 소매·바지 끝단·허리·겨드랑이·목·셔츠 밑단 수정 후 같은 조립 검사와 GLB 속성·가중치·입력 해시 검사를 다시 통과했다.
  프런트엔드 check·lint와 관련 테스트 14개도 다시 통과했다.
  [내보내기 검사](../../assets/modular_human_male_01/parts/fitted/export-validation.json),
  [브라우저 검사·착용 화면](../../assets/modular_human_male_01/parts/fitted/browser-validation.json).
- **제작 중인 착용 샘플**이다. 옷깃·소매·갑옷 어깨의 겹침, 장갑의 관절 면과 헤어 표면은 추가 보완이 필요하다.
  갑옷 조합은 15,000–20,000 목표를 초과한다. 텍스처도 원본 해상도의 제작용 출력이므로 배포 최적화가 남아 있다.
  실제 플레이어·저장·장비 동기화에는 연결하지 않았다. 원화는 Git에, 원본·착용본·제작 도구는
  HF 에셋 저장소에 보관하며 함께 커밋한 `assets.lock`으로 복원한다. 소매·바지 끝단·허리 보정과 검증 기록을 포함한다.
  `beb1d6f2` 이후 겨드랑이·목·뒤어깨 보정과 검증 기록도 이번 체크포인트의 HF·`assets.lock`에 포함한다.

## Modular Human Male 01 — 제작 샘플 (2026-09-28)

- [캐릭터 커스터마이제이션](../CHARACTER_CUSTOMIZATION.md)의 기준 몸체 제작 샘플.
  기존 플레이어 모델을 교체하지 않은 작업 에셋이다. 몸체·반바지 분리와 이전 Mixamo 리그
  후보 검사를 마쳤다. 손바닥 방향 수정 후 재리깅으로 손목 뒤틀림을 개선했고,
  후속 작업본에서 손가락 관절·가중치·검 그립을 보정했다. 게임 연결 검증은 남아 있다.
- 원화: [정면](../images/characters/modular_human_male_01/base-front.png),
  [왼쪽 측면](../images/characters/modular_human_male_01/base-left.png).
  OpenAI Codex built-in ImageGen, ChatGPT Pro 20x, 2026-09-28. OpenAI 생성 출력물 이용 조건 적용.
  실제 프롬프트·해시·후면 생성 실패 기록: [concept-source.json](../../assets/modular_human_male_01/concept-source.json).
- 성인 남성, 민머리·수염 없음, 빈손 A포즈, 회색 모델링용 반바지. 체형과 얼굴·손의
  제작 가능성을 먼저 확인하며, 반바지와 몸체는 이후 분리·보완한다.
- Meshy.ai Premium(기존 사용자 확인), Multi-Image to 3D API, `meshy-7.1`,
  2026-09-28. [Meshy 유료 생성물 이용 조건](https://help.meshy.ai/en/articles/10137554-what-is-the-ownership-of-the-generated-models) 적용.
  Community에 공개 게시하지 않는다. 작업 ID `01a0e673-476f-7162-bce0-c9007851039b`.
- 생성 설정: 정면·측면 두 장, Ultra 2K 형상, 리메시 10,000 triangles 목표,
  4K PBR 텍스처, 리메시 전 원본 보존. 최종 조립 캐릭터의 15,000~20,000 목표 중
  기준 몸체 몫이다. 실제 출력 **10,443 triangles**, 리메시 전 원본 **215,016 triangles**.
  머리·목 근사 영역 1,767 triangles(키 상위 18%; 얼굴만의 수치 아님).
- 생성 35크레딧, Meshy 리깅 비교 5크레딧으로 총 **40크레딧** 사용(6,100 → 6,060).
  원본·PBR·Blender 파일·확대 렌더를 보존했다. 얼굴 윤곽·손가락 간격·손의 면 보정과
  복장 분리·홍채 마스크 제작이 필요하며, 아직 제작용 기준 몸체로 승인하지 않는다.
- 리깅 작업 `01a0e679-0cd5-7095-9b8c-27cc9a9dedf2`는 24본·손가락 본 없음.
  기존 Mixamo 팩과 본 이름·척추 계층이 달라 공통 리그에 채택하지 않는다.
  클라이언트 리타게팅으로 `idle1`·`walk`·`run`·`slash1` 각 12개 시점을 측정했지만,
  유한한 좌표 확인만으로 시각적 호환성을 통과한 것은 아니다.
- 원본 OBJ 보존·재질 참조·ZIP 무결성을 확인한 초기 비교용 ZIP은 업로드하지 않았다.
  이후 보정본을 사용자가 Mixamo에서 리깅해 제공했다(아래 기록).
  제작 원본·검증 파일은 HF에 보관하고 `assets.lock`으로 고정한다. 게임에는 연결하지 않았다.
- 비용·다운로드 기록: [generation.json](../../assets/modular_human_male_01/generation.json),
  [rigging.json](../../assets/modular_human_male_01/rigging.json).
  품질 판정·검증 결과·재현 명령: [샘플 README](../../assets/modular_human_male_01/README.md).

### 기준 몸체 보정과 반바지 분리 (2026-09-28)

- 같은 Meshy 원본을 Blender 5.2.0 LTS에서 보정했다. 추가 Meshy 생성은 하지 않았다.
  [몸체·반바지·조립 GLB와 렌더](../../assets/modular_human_male_01/refined/README.md).
- 몸체 **11,730**, 기본 반바지 **1,010**, 합계 **12,740 triangles**(가려지는 몸체 포함).
  머리·목 근사 영역은 1,767 → 3,057, 지정한 얼굴 앞쪽 영역은 **630 → 1,505 triangles**.
  얼굴 450개·나머지 머리 200개 엣지를 세분화하고 고밀도 원본 표면에 투영했다.
- 손가락은 단면 검사에서 원래 분리된 것으로 확인했다. 팔꿈치부터 손목까지 회전을
  보정해 손바닥을 앞으로 돌렸다. 열린 경계·비매니폴드 엣지는 몸체에서 모두 0이며,
  반바지의 열린 경계는 허리·양쪽 다리 세 곳이다. 반바지 아래 몸체는 단순화된 형상이다.
- 보정한 형상에 맞춰 4096² 노멀맵을 다시 베이크했다. 원본의 등 텍스처 반복 오류는
  OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-09-28 생성 출력으로 국소 보정했다.
  OpenAI 생성 출력물 이용 조건 적용. [프롬프트·입력·해시](../../assets/modular_human_male_01/refined/back-repair-source.json).
- GLB 재가져오기·파츠별 지오메트리/UV 일치·텍스처 내장·Mixamo ZIP 무결성 검증 완료.
  공통 리그·무기 그립·홍채 마스크·장비 조립은 이 단계에서 미완료다. 보정 원본·산출물은 HF에
  보관하고 `assets.lock`으로 고정한다. 초기 샘플은 `72ef46aa`와 해당 `assets.lock`에 보존한다.

### Mixamo 리그 후보와 기존 동작 검사 (2026-09-28)

- 사용자 제공 [Idle (7).fbx](../../assets/modular_human_male_01/refined/Idle%20%287%29.fbx).
  Adobe Mixamo에서 리깅·Idle 적용한 파일임을 사용자가 확인했다. 무료 서비스,
  아래 Mixamo 라이선스 적용. 제공일 2026-09-28, 원본 다운로드 날짜는 미확인.
- 65본·양손 다섯 손가락 포함. 단위·원점 정규화, PBR 복원, 정점당 최대 4개 본 가중치로
  몸체·반바지·통합 GLB를 내보냈다. 총량은 **12,740 triangles**를 유지한다.
  반바지 아래 피부 primitive를 숨긴 가시 총량은 **11,730 triangles**(무기·헤어 제외).
- 동일 골격에 별도 파츠를 연결한 결과는 통합본과 일치했다. 기존 이동·전투 동작 7종 ×
  25개 시점 수치 검사 통과. 다만 확대 렌더에서 손목 접힘·검 그립 문제가 있어 시각 검증은
  미통과다. 이동 팩의 새끼손가락 트랙 부재도 기록했다. 기준 리그는 후보 상태다.
- 제공된 Idle은 별도 참조 GLB에 보존했다. 원본 FBX와 공용 애니메이션 팩은 수정하지 않았다.
  [파일·출처 해시·비교 렌더·검증·재현](../../assets/modular_human_male_01/rigged/README.md).
  게임에는 연결하지 않았다. 원본·산출물은 HF에 보관하고 `assets.lock`으로 고정한다.

### 손바닥 방향 수정과 재리깅 입력 (2026-09-28)

- 사용자가 손바닥 정면 자세를 손목 뒤틀림 원인으로 지적했다. 전완을 돌렸던 보정을
  철회하고, 손바닥이 아래·몸통 쪽을 향하는 Meshy 원래 자세로 다시 제작했다.
  [현재 몸체·Mixamo ZIP·검토 렌더](../../assets/modular_human_male_01/refined_palms_down/README.md).
- 얼굴 보강·반바지 분리·UV·등 보정 텍스처를 유지했다. 전완·손 이외의 위치 오차 0,
  전체 **12,740 triangles**, 얼굴 지정 영역 **1,505 triangles**. 변경한 자세에 맞춰
  고밀도 원본에서 노멀맵을 다시 베이크했다. 추가 AI 생성이나 Meshy 비용은 없다.
- GLB 재검사, 이전 UV·베이스컬러 보존, ZIP 무결성·재질 참조 검사를 통과했다.
  이후 사용자가 재리깅한 `Idle (11).fbx`를 제공했다(아래 기록).
- **[미사용]** `refined/`의 손바닥 정면 버전과 `rigged/`의 이전 리그는 비교·출처 보존용이다.
  사용자 FBX는 수정하지 않았다. 원본과 새 작업본은 HF에 보관하고 `assets.lock`으로 고정한다.

### 손 방향 수정 후 Mixamo 리그 (2026-09-28)

- 원본: 사용자 제공 `Idle (11).fbx`, Adobe Mixamo 리깅임을 사용자 확인.
  `/mnt/y/web_downloads/Idle (11).fbx`를
  [로컬 FBX](../../assets/modular_human_male_01/refined_palms_down/Idle%20%2811%29.fbx)로 그대로 보존했다.
  무료 서비스·아래 Mixamo 이용 조건 적용. 제공일 2026-09-28, 별도 다운로드 날짜는 미확인.
- 손가락 포함 65본, 30fps Idle 1~60프레임. 참조 GLB는 2초 Idle을 보존한다.
  `human_male_01_mixamo_candidate_v2`로 몸체·반바지를 내보냈다. 총 **12,740 triangles** 유지,
  기존 수정본과 면 중심 위치 오차는 최대 0.00000084m다. PBR 복원·최대 4본 가중치 정규화 완료.
- 기존 동작 7종 × 25개 시점에서 별도 파츠 조립본과 통합 GLB의 정점 위치 오차 0.
  비교 렌더에서 이전 손목 뒤틀림은 보이지 않는다. 검을 쥐는 손가락의 국소 접힘과 그립 간격,
  이동 팩의 새끼손가락 트랙 부재는 남아 있다. 전체 품질 승인·게임 연결은 아직 하지 않았다.
- [파일·해시·검증·렌더·재현](../../assets/modular_human_male_01/rigged_palms_down/README.md).
  원본 FBX·이전 리그·공용 애니메이션 팩은 수정하지 않았다. 추가 생성 비용 없음.
  새 원본과 산출물은 HF에 보관하고 `assets.lock`으로 고정한다.

### 손가락 관절과 검 그립 보정 (2026-09-28)

- `cf2ad8bb` 커밋 이후의 [보정본](../../assets/modular_human_male_01/rigged_hand_tuned/README.md).
  원본·가공 파일·검증 기록은 HF에 보관하며 함께 커밋한 `assets.lock`으로 복원한다.
- 위 Mixamo 후보와 Meshy·ImageGen 원본을 Blender 5.2.0 LTS, NumPy, Three.js로 가공했다.
  원본의 이용 조건을 따른다. 추가 AI 생성·유료 서비스 사용·비용은 없다.
- 손 부위 698개 정점의 가중치를 보정하고 관절 엣지 350개를 세분화했다.
  몸체 **12,430** + 반바지 **1,010** = **13,440 triangles**. 반바지 아래 피부를 제외하면
  **12,430 triangles**다. 실제 `iron_sword` 302개를 포함한 검토 조합은 총 **13,742**,
  가시 **12,732 triangles**이며 헤어·추가 의상·망토는 아직 없다. 얼굴 지정 영역은 **1,505** 유지.
- 원래 정점·UV 쌍, 65본의 기준 행렬과 반바지 바이트를 보존했다. GLB 재가져오기에서
  몸체 열린 경계·비매니폴드 엣지 0, 반바지는 허리·양쪽 다리 세 경계를 확인했다.
- 기존 동작 7종 × 25개 시점의 조립 검사 통과. 같은 보정 포즈로 비교한 손 표면의
  역방향 면적 진단값은 약 73.4% 감소했다. 이는 자기 관통이 모두 사라졌다는 판정은 아니다.
  전투 대기·공격 확대 렌더에서 접힘 감소와 그립 정렬을 확인했다.
- 손가락 자세와 검 장착 변환은 `hand-grips.json`을 읽는 에셋 검증과 브라우저
  `/modular-character-preview.html`에서 같은 함수를 사용한다. 원본 FBX·공용 동작 팩은 유지했다.
- Chrome WebGL(SwiftShader)에서 175개 포즈, 하의 재장착 10회, 동작 전환 6회,
  연속 재생·보정 비교·390px 화면을 확인했다. 몸체와 하의는 65본의 Skeleton 하나를 공유한다.
  [브라우저 결과·해시·화면](../../assets/modular_human_male_01/rigged_hand_tuned/browser-validation.json).
  실제 GPU 성능·플레이어 연결은 아직 검증하지 않았다.
- 전투 자세의 허리 경계 관통·겨드랑이 접힘은 그림자 수신을 끈 화면에도 남는다.
  최종 리그 품질 승인은 보류한다. 기본 복장·헤어·장비 파츠 제작을 먼저 진행하고,
  해당 변형은 파츠를 착용한 동작 검사에서 필요한 범위로 보완한다(사용자 확인: 2026-09-28).
- 사용자 지적에 따라 검 부착을 손잡이 중심 기준으로 앞쪽에 40도 회전했다.
  이 단계에서는 손목·손가락 자세나 메시를 변경하지 않았다. 전투 대기 129개 시점에서 머리 경계
  영역 간섭이 없고 공격·전환도 검사했다. [결과·수정 화면](../../assets/modular_human_male_01/rigged_hand_tuned/README.md#검-부착-회전-검증).
- 후속 요청에 따라 같은 회전을 유지하며 손잡이를 손목 방향으로 1cm 옮겼다.
  전투 대기·공격에서 오른손 검지·새끼손가락의 두 번째·세 번째 관절을 각각 25%·18%
  기준 자세 쪽으로 펴도록 보정했다. 손목, 다른 손가락과 원본 동작 팩은 유지했다.
- 걷기에는 검을 세운 별도 장착 변환을 적용하고, 걷기 키프레임의 골반 높이를 발바닥
  기준으로 보정했다. 지지하는 발의 높이는 기준 지면에서 약 9.6~12.3cm → 5~7mm다.
  검이 너무 서 있다는 후속 의견에 따라 추가 회전을 절반으로 줄였다(기준 자세 약 70도 → 56도).
  걷기 129개 시점에서 오른쪽 허벅지 메시와의 실제 교차와 머리 경계 영역의 겹침은 0이었다.
  [검사·화면](../../assets/modular_human_male_01/rigged_hand_tuned/README.md#걷기-검-각도와-접지).
- 손이 올라오는 걷기 구간에서는 검 장착값을 왼팔 쪽으로 최대 10도 기울여 검지 쪽 접촉을
  줄였다. 손잡이 중심과 손목 자세는 유지하며 사용자가 걷기 상태를 확인했다(2026-09-28).
- 점프는 복제한 골반 위치 트랙 전체를 약 43.4cm 내려 시작·착지 때 발 높이를 약 1cm로
  맞췄다. 상대적인 점프 궤적·타이밍과 다른 동작은 유지한다. 원본 애니메이션 파일은 그대로이며
  개발 미리보기에서 클립 준비 시 적용한다. 사용자 확인 완료(2026-09-28).
  [검사·화면](../../assets/modular_human_male_01/rigged_hand_tuned/README.md#점프-기준-높이).
- 위 런타임 보정 단계는 `f4a63e49`에 보존했다. 후속 작업에서 손가락·접지 보정을
  전용 애니메이션 GLB로 저장하고 미리보기가 직접 재생하도록 연결했다. 몸체·하의 GLB는 그대로다.
  두 비교 버전 × 7동작 × 65시점의 본·정점 오차는 0이다. 검 장착 설정은 별도로 유지한다.
  [전용 애니메이션 기록](animation.md#modular-human-male-01--baked-animation-preview-2026-09-28).

## Human

- https://sketchfab.com/3d-models/blake-slim-walk-c4d-c076264ca7394357bf3f17837edd72c9 — **[미사용]** 캐릭터 미사용; 걷기 애니는 Mixamo 사용
- https://sketchfab.com/3d-models/xbot-049e4a44ad8b449dba8a2c4824502f5c — **[미사용]** 사용한 적 없음
- "Beauty Girl Exercising - Undressed Workout" (https://skfb.ly/pxpoo) by Polygonal Studios is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/). — **[미사용]** Mixamo 도입 후 삭제
- "Beautiful Realistic Undressed Girls - 14 Anims" (https://skfb.ly/pxpoH) by Polygonal Studios is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/). — **[미사용]** 초기 테스트용, Mixamo 도입 후 삭제
- "Mutant Mixamo" (https://skfb.ly/6DvxK) by NAZTart is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/). — **[미사용]** Mixamo 도입 후 삭제
- "MIXAMO" (https://skfb.ly/ottKO) by sdhkim is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/). — **[미사용]** Mixamo 사이트 알기 전 Sketchfab에서 찾은 애니, Mixamo 도입 후 미사용
- "Bandit Armor and Clothes - Game Model" (https://skfb.ly/6UVot) by wolkoed is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/). — **[미사용]**
- Maria https://sketchfab.com/3d-models/maria-a04cac95ab8046e4bbdc9dec30c7d92d — **[미사용]** 초기 사용, 현재 미사용
- dying https://sketchfab.com/3d-models/dying-98a1d5b2288d49d993039cb161913cd3 — **[미사용]** 정적 dead 포즈 모델(CC-BY, robotgoul); 인게임 death 애니와 다름을 확인 → 캐릭터·애니 소스 아님 (death 클립은 Mixamo 계열)
- medieval_knight https://sketchfab.com/3d-models/medieval-knight-sculpture-game-ready-6cdd055b4afa41eb9360dbbfe75c7f10 — **[미사용]**

## Female Knight

- (초기) ComfyUI에서 jibMixZIT_v10.safetensors로 원화 생성 ![원화](../images/characters/female-knight-concept.png)
- 현재 원화 ![원화](../../client/public/character_concepts/female_knight.webp) (그리기: ComfyUI jibMixZIT_v10, A포즈 변경만 Qwen Image Edit; ChatGPT Pro 20x로 배경 투명화·키 약간 축소, 2026-08-28; WebP q85)
- Tripo(유료 등급)에서 3d 모델로 변환 -> 10k 모델로 리매쉬
- mixamo.com에서 리깅 및 애니메이션 부착
- blender에서 스케일/위치 조정(rest pose 원점 발 밑에 오게) -> 매터리얼 조정 (Shader Editor에서 Alpha 끊기) -> .glb 내보내기
- tools/glb-editor에서 `본 이름 표준화`

## Thief → Rogue

`female_thief.glb`가 `female_rogue.glb`로 개명됨 (클래스 Thief → Rogue, 커밋 7eebc39). 현재 사용 중.

- female_knight와 같은 workflow (3D 생성은 meshy.ai)
- 원화 ![원화](../../client/public/character_concepts/female_rogue.webp) (캐릭터 선택 UI 원화; WebP q85 변환 2026-08-28; 초기 thief 원화 `../images/characters/thief-concept.png`에서 교체)

## Knight

- female_knight와 같은 workflow (3D 생성은 meshy.ai)
- 원화 ![원화](../images/characters/knight-concept.png)
- nano banana2로 A 포즈 ![T-pose](../images/characters/knight-A-pose.png)
- character_concepts 원화 `character_concepts/knight.webp` (Gemini 원본을 ChatGPT Pro 20x로 배경 투명화, 2026-08-28; WebP q85)

## Other Classes

아래 플레이어 클래스는 female_knight와 같은 AI 워크플로우 (ComfyUI 원화 → Nano Banana/Grok 포즈 → 3D 생성 → Mixamo 리깅). 3D 도구는 캐릭터별로 다름(Meshy/Tripo) — License 섹션의 3D 도구 매핑 참조.

- barbarian / female_barbarian — Warrior 대체; 원화: barbarian(남) `character_concepts/barbarian.webp` (Gemini 원본을 ChatGPT Pro 20x로 배경 투명화, 2026-08-28; WebP q85), female_barbarian `character_concepts/female_barbarian.webp` (ChatGPT Pro 20x로 배경 투명화, 2026-08-28; WebP q85)
- caveman / cavewoman — 원화: caveman(남) `character_concepts/caveman.webp` (ChatGPT Pro 20x 생성, 2026-08-28; WebP q85. 이전 Qwen Image Edit 원화는 **[미사용]**), cavewoman `character_concepts/cavewoman.webp` (ChatGPT Pro 20x로 배경 투명화, 2026-08-28; WebP q85)
- priest / female_priest — 원화: priest(남) `character_concepts/priest.webp` (Gemini 원본을 ChatGPT Pro 20x로 배경 투명화, 2026-08-28; WebP q85), female_priest `character_concepts/female_priest.webp` (ChatGPT Pro 20x로 배경 투명화, 2026-08-28; WebP q85)
- ranger — 남성 ranger; 원화 `character_concepts/ranger.webp` (Gemini 원본을 ChatGPT Pro 20x로 배경 투명화, 2026-08-28; WebP q85)
- valkyrie — 단일 성별; 원화 `character_concepts/valkyrie.webp` (ChatGPT Pro 20x로 배경 투명화, 2026-08-29; WebP q85)
- rogue (남) — 남성 rogue 모델; 원화 `character_concepts/rogue.webp` (Gemini/Nano Banana 원본을 ChatGPT Pro 20x로 배경 투명화, 2026-08-28; WebP q85); female_rogue는 [Thief → Rogue](#thief--rogue) 참고

## Bard

`female_bard.glb` — 단일 성별(female). Meshy.ai Premium 등급, 생성 2026-08-05 (프롬프트명 "Crimson Vanguard").

- 원화 ![원화](../../client/public/character_concepts/female_bard.webp) (ChatGPT Pro 20x로 배경 투명화, 2026-08-28; WebP q85)

기존 워크플로우와 다른 점: Meshy 출력이 63개 분리 셸에 뒤집힌 면 403개, 열린 경계 1,093개라 Mixamo 업로드가 실패했다. Blender에서 커스텀 스플릿 노멀 제거 → Recalculate Outside → 8변 이하 구멍만 메움으로 정리 후 업로드 성공. 큰 개구부는 다른 조각에 가려 보이지 않아 남겼다(머티리얼 `doubleSided`).

- Mixamo 업로드용 FBX는 머티리얼 제거 + 텍스처 임베드 해제 필요 — metallic/roughness/emissive가 물려 있으면 "unable to map your existing skeleton"으로 실패
- 텍스처: baseColor 2048² JPEG + normal 1024² JPEG (Meshy 원본 2048² PNG에서 축소)
- emissive 미연결 (Meshy 기본 `EmissiveColor [1,1,1]` 방치 시 백색 발광)

## NPC Models

플레이어 클래스 아님.

- guard — 경비병 NPC Karl (`guard.glb`, CharacterClass::Guard); 원화 `../images/characters/karl-concept.png`, 3D는 Meshy.ai (라이센스는 위 License 표 참조); 거래 창 초상화 `../images/characters/karl-portrait.png` (ChatGPT, 2026-06-12, `doc/assets/ui.md` 참조)
- npc_woman — 상인 NPC Rica (`npc_woman.glb`); 원화 `../images/characters/rica-concept.png` (Gemini) (커밋 fb299e7); 거래 창 초상화 `../images/characters/rica-portrait.png` (ChatGPT, 2026-06-10, `doc/assets/ui.md` 참조)
- maid — 여관 직원 NPC용 메이드 (`maid.glb`, NPC Miriel); 원화 `../images/characters/maid-concept.png` (ComfyUI krea2_turbo_fp8_scaled, 2026-08-31); 거래 창 초상화 `../images/characters/miriel-portrait.png` (2026-09-02, `doc/assets/ui.md` 참조); 3D는 Meshy.ai Image to 3D (유료 등급, 생성 2026-08-30), Meshy 원본 `assets/Meshy_AI_Elegant_Maid_Pose_0830152239_texture_obj.zip`(OBJ, Mixamo 업로드용) + `assets/Meshy_AI_Elegant_Maid_Pose_0830151822_texture (1).glb`(같은 모델의 GLB 재다운로드, 노멀·MR 맵 포함), Mixamo 리깅 65본 `assets/maid_mixamo.fbx` (2026-08-31, Stand To Sit 스킨째). 헤드리스 Blender에서 `fix_mixamo_transforms` → 애니 제거 → 키 1.90m(npc_woman 1.91m 기준), 발 원점 → `mixamorig:` 접두 제거 → 머티리얼은 Meshy GLB 것을 통째로 이식(UV 동일: baseColor 픽셀 일치 확인), emissive/specular 제거 → GLB export → 후처리로 본 노드의 float 오차 scale 제거·노멀/MR 1024² 축소. baseColor 2048² JPEG q94 4:4:4(exporter 기본 q92 4:2:0은 얼굴이 뭉개져 상향; q97 백업 `~/assets_original/maid/maid_q97.glb`), normal·metallicRoughness 1024² JPEG
- pink_maid — 여관 메이드 NPC Cocoly (`pink_maid.glb`); 원화 `../images/characters/pink-maid-concept.png` (ComfyUI krea2_turbo_fp8_scaled, 2026-08-31 이전 생성); 거래 창 초상화 `../images/characters/cocoly-portrait.png` (2026-09-02, `doc/assets/ui.md` 참조); 3D는 Meshy.ai Image to 3D (유료 등급, 생성 2026-08-31, 프롬프트명 "Pink Porcelain Maid"), Meshy 원본 `assets/Meshy_AI_Pink_Porcelain_Maid_0831180703_texture_obj.zip`(OBJ, Mixamo 업로드용) + `..._0831180550_texture.glb`(GLB, 머티리얼 이식용) + `..._0831180601_texture_fbx.zip`(FBX, 미사용), Mixamo 리깅 `assets/Taunt.fbx` (2026-09-01, 검지만 있는 33본 스킨째 — Meshy 메시의 손가락이 붙어 있어 Mixamo가 풀 스켈레톤을 못 만듦; 나머지 손가락 애니 트랙은 무시됨). 가공은 maid 항목과 동일 파이프라인 (키 1.90m, `mixamorig:` 제거, Meshy GLB 머티리얼 이식, baseColor 2048² JPEG q94 4:4:4, normal·MR 1024², q97 백업 `~/assets_original/pink_maid/pink_maid_q97.glb`)
- night_merchant — 야간 상인 NPC Wick (`night_merchant.glb`); 거래 창 초상화 `../images/characters/wick-portrait.png` (ChatGPT, 2026-08-27, `doc/assets/ui.md` 참조); Meshy.ai Premium 등급, 생성 2026-08-08 (프롬프트명 "The Jolly Buccaneer"). OBJ로 받아 Mixamo 리깅(Excited) — Mixamo에서 텍스처가 하얗게 깨져 Blender에서 baseColor 재연결. 손가락 본 없는 33본 스켈레톤(기존 65본과 달리 손가락 애니 안 먹음, 런타임 리타게팅이 없는 본 트랙은 무시). baseColor 2048² JPEG, 노멀맵 없음. .blend 소스 `~/assets_original/night_merchant.blend` (텍스처 팩 포함)
- steward — 영지 관리인 NPC용 (`steward.glb`); 거래 창 초상화 `../images/characters/steward-portrait.png` (사용자 제공 ChatGPT 이미지, 2026-09-07, ChatGPT Pro x20 등급; `doc/assets/ui.md` 참조); 원화 `../images/characters/steward-concept.png` (ChatGPT 이미지 생성, 2026-09-05); 3D는 Meshy.ai Image to 3D (Premium 등급, 생성 2026-09-05, 프롬프트명 "The Master Keykeeper"), Meshy 원본 `assets/steward/Meshy_AI_The_Master_Keykeeper_0905051902_texture.glb`(GLB, 머티리얼 이식용) + `..._0905051916_texture_obj.zip`(OBJ, Mixamo 업로드용), Mixamo 리깅 65본 `assets/steward/Sitting Laughing.fbx` (2026-09-05, 스킨째). `tools/blender-scripts/export_character.py`로 가공 (Blender 5.2.1, 2026-09-05): 애니 제거, 키 1.90m(maid 기준), 발 원점, `mixamorig:` 제거, Meshy GLB 머티리얼 이식(면 단위 UV 일치 검증), emissive 제거, 본 노드 float 오차 scale 제거. 텍스처는 maid의 JPEG 대신 WebP q90 — baseColor 2048², normal·MR 1024² (GLB 1.59MB). 작업 blend `assets/steward/steward.blend`. Land Registrar NPC `Aldwin`에 연결 (2026-09-05), Land Deed 판매. 2026-09-06부터 낮에는 집 1층 61번 의자에 앉고 밤에는 같은 집 2층 60번 침대에서 수면

### Estate Architect (2026-09-06)

- 영지 건축가용 모델 `client/public/models/characters/estate_architect.glb` (NPC `Rowan`, merchant). 2026-09-06 광장에 배치하고 목책·조경 도구함·바닥 재질 견본집 7종 판매를 연결.
- 거래 창 초상화 `../images/characters/estate-architect-portrait.png` — 사용자 제공 ChatGPT 이미지, ChatGPT Pro x20 등급, 2026-09-07; `doc/assets/ui.md` 참조.
- 원화: [estate-architect-concept.png](../images/characters/estate-architect-concept.png). ChatGPT Pro 20x, 생성 2026-09-06, OpenAI 이용약관 적용. 제공된 `assets/ChatGPT Image 2026년 9월 6일 오후 10_22_31.png`를 이동.
- 3D 원본: `assets/Meshy_AI__0906132418_texture.glb`. Meshy.ai Premium, 생성 2026-09-06, 유료 생성물 라이선스 적용(아래 License 참조).
- 리깅 원본: `assets/Defeated.fbx`. Mixamo, 2026-09-06 제공, 손가락 포함 65본. Mixamo 라이선스 적용(아래 License 참조).
- Blender 5.2.0 LTS에서 기존 캐릭터 내보내기 스크립트로 키 1.90m, 발 원점, 회전·스케일 적용, `mixamorig:` 접두 제거. Meshy 재질을 이식하고 면 단위 UV 일치 확인(최대 오차 0). emissive와 원본 Defeated 애니메이션 제거.
- 텍스처: WebP q90, baseColor 2048², normal·metallicRoughness 1024². GLB 1,676,780바이트, 65본, 노드 scale 없음. 기존 클라이언트 리타게팅으로 걷기·달리기·slash1을 적용해 포즈 렌더 확인.
- 작업 파일: `assets/estate_architect/estate_architect.blend` (텍스처 포함). 미리보기: `assets/estate_architect/preview.png`.

재생성:

```sh
blender -b --python-exit-code 1 -P tools/blender-scripts/export_character.py -- \
  --fbx assets/Defeated.fbx \
  --glb assets/Meshy_AI__0906132418_texture.glb \
  --name estate_architect --height 1.90 \
  --out client/public/models/characters/estate_architect.glb \
  --blend assets/estate_architect/estate_architect.blend
```

### Grida / 그리다 — ORKEA 점원 (2026-09-20)

- ORKEA에서 일할 오크 여성 점원. 캐릭터 이름은 **Grida (그리다)**, 에셋 이름은 `grida`.
- 원화: [grida-concept.png](../images/characters/grida-concept.png), 사용자 제공 2026-09-20(원화 생성일 미확인). ComfyUI 로컬 실행, **`krea2_turbo_fp8_scaled.safetensors`** (Krea 2 Turbo FP8; 사용자 확인 및 PNG 메타데이터 일치). 기본 모델 라이선스는 [Krea 2 Community License Agreement](https://www.krea.ai/krea-2-licensing); 출력물 소유권은 §5.3, 상업 이용 조건은 §2.3을 따른다. 사용한 LoRA 정보는 원본 PNG의 ComfyUI 메타데이터에 보존.
- 상점 창 초상화: [grida-portrait.png](../images/characters/grida-portrait.png) → `client/public/portraits/grida.webp`(원본 상단 2/3 크롭, 512×341, alpha 유지). 기존 원화의 얼굴·머리·의상을 참조한 상반신 구도. OpenAI Codex built-in ImageGen, ChatGPT Pro x20 등급(사용자 확인), 2026-09-20; 출력물 이용 조건과 프롬프트는 [UI 에셋 문서](ui.md#npc-거래-초상화) 참조.
- 3D 생성: Meshy.ai **Premium** 등급(사용자 확인), Image to 3D API, 2026-09-20. [Meshy 유료 생성물 소유권 조건](https://help.meshy.ai/en/articles/10137554-what-is-the-ownership-of-the-generated-models) 적용. Meshy Community에 공개 게시하지 않음.
- 생성 설정: `ai_model=meshy-7.1`, `model_type=standard`, `should_remesh=true`, `topology=triangle`, `target_polycount=10000`, A 포즈, PBR 텍스처 2048², `image_enhancement=false`로 원화 외형 유지. GLB·FBX·OBJ 요청.
- 작업 ID: `01a0ba74-d31f-7744-bc58-31ef8c01e280`. 요청·결과 기록은 `assets/grida/generation.json`, 원화 출처 기록은 `assets/grida/concept-source.json`.
- Meshy 원본: `assets/grida/grida_meshy.glb` 및 `texture_0_*.png`. 최종 재생성에 필요한 원본과 PBR 텍스처를 보존한다.
- **[미사용]** 리깅 전 검토본, 중복 Meshy FBX·OBJ·MTL, Mixamo 업로드 ZIP, 미리보기·포즈 데이터·일회성 검사 스크립트·중복 API 응답은 2026-09-21에 삭제했다.
- 게임 모델: `client/public/models/characters/grida.glb` — **9,812 triangles**, 손가락 포함 **65본**, 키 1.90m, 발밑 원점, 1,972,300바이트. `Grida` 이름의 NPC 모델로 연결.
- 리깅 원본: 사용자 제공 `/mnt/y/web_downloads/Idle (5).fbx` → `assets/grida/grida_mixamo.fbx` (Mixamo, 2026-09-20, 무료 서비스; 아래 Mixamo 라이선스 참조). 포함된 Idle 애니메이션은 제거하고 기존 게임 애니메이션 팩을 사용.
- Blender 5.2.0 LTS에서 `tools/blender-scripts/export_character.py`로 변환. Meshy 텍스처 이름만 역할별로 정규화한 임시 GLB를 사용해 재질 이식, 면 단위 UV 최대 오차 0 확인. 키·발 원점 적용, `mixamorig:` 접두·emissive·본 scale 오차 제거. WebP q90, baseColor 2048², normal·metallicRoughness 1024². 작업 파일: `assets/grida/grida_rigged.blend`. 재현: `.venv/bin/python assets/grida/export_rig.py`.
- 검증: GLB 본 이름·스킨 가중치·내장 텍스처·키·원점 확인. 실제 클라이언트 리타게팅으로 idle1·walk·run을 각각 12개 시점에서 검사하고 포즈 렌더를 확인했다. 검증 요약과 원본·결과 해시는 `assets/grida/generation.json`의 `rigging`에 보존한다.
- NPC 레지스트리 `grida` / `Grida`, 한국어 별칭 `그리다`. ORKEA 서쪽 계산 구역 옆 `(-1452.0, 1.0, 4777.0)` 근무 일정과 전시품·카트·출입구 결제 안내를 연결. 별도 개인 상점은 없으며 ORKEA의 기존 결제 흐름을 사용.
- 생성 비용: **30 API 크레딧**(5,096 → 5,066).

### Tobin / 토빈 — 강가의 낚시꾼 (2026-09-21)

- 낚싯대를 팔고, 곁에서 한 차례의 낚시를 관찰하면 낚시 스킬을 가르치는 인간 남성 NPC. 이름은 **Tobin (토빈)**, 에셋 이름은 `tobin`.
- 원화: [tobin-concept.png](../images/characters/tobin-concept.png), 1024×1536 PNG. OpenAI Codex built-in ImageGen, ChatGPT Pro 20x, 생성·수정 2026-09-21. OpenAI 생성 출력물 이용 조건 적용.
- 상점 창 초상화: [tobin-portrait.png](../images/characters/tobin-portrait.png) → `client/public/portraits/tobin.webp`(512², alpha 유지). 기존 원화의 얼굴·복장을 유지한 가슴까지의 투명 배경 초상화. OpenAI Codex built-in ImageGen 편집, ChatGPT Pro 20x, 2026-09-21. 출처·출력물 이용 조건과 편집 프롬프트는 [UI 에셋 문서](ui.md#npc-거래-초상화) 참조.
- 햇볕에 그을린 얼굴과 짧은 희끗한 수염의 친근한 중년 낚시꾼. 리넨 튜닉·가죽 조끼·모직 바지·챙 없는 모직 모자·가죽 신발의 중세풍 복장. 청바지와 현대적인 모자가 있던 초안은 **[미사용]**이며, 사용자 요청으로 복장을 수정했다.
- Meshy 변환과 Mixamo 리깅을 위해 빈손의 정면 A포즈로 제작했다. 낚싯대는 별도 장착 아이템으로 사용한다.
- 실제 생성·편집 프롬프트와 출처·해시: [concept-source.json](../../assets/tobin/concept-source.json).
- 3D 생성: Meshy.ai **Premium**(사용자 확인), Image to 3D API, `meshy-7.1`, 2026-09-21. [Meshy 유료 생성물 소유권 조건](https://help.meshy.ai/en/articles/10137554-what-is-the-ownership-of-the-generated-models) 적용. Meshy Community에 공개 게시하지 않았다.
- 10,000 polygons 목표로 생성한 **10,370 triangles**, 빈손 A포즈, PBR 텍스처 2048². 추가 폴리곤 감면 없이 GLB·FBX·OBJ와 텍스처를 다운로드했다. 작업 ID `01a0bfbf-24c2-74cb-aebb-2a24c7a883dc`, 비용 **30 API 크레딧**(4,976 → 4,946). [생성 설정·결과·해시](../../assets/tobin/generation.json).
- 재생성에 필요한 [PBR 원본 GLB](../../assets/tobin/tobin_meshy.glb), Mixamo 리깅 FBX, Blender 작업 파일, 원본 `texture_0_*.png`를 보존한다. GLB의 베이스컬러·metallicRoughness는 JPEG이므로 원본 PNG도 유지한다. 사용 완료한 업로드 ZIP과 리깅 전 FBX·OBJ·MTL, 이전 미리보기·임시 로그·중복 검증 JSON은 사용자 요청으로 삭제했다(2026-09-21). [파일 안내](../../assets/tobin/README.md).
- Mixamo 업로드 후 **텍스처 정상 표시를 사용자 확인**(2026-09-21). ZIP 루트의 OBJ·MTL·베이스컬러와 재질 참조를 맞춘 구성이며, 이후 캐릭터에도 같은 [업로드 준비 방식](creation-guidelines.md#mixamo-upload-preparation)을 사용한다.
- 리깅 전 GLB·FBX·OBJ는 메시 1개·10,370 triangles·UV 1개·2048² 텍스처·본 0개로 검증했다. 검사 결과와 삭제 파일 기록은 [generation.json](../../assets/tobin/generation.json)의 `validation`·`cleanup`에 보존한다.
- 리깅 원본: 사용자 제공 `/mnt/y/web_downloads/Idle (6).fbx` → [tobin_mixamo.fbx](../../assets/tobin/tobin_mixamo.fbx). Mixamo(Adobe), 2026-09-21, 무료 서비스; 아래 Mixamo 라이선스 참조. 포함된 Idle 애니메이션은 제거하고 기존 게임 애니메이션 팩을 사용한다.
- 게임 모델: [tobin.glb](../../client/public/models/characters/tobin.glb) — **10,370 triangles, 33본**, 키 1.90m, 발밑 원점, WebP q90. 베이스컬러 2048², 노멀·metallicRoughness 1024². 양손에 검지 체인만 있는 간소화된 리그로, 엄지·중지·약지·소지를 각각 제어할 수는 없다.
- Blender 5.2.0 LTS에서 공용 `tools/blender-scripts/export_character.py`로 원본 재질을 이식했다. 면 단위 UV 최대 오차 0, `mixamorig:` 접두·본 scale 오차 제거. 텍스처를 내장한 [Blender 작업 파일](../../assets/tobin/tobin_rigged.blend), 재현: `.venv/bin/python assets/tobin/export_rig.py`.
- 실제 클라이언트 리타게팅으로 `idle1`·`walk`·`run`·`fishing_cast`·`fishing_idle`을 각각 12개 시점에서 검사하고 동작 렌더를 검토했다. 검증 JSON 3개는 [generation.json](../../assets/tobin/generation.json)의 `rigging.export_report`·`rigging.validation`·`npc_placement.verification`에 통합하고 동작 미리보기는 삭제했다. Grida와 같은 보관 기준으로 폴더에 11개 파일을 유지한다. 클라이언트 모델 경로를 NPC 이름 `Tobin`에 연결했다.
- NPC 레지스트리 `tobin` / `Tobin`, 한국어 별칭 `토빈`. **world(-1499.9, 0.6, 4728.4), 방향 -89.0°**, tile(-23, 74), cell(4, 24)의 강가에 배치. `fishing_rod`를 작업 장비로 장착하고 캐스팅·입질 대응·실제 물고기 획득을 반복한다. 찌·바깥 낚싯줄은 실제 낚시 중에만 표시하며, 매 시도 사이에는 잠시 쉰다. [일정](../../agent-client/data/npcs/tobin/schedule.json), [낚시 연출과 구현 범위](../FISHING.md#토빈-배치-2026-09-21-구현). 상점에서 낚싯대를 기본 가격 3실버에 판매하며, 6m 이내에서 캐스팅부터 포획까지 관찰하면 낚시 스킬을 영구 습득한다.

## 텍스처 재패킹 (2026-08-06)

Meshy/Tripo 내보내기가 노멀·metallicRoughness 맵을 2048² RGBA PNG로 임베드해
캐릭터당 12~15MB였다. `tools/repack-glb-textures.py`로 전체 재패킹:
노멀·MR은 JPEG 4:4:4 1024²(q92/q90), 베이스컬러는 해상도 유지한 채 JPEG q92
(female_knight만 PNG였음), 플랫 노멀맵(knight, npc_woman)과 알파가 상수라
무의미했던 specularTexture(female_knight)는 제거.
합계 168.5MB → 33.9MB, 텍스처 VRAM 900MB → 464MB.
스크립트는 멱등이라 재실행해도 재인코딩하지 않는다.

베이스컬러 축소는 보류. `--base-max 1024`면 23.5MB/VRAM 229MB까지 내려가지만
선택 화면 크기에서 사슬갑옷·문장 같은 패턴 면이 뭉갠다(guard 45dB가 최악).
1536은 NPOT 리샘플 탓에 1024보다도 나쁘니 중간값은 없다.

## License (AI 제작 캐릭터)

위 AI 워크플로우로 만든 플레이어 캐릭터 전부(knight, barbarian, caveman, priest, rogue, ranger, valkyrie의 male/female)의 도구별 라이센스. 3D 도구는 female_knight만 Tripo, 그 외 전부 Meshy. (조사 2026-07, 약관 변경 가능)

| 단계 | 도구 | 라이센스 | 비고 |
|------|------|---------|------|
| 원화 | ComfyUI + jibMixZIT / Z-Image Turbo / Qwen Image Edit | Apache 2.0 | 상업 OK, 표시 의무 없음 (로컬 실행) |
| T/A 포즈 | Nano Banana(Gemini) / Grok | 출력물 사용자 소유, 상업 OK | 전 등급 동일, IP 배상 없음 |
| 3D 메쉬 (대부분) | Meshy.ai (유료 생성) | 완전 소유권, 상업 OK | 무료 다운그레이드해도 유지 (CC-BY 전환 안 됨) |
| 3D 메쉬 (female_knight) | Tripo (유료 Pro+ 생성) | 유료=완전 상업권 | ⚠️ 다운그레이드 후 유지 여부 약관 미명시 — 인보이스 보관·support 문의 |
| 리깅/애니 | Mixamo (Adobe) | 무료·로열티 없음·상업 OK | 원본 파일 단독 재배포 금지, 임베드는 OK |

핵심 조건:

- Meshy: 유료 때 생성분은 상업권 영구 유지. 단 ① Meshy Community에 공개 게시 안 함, ② 입력물이 타 저작권 미침해(위 원화·포즈 체인은 Apache 2.0/사용자 소유라 충족).
- Tripo: 유료 생성 시점엔 완전 상업권이나 **다운그레이드 후 유지 여부가 약관에 없음** (Meshy보다 리스크). 상업화 전 support@tripo3d.ai 확인 권장.
- **3D 도구 매핑** (Tripo=리스크, Meshy=안전): Tripo = female_knight (유일) / Meshy = 그 외 캐릭터·NPC 전부.
- 입증 대비: **Meshy·Tripo 결제 인보이스 + 생성 날짜** 보관 (유료 시점 생성 증빙).
- AI 생성 이미지는 저작권 보호가 약해 독점권 주장은 어려움(사용은 무방).
- Mixamo "단독 재배포 금지" 판단(2026-08-13): 애니메이션은 독립 배포물이 아니라 OpenMMO
  게임의 일부로 딸려나가므로 임베드에 해당한다고 본다. HF 데이터셋(`assets.lock`)과
  `assets/all_animation.blend`도 같은 게임의 빌드 소스로 취급한다.
- 같은 날 애니메이션 팩 GLB에서 Mixamo 캐릭터 메쉬(Medea)와 텍스처를 걷어냈다
  (36.5MB → 2.4MB). 런타임이 안 읽는 데이터라 크기·성능 목적 — [animation.md](./animation.md) 참조.

출처: [Meshy 취소 시 라이센스](https://help.meshy.ai/en/articles/9992023-if-i-cancel-my-subscription-will-all-my-models-revert-to-a-cc-by-4-0-license), [Tripo 약관](https://www.tripo3d.ai/terms), [Tripo 라이센스 가이드](https://www.tripo3d.ai/game-development/3d-assets-license-game-development), [Mixamo FAQ](https://helpx.adobe.com/creative-cloud/faq/mixamo-faq.html), [jibMixZIT](https://civitai.com/models/2231351/jib-mix-zit), [Z-Image Turbo](https://huggingface.co/Tongyi-MAI/Z-Image-Turbo)
