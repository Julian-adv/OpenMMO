# 조립식 복장 KTX2 전환 — 2026-10-10

사제에 적용했던 GPU 텍스처 압축을 기본·기사·야만전사·도적·원시인·순찰자 복장에도 적용했다.
추가 전환 대상은 25개 파츠, 텍스처 60장이다. 사제 4개를 포함한 복장 29개가 모두
`KHR_texture_basisu`와 `EXT_meshopt_compression`을 사용한다.

기존 피팅 원본에서 게임용 GLB를 다시 만들었다. 출처·라이선스·생성 구독 조건은
[캐릭터 기록](characters.md), [기본 몸체·복장 기록](modular-human-male-01.md)과
각 복장의 기존 제작 기록을 따른다. 새 이미지·모델 생성이나 유료 API 사용은 없다.
원본 경로와 SHA-256은 게임용 [manifest](../../client/public/models/characters/modular_male/manifest.json)에 보존한다.
얼굴·몸체·머리카락, 바닥 아이템 모델과 아이콘은 이번 복장 전환 대상에 포함하지 않는다.

## 인코딩과 재현

- KTX2 UASTC quality 3, RDO 0.5, Zstd 18, 전체 mipmap.
- 기존 크기 정책 유지: 최대 512px, 종횡비 유지, 확대 없음. 도적 손목 천은 512×256이다.
- Base color/emissive는 sRGB, normal/metallic/roughness 등 데이터 맵은 linear.
- 기존 Meshopt 압축 및 메시·UV·스킨 가중치·본·재질 설정 유지.

`tools/prepare-modular-character.mjs`는 모든 복장 파츠에 `--ktx2`를 자동 적용한다.
전체 복장만 다시 만들려면 다음 명령을 사용한다. 개별 `--part`와 기존 세트별 옵션도
동일한 KTX2 경로를 사용한다.

```bash
node tools/prepare-modular-character.mjs --outfits-only
```

KTX-Software 4.4.2의 `toktx`가 필요하다. PATH 또는 `TOKTX` 환경변수로 지정한다.
기존 `@gltf-transform` 4.3.0, `meshoptimizer`, `sharp` 기반 도구를 사용한다.

## 크기와 검증

| 복장 | 파츠 | 텍스처 | 이전 GLB bytes | KTX2 GLB bytes |
| --- | ---: | ---: | ---: | ---: |
| 기본 | 3 | 9 | 1,023,712 | 2,194,880 |
| 기사 | 5 | 15 | 2,475,700 | 4,979,116 |
| 야만전사 | 5 | 20 | 3,413,856 | 6,936,656 |
| 도적 | 4 | 7 | 1,474,864 | 2,650,892 |
| 원시인 | 4 | 5 | 1,231,880 | 2,229,080 |
| 순찰자 | 4 | 4 | 1,296,792 | 2,061,748 |
| 합계 | 25 | 60 | 10,916,804 | 21,052,372 |

8bpp GPU 압축 형식에서는 같은 크기의 RGBA8 대비 mipmap 포함 텍스처 메모리가 약 75% 감소한다.
GLB 다운로드 크기는 약 10.4 MiB에서 20.1 MiB로 증가한다. GPU 압축 형식 미지원 시
RGBA 대체 경로를 사용하므로 메모리 절감량은 장치에 따라 다르다.

- 29개 GLB의 변환 전후 메시·노드·스킨·재질 및 디코딩한 모든 accessor 배열이 정확히 일치한다.
- 전체 KTX2의 색 공간, mipmap, Zstd, manifest SHA-256을 확인했다. 사제 4개 파일은 해시도 동일하다.
- 복장 조립·착탈 관련 테스트 118개, `npm run check`, `npm run lint` 통과.
- Chromium SwiftShader의 WebGPUBackend와 WebGLBackend에서 7개 복장, 29개 파츠를 로드했다.
  모든 복장 텍스처의 GPU 압축을 확인하고 각 장비 슬롯 해제·재장착과 옷자락 물리 재초기화를
  렌더했다. 페이지·에셋 요청·GPU 검증 오류는 0건이다. 실제 하드웨어 GPU 성능은 측정하지 않았다.
- 6개 신규 전환 복장의 렌더에서 색상·금속·가죽·천 표현과 텍스처 누락 여부를 확인했다.

파츠별 크기·해상도·삼각형 수·해시와 브라우저 측정값은
[검증 기록](modular-outfits-runtime-ktx2-v1.json)에 보관한다.
