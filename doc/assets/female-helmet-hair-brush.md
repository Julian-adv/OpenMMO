# 여성 투구 헤어 브러시

투구를 써도 고른 헤어(단발 `hair`, 긴 생머리 `valkyrie_hair`, 긴 생머리 · 앞머리 `long_bangs_hair`, 포니테일 `ranger_hair`)가 그대로 보인다. 단발만 뿔 투구에서 투구용 단발, 기사·광휘 투구에서 숨김이다(2026-11-06 사용자 결정으로 포니테일을 단발로 바꾸던 규칙을 없앰).
투구 밖으로 삐져나온 헤어는 사용자가 여성 미리보기에서 클릭으로 지운다(사용자 결정: 자동 계산보다 손으로 고르는 편이 낫다).

## 쓰는 법

1. `/modular-character-preview.html?character=female`에서 투구와 헤어를 고른다. 투구 아래 `투구 헤어 브러시`를 켠다(동작이 멈춘다).
2. 보이는 헤어를 짧게 클릭하면 반경 안 삼각형을 지우거나(`지우기`) 되살린다(`되살리기`). 드래그는 회전이다.
3. `이 투구의 헤어 저장`은 [female-helmet-hair-brush.json](female-helmet-hair-brush.json)에 투구 런타임 파츠(`bard_helmet`, `brilliance_helmet` 등)·헤어 파츠별 삼각형 번호를 쓴다.
4. 런타임 투구를 다시 만든다. 블렌더 피팅은 다시 할 필요가 없다.

```sh
node tools/prepare-female-knight.mjs --part brilliance_helmet
```

## 구조

- `prepare-female-knight.mjs`가 투구 GLB의 scene extras `hair_masks`에 헤어별 `primitive_triangles`·`hide_triangle_indices`를 싣는다. 헤어 원본의 삼각형 수를 같이 넣어 런타임 헤어와 어긋나면 등록 때 오류가 난다.
- 게임(`femaleCharacter.ts`)은 그 투구를 쓰고 그 헤어가 보일 때만 해당 삼각형을 뺀 인덱스로 헤어 지오메트리를 바꾼다. 장비가 바뀔 때 한 번 만들고 매 프레임 계산하지 않는다.
- 삼각형 번호는 휴지 자세 기준이다. 런타임 헤어 GLB는 meshopt 압축으로 삼각형 안 꼭짓점 순서만 돌아가고 삼각형 순서·위치는 원본과 같다(단발·긴 생머리·포니테일 확인).
- 미리보기 서버 경로: `/__female-preview/helmet-hair.json`(`tools/lib/helmet-hair-brush.mjs`).

## 기록

| 투구 | 헤어 | 가린 삼각형 | 날짜 |
| --- | --- | ---: | --- |
| `bard_helmet` | `hair` (단발) | 38 | 2026-11-06 |
| `brilliance_helmet` | `ranger_hair` (포니테일) | 713 | 2026-11-06 |
| `helmet` (은빛 기사 투구) | `ranger_hair` (포니테일) | 152 | 2026-11-06 |
| `barbarian_helmet` (뿔 투구) | `ranger_hair` (포니테일) | 555 | 2026-11-06 |

2026-11-11 긴 생머리를 [새 메시](modular-female-long-straight-hair.md)로 바꾸면서 `valkyrie_hair` 기록 두 건(은빛 기사 투구 222, 뿔 투구 289)을 지웠다. 삼각형 번호가 새 메시와 맞지 않는다. 새 긴 생머리 두 종은 아직 브러시 기록이 없다.
