# Splatmap V2: 인덱스 기반 2-텍스처 블렌드 인코딩

## 1. 배경 및 동기

현재 splatmap은 **4채널 가중치(RGBA)** 방식이다:

- 셀당 4바이트, 각 바이트는 해당 슬롯 텍스처의 weight (0-255)
- 지역(region)당 최대 4개 텍스처만 사용 가능
- 픽셀당 최대 4개 텍스처를 가중합

**문제**: 해안 + 고산이 한 리전에 나타나는 경우 필요한 텍스처가 쉽게 4개를 초과한다 (풀, 모래, 바위, 라테라이트, 눈, 포장도로 등).

**해결**: 리전당 팔레트를 **16개**로 확장하고, 셀당 **2개 텍스처 블렌드**로 저장한다. 동일 4바이트 footprint를 유지한다.

## 2. 새 인코딩 (V2)

셀당 4바이트:

| 바이트 | 필드             | 범위     | 의미                                                  |
|--------|------------------|----------|------------------------------------------------------|
| 0      | `indices`        | 0–255    | 상위 4비트 = `primaryIdx`, 하위 4비트 = `secondaryIdx` (각 0–15) |
| 1      | `reserved`       | 0–255    | 예약 — 향후 edge jitter seed / material variant 등    |
| 2      | `blend`          | 0–255    | `0`=100% primary, `255`=100% secondary                |
| 3      | `vegMeta`        | 0–255    | 식생/기타 메타 — 섹션 8 참조                          |

`primaryIdx == secondaryIdx`면 단일 텍스처 셀 (blend 값 무시). 파일 크기는 변동 없음 (64×64×4 = 16,384 B).

### 인덱스 패킹/언팩
```
indices = (primaryIdx << 4) | (secondaryIdx & 0x0F)
primaryIdx   = (indices >> 4) & 0x0F
secondaryIdx = indices & 0x0F
```
셰이더에서는 `float(indices * 255.0)`를 정수로 변환 후 bit op 2회. 현대 GPU 비용 무시.

## 3. 지역 메타 (RegionMeta)

`layers`를 최대 16개까지 허용:

```json
{
  "layers": [
    { "texture": "rocky_terrain_02_1k", "tileScale": 8.0 },
    { "texture": "sandy_gravel_02_1k",  "tileScale": 8.0 },
    { "texture": "red_laterite_soil_stones_1k", "tileScale": 10.0 },
    { "texture": "snow_02_1k",          "tileScale": 4.0 },
    { "texture": "patterned_paving_02_1k", "tileScale": 30.0 },
    { "texture": "gravel_road_1k",      "tileScale": 8.0 }
    /* ... up to 16 ... */
  ]
}
```

팔레트 길이는 1–16 가변. 서버 검증 (`terrain/src/io.rs:399`) 을 `<= 16`으로 완화.

## 4. Atlas 레이아웃 (4×4, 512 슬롯)

- **슬롯 해상도: `ATLAS_SLOT_SIZE = 512`** (기존 `.glb`의 1K 소스를 atlas 빌드 시 512로 다운샘플)
  - 근거: 최대 줌인 시 1m 셀이 화면에서 ~256px. 가장 작은 tileScale=4 (눈)도 1m = 512/4 = 128 소스 px로 Nyquist 근방. tileScale ≥ 8인 대부분 레이어는 충분.
  - 다운샘플은 `canvas.drawImage(img, 0,0, srcW,srcH, slotX+B, slotY+B, 512, 512)` 한 줄. `.glb` 자체는 그대로 (다운로드 대역폭 변동 없음).
- **슬롯 크기: `512 + 2*ATLAS_BORDER = 528` px**
- **Atlas 크기: `4 × 528 = 2112` px per axis**
- **메모리 (3종 합산)**: 2112² × 4 bytes × 3 atlases ≈ **54 MB** (기존 1K/2×2 = 48MB, 1K/4×4 = 192MB 대비 훨씬 양호)
- 슬롯 좌표: `slotX = (idx % 4) * slotSize`, `slotY = floor(idx / 4) * slotSize`
- 빈 슬롯(팔레트 길이 < 16)은 fallback 색으로 채움
- 기존 `drawWithWrapBorder` 로직은 그대로 재사용 (slotSize만 교체)

## 5. 셰이더 변경

### 샘플링
```glsl
vec4 splat = texture2D(splatMap, vUv);  // bytes as floats 0-1
int  packed = int(splat.r * 255.0 + 0.5);
int  pIdx = (packed >> 4) & 0xF;
int  sIdx = packed & 0xF;
float blend = splat.b;                  // already 0-1
// splat.g reserved, splat.a = vegMeta (not used in material; read by grass system)
```

### 각 슬롯별 UV + tileScale
`uTile0..uTile15` 대신 **uniform 배열** `uTile[16]` 사용. 픽셀당 primary/secondary 두 슬롯의 tileScale을 인덱싱.

```glsl
float tileP = uTile[pIdx];
float tileS = uTile[sIdx];
vec2 uvP = atlasUV(vUv, pIdx, tileP);
vec2 uvS = atlasUV(vUv, sIdx, tileS);
vec3 cP = textureGrad(diffuseAtlas, uvP, dP).rgb;
vec3 cS = textureGrad(diffuseAtlas, uvS, dS).rgb;
vec3 color = mix(cP, cS, blend);
```

Normal / ORM 동일 방식.

### 장점
- 픽셀당 atlas 샘플 **2회** (현재 4회) → 대역폭 절약
- 가중치 정규화 불필요 (`mix` 한 번)

### 단점
- 인덱스가 integer라 bilinear 보간 불가 → splat 텍스처는 **NEAREST 필터**로 샘플링. 경계 smoothing은 섹션 6의 weight-space bilinear로 처리.

## 6. 셀 보간 전략 (Weight-space Bilinear Blend)

splat 텍스처를 NEAREST로 샘플링하되, `blend` 값은 4 인접 셀의 가중치를 weight space에서 bilinear로 섞어 셀 격자가 보이지 않도록 한다. 인덱스는 그대로 현재 셀 기준, blend만 스무스하게.

### 핵심 아이디어
각 셀은 자신의 (primary, secondary) 쌍에 대해 가중치 `(1-blend, blend)`를 갖는다. 현재 픽셀이 쓸 텍스처 `P`, `S`는 nearest 셀에서 확정되므로, 각 이웃 셀이 텍스처 `P`·`S`에 얼마나 기여하는지만 구해서 bilinear로 섞으면 된다.

```glsl
// 1. 현재 셀 (nearest) — 인덱스 P, S 확정
vec4 cur = texture(splat, vUv);
int pIdx = decodePrimary(cur);
int sIdx = decodeSecondary(cur);

// 2. 4 이웃 셀 샘플 (sub-cell 위치로 오프셋)
vec2 cellPos = vUv * SPLAT_SIZE - 0.5;
vec2 baseUv  = (floor(cellPos) + 0.5) / SPLAT_SIZE;
vec2 frac    = fract(cellPos);
vec4 s00 = texture(splat, baseUv);
vec4 s10 = texture(splat, baseUv + vec2(1.0/SPLAT_SIZE, 0));
vec4 s01 = texture(splat, baseUv + vec2(0, 1.0/SPLAT_SIZE));
vec4 s11 = texture(splat, baseUv + vec2(1.0/SPLAT_SIZE, 1.0/SPLAT_SIZE));

// 3. 각 셀의 texture-i 가중치: primary==i → (1-blend), secondary==i → blend, else 0
float idxWeight(vec4 s, int i) {
  return (s.primary == i ? 1.0 - s.blend : 0.0)
       + (s.secondary == i ? s.blend : 0.0);
}

// 4. Bilinear 가중치
float w00 = (1-frac.x)*(1-frac.y), w10 = frac.x*(1-frac.y);
float w01 = (1-frac.x)*frac.y,     w11 = frac.x*frac.y;

// 5. 텍스처 P, S의 픽셀 가중치
float pW = idxWeight(s00,pIdx)*w00 + idxWeight(s10,pIdx)*w10
        + idxWeight(s01,pIdx)*w01 + idxWeight(s11,pIdx)*w11;
float sW = /* same for sIdx */ ;

// 6. 최종 blend
float blend = (pW + sW > 0.01) ? sW / (pW + sW) : cur.blend;
```

### 커버하는 케이스
- **같은 쌍 (grass,sand) ↔ (grass,sand)**: `pW`, `sW` 모두 부드럽게 변화 → 부드러운 blend.
- **primary 공유 (grass,sand) ↔ (grass,laterite)**: 공유된 grass의 `pW`는 부드럽고, 현재 셀의 secondary(sand)의 `sW`는 반대편 셀에서 0으로 fade → 격자 없음.
- **완전 다른 쌍 (grass,sand) ↔ (snow,rock)**: `pW + sW ≈ 0` → nearest 셀 blend로 fallback. 경계가 선명하게 보이지만 이 케이스는 절차 생성에선 거의 발생하지 않음.

### 비용
- splat 샘플 1회 → **4회** (캐시 친화적, 64×64 텍스처라 미미)
- atlas 샘플은 그대로 2회
- 부가 연산 (곱셈·덧셈 몇 개) 무시 가능

## 7. 페인트 로직

브러시로 텍스처 `X`를 세기 `s` (0-1)로 칠할 때 각 셀 처리:

```
if primaryIdx == X:
  blend = round(blend * (1 - s))       # primary 비중 증가
elif secondaryIdx == X:
  blend = round(blend + s * (255 - blend))  # secondary 비중 증가
else:
  # 슬롯 교체: blend 기준 약한 쪽을 X로 교체
  if blend < 128:  # primary가 강한 픽셀 → secondary 교체
    secondaryIdx = X
    blend = round(s * 255)
  else:            # secondary가 강한 픽셀 → primary 교체
    primaryIdx = X
    blend = round(255 - s * 255)
```

엣지 케이스:
- `primaryIdx == secondaryIdx`: 단일 텍스처 픽셀. 첫 브러시로 교체 시 secondary에 X를 넣고 blend 올림.
- 강한 브러시 (s=1.0): 한 번에 픽셀을 완전 교체.

## 8. 식생 메타 (`vegMeta`, byte 3)

기존 R 채널의 범위를 **그대로 byte 3으로 이식**한다. 코드 변경을 최소화하고, 0-229 구간을 향후 다른 용도로 확장 가능하게 남긴다.

| 범위      | 의미                                         |
|-----------|----------------------------------------------|
| 0..229    | 예약 — 향후 확장 (moss, snow cover level 등) |
| 230..239  | short grass, 밀도 = `value - 230` (0–9)       |
| 240..249  | tall grass,  밀도 = `value - 240` (0–9)       |
| 250..255  | 예약 — 향후 확장 (특수 식생 등)              |

- `SHORT_GRASS_R_MIN=230`, `SHORT_GRASS_R_MAX=239`, `TALL_GRASS_R_MIN=240`, `TALL_GRASS_R_MAX=249` 상수를 `grass-material.ts`에서 **의미만 재정의** (이제 R채널이 아니라 vegMeta 바이트 기준).
- 꽃 스캐터링은 현재와 동일하게 short grass 밀도 기반 (`grass-data.ts`).
- **장점**: 주 텍스처가 풀이 아니어도 풀을 얹을 수 있음 (바위 위 이끼, 모래 위 잔풀 등).
- 코드 변경: `R 채널 읽기` → `vegMeta 바이트(byte 3) 읽기`. 조건/범위 상수는 그대로.

## 9. 마이그레이션

**정책**: "backwards compat 신경 쓰지 않음" (CLAUDE.md). 기존 데이터는 일괄 변환 또는 재생성.

**선택지 A (변환 스크립트)**: `tools/migrate_splat_v1_to_v2.mjs`
- 모든 `data/terrain/splat/**/*.bin` 순회
- 각 셀에서 dominant/second-dominant 채널 인덱스 추출
- blend = second/(first+second) * 255
- grassMeta = R 채널의 grass 밀도 영역 (기존 SHORT/TALL 비트)
- region meta도 동시 변환 (기존 4 layers → 새 스키마 유지)

**선택지 B (재생성)**: 지역 에디터에서 모든 region을 재생성. 단순하나 수동 페인트 작업 손실.

초기 개발 단계이므로 **B 권장**. 필요 시 A 제공.

## 10. 미니맵 생성기

`regionMinimapGenerator.ts`:
- 각 셀에서 `primaryIdx`와 `secondaryIdx`의 색을 읽어 `blend`로 lerp
- `TEXTURE_COLORS` 테이블 확장 (patterned_paving, cobblestone 등 추가 텍스처)

## 11. 구현 순서

1. **타입/상수 정의**
   - `client/src/lib/terrain/splat-encoding.ts` 신설: `MAX_PALETTE = 16`, 패킹/언패킹 헬퍼 (`packCell`, `unpackCell`)
   - `DEFAULT_LAYER_CONFIGS`를 최대 16개 슬롯 구조로 전환 (`LayerConfig[]`, 길이 가변)

2. **서버 검증 완화**
   - `terrain/src/io.rs`: `layers.len() > 16` 시 에러, 하한은 1
   - `terrain/src/tests.rs`: 테스트 수정

3. **Atlas 4×4**
   - `splatLayerLoader.ts`: `buildAtlasTexture` 2→4 확장, `loadSplatLayers` 배열 길이 가변
   - `SplatAtlasSet` 인터페이스 그대로

4. **셰이더 재작성**
   - `makeSplatStandardMaterial.ts`:
     - `splatMap` 필터 NEAREST로 변경
     - `uTile` 배열화 (16 float)
     - UV 계산 4×4 기반
     - primary/secondary 인덱스 언팩 → 2회 샘플 → mix
     - `getWeights` 함수 제거

5. **지형 생성기 재작성**
   - `terrain-splat-gen.ts`:
     - 출력을 V2 포맷으로. 생성 규칙은 기존 유지 (h<0 → 모래, 해안 → 모래, 경사지 → 바위, 고지대 → 눈)
     - 단, 각 셀이 primary/secondary 2개만 사용 (기본 primary = 풀, secondary = 해당 biome 텍스처, blend = biome weight)
   - `GenerateTerrainDialog.svelte`: 팔레트를 확장 (해안+산악 지역은 풀/모래/바위/눈/라테라이트/포장 등)

6. **브러시 재작성**
   - `terrainSplatManager.ts`: `applySplatBrush` / `applySplatLine`를 § 7 로직으로 전면 재작성
   - 브러시 UI에서 "선택한 텍스처 = 팔레트 인덱스" (현재 4채널 선택 UI 확장)

7. **풀 생성 경로 수정**
   - grass-material 상수 재정의
   - 잔디 인스턴스 생성 코드가 byte 3을 읽도록

8. **미니맵 재작성**
   - `regionMinimapGenerator.ts`: § 10 로직

9. **페인트 UI 확장** (`SplatBrushPanel.svelte`)
   - 4개 슬롯 → 16 슬롯 그리드, 팔레트 편집(추가/교체) 지원

10. **마이그레이션 / 재생성** — 선택지 B 기본, A는 필요 시.

11. **문서 업데이트**
    - `doc/MAP_DESIGN.md`의 "4-Layer PBR Splatting" 섹션을 V2로 수정

## 12. 리스크 및 대안

- **3개 이상 텍스처가 한 점에서 만나는 경우 seam**: 셀 경계에서 약한 텍스처가 드롭됨. 대부분 상황에서 허용 가능 (절차 생성은 grass + biome 이진 블렌드).
- **NEAREST splat 샘플링으로 경계가 날카롭게 보일 수 있음**: 셀 크기 1m이므로 시각적 영향 제한. 필요 시 픽셀 셰이더에서 `fwidth` 기반 dither 추가.
- **Atlas 메모리**: 4K×4K × 3 (diffuse/normal/ORM) × 4 B = **192 MB**. 모바일에선 부담이지만 데스크톱 WebGPU에선 OK. 장기적으로 bindless/virtual texturing 검토.
