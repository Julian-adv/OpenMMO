# 순찰자 기본 복장 등록 — 2026-10-07

신규 남성 순찰자는 상의·바지·장갑·부츠를 각 1개 자동 장착하고 시작한다.
캐릭터 생성 화면에도 같은 복장을 표시한다. 기존 캐릭터에 소급 지급하지 않는다.
여성 모델은 별도 피팅이 없어 기존 시작 장비를 유지한다.

| 아이템 ID | 슬롯 |
| --- | --- |
| `worn_ranger_top` | chest |
| `worn_ranger_pants` | pants |
| `worn_ranger_gloves` | hands |
| `worn_ranger_boots` | boots |

기존 클래스 시작 복장과 같이 부위당 방어 1, 가격·상자 드랍 티어 없음,
`untradeable=true`로 등록한다. 상점 판매와 플레이어 거래를 차단한다.
공통 시작 장비인 낡은 철검과 횃불도 함께 지급한다.

승인된 Tripo [상의 v4](modular-ranger-tripo-top.md), [바지](modular-ranger-tripo-pants.md),
[장갑](modular-ranger-tripo-gloves.md), [부츠](modular-ranger-tripo-boots.md)를 재사용했다.
출처·라이선스·구독 등급·전달일은 각 원본 기록을 따른다. 새 AI 생성이나 유료 호출은 없다.
게임용 파츠는 기존 압축 도구로 메시 압축·512px 텍스처 변환을 적용했다.
부츠 입구의 바짓단 보정, 장갑의 소매 보정, 착탈 시 피부 복원은 제작실과 공통 로직을 사용한다.

```bash
node tools/prepare-modular-character.mjs --ranger-only
blender -b -t 6 --python-exit-code 1 --python tools/blender-scripts/export_ranger_items.py
```

바닥 모델과 투명 128px 인벤토리 아이콘은 피팅 모델을 로컬 Blender에서 변환·렌더했다.
상의·바지는 바닥에 눕히고 장갑은 오른손 한쪽으로 표시한다.
게임 출력은 `client/public/models/armor/ranger_{top,pants,gloves,boots}.glb`,
`client/public/items/armor/ranger_{top,pants,gloves,boots}.png`이며,
편집본은 `assets/ranger_{top,pants,gloves,boots}/`에 보관한다.
게임용 GLB와 편집본은 기존 바이너리 보관 정책을 따른다.

생성·목록 장비·판매·거래 서버 테스트 8개와 관련 프런트엔드 테스트 65개가 통과했다.
`cargo fmt`, `cargo check`, `npm run check`, `npm run lint`도 통과했다.
브라우저에서는 압축된 게임용 모델의 대기·달리기·점프·공격·앉기 표본과 부위 해제를 확인했다.
제작실의 WebGL 렌더러에서 호환되지 않는 외모용 NodeMaterial은 검수 시 원래 텍스처로 복원했다.
이 검수는 실제 게임의 WebGPU 렌더러 전체나 모든 혼합 장비의 관통 검사를 대신하지 않는다.

몸체·crop 헤어·네 부위의 삼각형 합계, 런타임 표시 수, 얼굴 배분과 파일 해시는
[등록·검수 기록](modular-ranger-runtime.json)에 있다. 무기·망토는 예산에서 제외했다.
기존 15,000–20,000 목표를 초과하며 이번 등록에서 형상을 축소하지 않았다.

등록 후 보정한 목 경계와 최종 동작 화면은 [목 검수 기록](modular-ranger-neck-review.json)을 참고한다.
