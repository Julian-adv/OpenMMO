# 수계 시스템 (Water System)

바다 수면 렌더링 시스템의 기술적 사양. 지형 시스템과 연동하여 해수면 이하 타일에 물을 자동 배치한다.

## 1. 개요

*   해수면(Y=0) 이하 높이를 가진 타일에 물 메쉬를 자동 생성한다.
*   물 메쉬는 지형과 동일한 지오메트리(64×64 세그먼트)를 공유하며, Y=0.01 위치에 배치한다.
*   매 프레임 3-패스 렌더링: 굴절(refraction) → 반사(reflection) → 메인 렌더.

## 2. 멀티패스 렌더링

### 2.1 굴절 패스 (Refraction Pass)

*   물 메쉬를 숨기고 씬을 절반 해상도로 렌더링하여 굴절 텍스처를 생성한다.
*   물 셰이더에서 표면 노멀로 UV를 왜곡하여 수중 지형이 굴절되어 보이는 효과를 낸다.
*   파일: `client/src/lib/managers/refractionRenderManager.ts`

### 2.2 반사 패스 (Reflection Pass)

*   카메라를 해수면(Y=0) 기준으로 Y축 반전하여 엔티티만 절반 해상도로 렌더링한다.
*   지형과 물은 숨기고 엔티티(캐릭터, 몬스터 등)만 반사에 포함된다.
*   Eraser Mesh: Y=0에 투명 평면을 배치하여, 반전 카메라에서 수면 아래로 보이는 엔티티 부분을 지운다 (depthFunc=GREATER + zero blending).
*   파일: `client/src/lib/managers/reflectionRenderManager.ts`

## 3. 물 셰이더 (Water Material)

WebGPU TSL(Three.js Shading Language) 노드 기반 커스텀 셰이더.

파일: `client/src/lib/shaders/water-material.ts`

### 3.1 버텍스 단계: Gerstner Wave

*   3개의 Gerstner Wave를 합산하여 수면의 물리적 파동을 시뮬레이션한다.
*   Gerstner Wave는 사인파와 달리 파봉이 뾰족하고 파곡이 넓은 자연스러운 해양 파동을 생성한다.
*   파라미터 (방향, Steepness, Wavelength):
    *   Wave A: 0도, 0.03, 20m
    *   Wave B: 30도, 0.02, 15m
    *   Wave C: 60도, 0.015, 10m

### 3.2 프래그먼트 단계

셰이더의 프래그먼트 처리 흐름:

1.  **수심 계산:** 하이트맵 텍스처에서 지형 높이를 읽어 수면과의 깊이(depth)를 계산한다.
2.  **깊이 기반 색상:** 4단계 그라디언트로 수심에 따른 색상을 결정한다.
    *   해안 거품(foam band) → 얕은 바다(shallow) → 중간(mid) → 심해(deep).
3.  **표면 노멀:** 노멀맵을 4방향으로 시간 오프셋 샘플링하여 세밀한 수면 요철을 표현한다.
4.  **굴절:** 화면 UV를 표면 노멀로 왜곡하여 굴절 텍스처를 샘플링한다. 얕은 곳에서 강하게, 깊은 곳에서 약하게 적용.
5.  **수중 코스틱스(Caustics):** 보로노이 기반 코스틱스 텍스처를 2개 레이어로 샘플링하여 min 합성. 쉬머(shimmer) 효과로 반짝임을 추가한다.
6.  **반사 (Specular + Fresnel):**
    *   Blinn-Phong 스페큘러로 태양 반사를 표현한다.
    *   노멀맵 기반 스파클(sparkle)로 파봉에서 반짝이는 효과를 낸다.
    *   프레넬 반사: 시선 각도에 따라 하늘/엔티티 반사를 혼합한다.
    *   절차적 하늘 반사 (지면색 → 헤이즈 → 천정색 그라디언트) + 엔티티 평면 반사를 합성한다.
7.  **해안 거품(Shore Foam):**
    *   2개의 반주기(half-cycle) 파동 밴드가 심해에서 해안으로 이동하며 교차 페이드한다.
    *   노이즈로 불규칙한 거품 가장자리를 생성한다.
    *   해안 근처에 상시 거품(persistent shore foam)을 유지한다.
8.  **해안 가장자리 처리:** 수심이 매우 얕은 곳(< 0.6m)에서 노이즈 기반으로 물에 구멍을 뚫어 지형이 자연스럽게 드러나게 한다.
9.  **알파:** 수심에 따라 0.15(극얕은)~0.98(깊은)으로 투명도를 조절한다.

## 4. 절차적 텍스처 생성

렌더링에 필요한 텍스처 중 일부는 런타임에 절차적으로 생성한다.

| 텍스처 | 해상도 | 생성 방식 | 파일 |
|--------|--------|-----------|------|
| 노멀맵 | 256×256 | 4-옥타브 value noise → 중앙 차분 노멀 | `water-normal-gen.ts` |
| 코스틱스 | 256×256 | 128셀 보로노이 거리 필드 | `caustics-gen.ts` |

외부 텍스처(파일에서 로드):

| 텍스처 | 용도 | 파일 |
|--------|------|------|
| 거품(foam) | 해안 거품 패턴 | `client/public/textures/13843.jpg` |
| 수면(surface) | 수면 디테일 | `client/public/textures/4141.jpg` |

파일: `client/src/lib/shaders/water-foam-gen.ts`

## 5. 타일 관리

*   물은 지형 타일 단위로 관리된다. `TerrainHeightManager.hasWater()`로 타일에 해수면 이하 버텍스가 있는지 판별한다.
*   타일별로 하이트맵 DataTexture를 생성하여 물 셰이더에 전달한다 (수심 계산용).
*   높이 브러시로 지형을 편집하면 물 타일이 실시간으로 추가/제거/갱신된다.
*   파일: `client/src/lib/components/game-scene/GameSceneWaterLayer.svelte`

## 6. 핵심 파일

| 파일 | 역할 |
|------|------|
| `client/src/lib/shaders/water-material.ts` | 물 셰이더 (Gerstner + 프래그먼트 전체) |
| `client/src/lib/shaders/water-normal-gen.ts` | 절차적 노멀맵 생성 |
| `client/src/lib/shaders/water-foam-gen.ts` | 거품/수면 텍스처 로더 |
| `client/src/lib/shaders/caustics-gen.ts` | 절차적 코스틱스 텍스처 생성 |
| `client/src/lib/components/WaterTile.svelte` | 물 타일 컴포넌트 (머티리얼 생성·유니폼 갱신) |
| `client/src/lib/components/game-scene/GameSceneWaterLayer.svelte` | 물 타일 관리 레이어 |
| `client/src/lib/managers/refractionRenderManager.ts` | 굴절 렌더 패스 |
| `client/src/lib/managers/reflectionRenderManager.ts` | 반사 렌더 패스 |

## 7. 향후 확장 *(TODO)*

*   강, 호수 등 내륙 수계 표현.
*   수면 높이를 타일/리전별로 다르게 설정 (현재는 전역 Y=0 고정).
*   파도 높이·방향의 날씨 연동.
