# 여성 트윈테일

2026-11-19. Miriel을 모듈형으로 옮기며 만든 헤어다([메이드복](modular-female-maid-tripo.md)). 플레이어도 고를 수 있다. 절차는 [긴 생머리 두 종](modular-female-long-straight-hair.md)과 같다.

| | |
|---|---|
| 저장값 `HairStyle` | `twintail` (신규, 프로토콜 164) |
| 게임 파츠 | `twintail_hair.glb` (고른 캐릭터만 지연 로드) |
| 작업 폴더 | `assets/modular_human_female_01/hair/tripo_twintail_v1` |
| 헤어 삼각형 (생성 전체) | 2,694 (9,512) |
| 기본 색(텍스처 면적 가중 평균) | `#643625` |
| 생성 화면 라벨 | 트윈테일 |

## 원화

OpenAI Codex built-in ImageGen(`codex exec`), **ChatGPT Pro 20x**, 2026-11-19, OpenAI 생성 출력물 이용 조건. 프롬프트는 작업 폴더의 `worn-*-prompt.txt`.

- `tripo_long_side_v2/mannequin-*.png`(민머리 파란 마네킹 정면·옆·뒤) 위에 그렸다. 정면은 번 헤어 시안 `tripo_bun_v1/worn-front.png`의 앞머리와 색을 참고했고, 뒤와 옆은 앞 결과를 참고했다.
- 눈썹 위까지 오는 일자 앞머리, 귀 앞의 가는 옆머리, 가운데 가르마, 귀 위 뒤쪽에 검은 끈으로 묶은 두 꼬리. 꼬리는 어깨 뒤로 넘어가 견갑골 높이에서 끝난다.
- 첫 시안은 꼬리가 어깨 앞으로 내려왔다. 사용자 요청으로 뒤로 넘겨 다시 그렸고 그 그림은 남기지 않았다.
- `worn-front.png` `b61e6a7e…9aa8`, `worn-side.png` `0d0e3657…44f2`, `worn-back.png` `45b4c848…d41d`. 세 장 모두 사용자 승인.
- 번 헤어 시안 세 장은 `tripo_bun_v1`에 있다. **[미사용]** Tripo에 제출하지 않았다(사용자 결정: 트윈테일 먼저).

## 3D

- Tripo API 유료 크레딧, `P2-20260801`, 텍스처 `v3.5-20260815`, detailed PBR, face_limit 10000, 다중 시점 front·left·back, seed 111902. 작업 ID `68c6b517-e7b1-4efc-a334-de6a9d84748b`, 120크레딧(1600 → 1480). [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
- 출력 `model_url.glb`, SHA-256 `4bfeb6cb…ab43`.

## 분리와 피팅

```bash
cd tools && ../.venv/bin/python split-female-long-hair.py --style twintail
blender -b --python-exit-code 1 --python tools/blender-scripts/fit_female_long_hair.py -- --style twintail
cd tools && ../.venv/bin/python rig-female-long-hair.py --style twintail
node tools/prepare-female-knight.mjs --part twintail_hair
```

- 분리: 검은 머리끈은 빨강−파랑 차이가 0에 가까워 기준을 −20으로 둔다. 두상 평균 오차 3.4mm, 95% 7.6mm.
- 피팅: `level_fringe`만 쓰고 이마선 내림은 없다. 처음에는 긴 앞머리 헤어처럼 `square_fringe`도 켰는데, 앞머리 끝과 옆머리를 잇는 삼각형 32개가 지워져 관자놀이에 살이 보였다(사용자 지적). 같은 날 끄고 다시 만들었다.
- 리깅: 긴 헤어와 같다. 목 위는 머리에 고정하고 그 아래 꼬리는 몸통 가중치를 받는다.
- 얼굴별 보정(`femaleHairFits.json`)은 넣지 않았다. 기본(순찰자) 얼굴에서만 봤다.

## 게임 연결

- `shared/src/character.rs`: `HairStyle::Twintail`, 여성 헤어 판정, 텍스처 평균색. 프로토콜 **164**.
- 클라이언트: `FEMALE_HAIR_PARTS`, `femaleHairFit.ts`, `appearanceColors.ts`, 생성 화면 선택지와 5개 언어 라벨, 미리보기 두 곳.
- 썸네일 `female-hair-twintail.png`: `render_female_hair_thumbnail.py --body .../faces/tripo_ranger_v1/base_ranger.glb`로 512² 렌더 후 128².
- `data-src/npcs.csv`의 Miriel이 `twintail`을 텍스처 원색으로 쓴다.

## 남은 점

- 투구 헤어 마스크와 chic·custom 얼굴 보정은 하지 않았다.
- 동작 검토(`review_female_long_hair.py`)는 돌리지 않았다. 미리보기의 대기·달리기에서만 봤다.
