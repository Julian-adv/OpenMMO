# Item Tiers & Dungeon Drop Design

던전 티어별 장비 파밍 설계 — 방어구 세트·무기·장신구의 티어 배치와 드랍 규칙. 결정 이력은 git 로그와 [devlog](devlog/) 참조.

핵심 컨셉: **세트는 인접한 두 티어에 걸쳐 드랍된다.** 티어 N 던전에서 일부 파츠를, 티어 N+1 던전에서 나머지(몸통 등 핵심 파츠)를 모아 완성한다. 완성한 세트가 그다음 티어에 도전할 체급이 된다. 무기도 방어구와 대칭으로 티어당 한 자루씩 오른다.

## 티어 로드맵

| 티어 | 던전 | 드랍 | 상태 |
|------|------|------|------|
| 1 | Old Crypt | 가죽 세트 일부 (투구·바지·벨트) + goblin_sword | 운영 중 |
| 2 | Orc Warrens | 가죽 세트 완성 (몸통·장갑·부츠) + 체인 세트 일부 (철 부츠·철 투구) + iron_sword | 운영 중 |
| 3 | Ogre Stronghold (15층, ogre_boss) | 체인 세트 완성 (체인 메일·건틀릿) + 판금 세트 일부 (부츠·그리브) + 기본 망토 + steel_longsword | 운영 중 |
| 4 | Skeleton Crypt (20층, skeleton_knight) | 판금 세트 완성 (흉갑·투구·건틀릿) + great_sword | 구현 완료·운영 배포 대기 |
| 4 | Thieves' Den (20층, thief_king) | 판금 세트 완성 (흉갑 확정·투구·건틀릿 롤) + cutthroat_dirk | 구현 완료, 단검·보스 모델 완료 |
| 5 | Sundered Bastion (25층, barkan) · [필드 레이드 연계 설계](TIER5_FIELD_RAID.md) | ring_of_protection + rune_blade + 대지의 아뮬렛 (가칭) | 던전 본체·rune_blade·아뮬렛 착용 효과 구현, 봉헌 미구현 |
| 5 | Shadow Guild (25층, shadow_master) | ring_of_protection + shadowfang | 구현 완료, 단검·보스 모델 완료 |

특수 망토(투명·보호 등)·셔츠·amulet_of_life_saving·ring_of_regeneration은 던전 풀에 넣지 않는다 — **월드 드랍 전용 희소템**으로 유저 간 거래의 축을 만든다 (아래 참조).

## 세트 구성

### 티어 1–2: 가죽 세트

| 슬롯 | 아이템 | guard | 티어 |
|------|--------|-------|------|
| head | leather_helmet | 1 | 1 |
| pants | leather_pants | 1 | 1 |
| belt | leather_belt | 0 | 1 |
| chest | leather_armor | 2 | 2 |
| hands | leather_gloves | 1 | 2 |
| boots | leather_boots | 1 | 2 |

세트 guard 합계 = 6.

### 티어 2–3: 체인 세트

| 슬롯 | 아이템 | guard | 티어 |
|------|--------|-------|------|
| boots | iron_boots | 2 | 2 |
| head | iron_helmet | 2 | 2 |
| chest | chain_mail | 5 | 3 |
| hands | iron_gauntlets | 2 | 3 |

세트 guard 합계 = 11.

### 티어 3–4: 판금 세트

| 슬롯 | 아이템 | guard | 티어 |
|------|--------|-------|------|
| pants | plate_greaves | 3 | 3 |
| boots | plate_boots | 3 | 3 |
| chest | breastplate | 7 | 4 |
| head | plate_helmet | 3 | 4 |
| hands | plate_gauntlets | 3 | 4 |

세트 guard 합계 = 19. 각 파츠는 같은 슬롯의 이전 세트보다 guard가 높아야 한다 (파밍 보상 원칙).

### 필드 레이드 소환자 전용 투구

[헬름 오브 브릴리언스](TIER5_FIELD_RAID.md)는 대지의 아뮬렛을 봉헌해 테라클래시를 소환한 사람에게 주는 전용 보상이다. 머리에 착용하면 지능(INT) +1과 횃불을 대체하는 머리 조명이 적용된다. 일반 확률 드롭·증표 교환 장비와 별도이며 미구현이다. 거래 가능하며 토벌 성공 시 매번 지급한다. 방어력은 미정이다.

### 장신구

효과 장신구는 NetHack 계열로 간다 (효과 수치는 구현 시 확정). gold_ring은 CHA +1(ring of adornment 계열)로 전환 — 흥정 밴드 ±2%p, 반지 2슬롯 중첩 허용(캡 안). 구 체스트 규칙 유통분 77개는 소급 적용. 마찬가지로 이미 유통 중인 silver_necklace도 효과를 소급해서 받는다. silver_necklace는 티어 1–2에 장신구가 하나도 없던 공백을 메우는 약한 목 장비 — ring of sustenance 계열(허기 감소)이며, 전투력이 아닌 편의 효과라 guard·CHA·HP 재생 레인을 건드리지 않는다.

특수 효과는 items.csv `effects` 열에 토큰으로 적는다 (`;` 구분 — CSV라 쉼표 불가). 효과마다 열을 늘리지 않기 위한 것이고, 방어구 17행에 촘촘히 붙는 핵심 스탯인 `guard`는 열로 남긴다. 서버는 부팅 때 토큰을 해석하며 모르는 토큰이면 부팅에 실패한다.

| 아이템 | 효과 (`effects`) | 획득처 | 상태 |
|--------|------|--------|------|
| ring_of_protection | guard +1 (guard 열) | 티어 5 던전 공용 (20% 롤) | 있음 |
| rift_ring (균열의 반지) | 치명타 확률 +5%p (`crit+5%`), 동일 반지 효과 중복 불가 | 이졸데에게 테라클래시 파편 30개로 교환 | 있음 (2026-10-25). 기본 5%에서 착용 시 총 10% |
| earth_amulet (대지의 아뮬렛, 가칭) | 목 장비. 레벨당 최대 HP +1 (`maxhp+1/lv`), 제단 봉헌 시 소모하여 필드 보스 소환 | Sundered Bastion 전용 (10% 롤) | 있음. 봉헌은 [설계](TIER5_FIELD_RAID.md)만 |
| gold_ring | CHA +1 (`cha+1`) | 티어 3 던전 (10% 롤) | 있음 |
| silver_necklace | 허기 감소 ×0.75 (`sustenance`) | 티어 2 던전 (10% 롤) | 있음 (아이콘 필요) |
| amulet_of_life_saving | 사망 1회 방지 후 소모 | **월드 드랍 전용** | 소모품 `phoenix_talisman`(죽은 뒤 사용해 제자리 부활, Rica 5,000c + 월드 드랍 0.2%)으로 대체 구현 (2026-08-20). 착용형 자동 발동판은 보류 |
| ring_of_regeneration | HP 지속 재생 | **월드 드랍 전용** | **신규 아이콘 필요** |

amulet_of_life_saving·ring_of_regeneration은 성능이 강력해 확정 파밍에서 제외 — 특수 망토와 같은 월드 드랍 트랙(거래템)으로만 푼다.

### 망토·셔츠 (신규 슬롯)

| 슬롯 | 아이템 | 획득처 | 상태 |
|------|--------|--------|------|
| back | wool_cape (기본 망토) | 티어 3 던전 (37% 롤) | 완료 (2026-08-18). 애셋은 GLB 없이 프로시저럴 — 색은 items.csv `capeColor` |
| back | 특수 망토 (투명·보호 등) | **월드 드랍 전용** | **신규 애셋 필요** + 효과 시스템 설계 필요 |
| shirt | 셔츠 | **월드 드랍 전용** | **신규 애셋 필요** — 악세서리성 희소템 |

## 드랍 설계

### 체스트 풀 규칙

**`chestTier`가 명시된 장비만 풀에 들어간다** (opt-in). 보스 무기는 상자의 확정 보상이다. 나머지 무기·환금템·소모품은 chestTier 미지정으로 체스트에서 제외한다.

### 드랍 방식: 시그니처 + 아이템별 독립 롤

- 던전당 시그니처(확정) 드랍: **각 세트의 몸통(핵심) 파츠 + 보스 무기** — Old Crypt는 leather_helmet·goblin_sword, Orc Warrens는 leather_armor·iron_sword, 티어 3은 chain_mail·steel_longsword, Skeleton Crypt는 breastplate·great_sword, Thieves' Den은 breastplate·cutthroat_dirk. 보스 무기는 보스가 떨구지 않고 상자에서만 나온다(2026-08-30, 보스 생사와 상자를 분리한 결과). 티어 5는 시그니처 없음 (확정이면 1회 만에 끝나므로 ring_of_protection은 20% 롤).
- 풀의 나머지 아이템은 **아이템별 확률(`chestChance`)로 독립 롤**. 시그니처는 독립 롤에서 제외해 중복을 막는다.
- 한 던전에서만 나와야 하는 확률 보상은 dungeons.csv `chestRolls`(전용 롤)에 둔다. 같은 티어의 다른 던전은 굴리지 않는다. 규칙은 [DUNGEON_REWARD.md](DUNGEON_REWARD.md).
- 하위 티어 이월템(chestTier < 던전 티어)은 각 10% 보너스 롤 — 놓친 파츠를 상위 던전에서 메꾼다.
- 골드는 `깊이 × 500 ~ 깊이 × 1500`.
- 열 자격은 **마지막 잠긴 층 열쇠 소지**뿐이다(doc/DUNGEON_REWARD.md). 보스 생사는 보지 않는다.
- 캐릭터당 하룻밤(게임 시간) 1회. 열쇠를 소모하고(doc/DUNGEON_REWARD.md) 밤 리셋이 전원을 내보내므로 파킹 방지용 강제 귀환은 없앴다(2026-08-30).
- 독립 롤인 이유: 균등 N개 뽑기는 풀 크기에 따라 개별 확률이 흔들리고 이월템이 신규 파츠 확률을 희석한다. 독립 롤은 파츠별 확률이 풀 크기와 무관해 기대 파밍 횟수를 직접 설계할 수 있다.

### 드랍 확률: 던전당 기대 ~5회

목표: **해당 티어에서 처음 나오는 파츠를 전부 모으는 데 평균 ~5회** (운 나쁘면 더 걸릴 수 있음 — 10회 초과 확률 ≈4~6%). 필요 파츠 K개가 각각 확률 p로 독립 드랍될 때의 완성 기대 횟수 기준: K=1 → p 20%, K=2 → p 30%, K=3 → p 33%, K=4 → p 37%.

| 던전 | 시그니처 (확정) | 독립 롤 파츠 (회당 p) | 완성 기대 |
|------|----------------|----------------------|-----------|
| Old Crypt (T1) | leather_helmet | leather_pants·leather_belt 각 30% | ≈4.7회 |
| Orc Warrens (T2) | leather_armor | leather_gloves·leather_boots·iron_boots·iron_helmet 각 37% | ≈5.0회 |
| Ogre Stronghold (T3) | chain_mail | iron_gauntlets·plate_greaves·plate_boots·기본 망토 각 37% | ≈5.0회 |
| Skeleton Crypt (T4) | breastplate·great_sword | plate_helmet·plate_gauntlets 각 30% | ≈4.7회 |
| Thieves' Den (T4) | breastplate·cutthroat_dirk | plate_helmet·plate_gauntlets 각 30% | ≈4.7회 |
| Sundered Bastion (T5) | (시그니처 없음) | ring_of_protection 20%, 전용 rune_blade 20%·earth_amulet 10% | ≈5.0회 (반지 기준) |
| Shadow Guild (T5) | (시그니처 없음) | ring_of_protection 20%, 전용 shadowfang 20% | ≈5.0회 (반지 기준) |

확률 상수는 아이템 데이터에 `chestChance`로 명시하고, 완성 기대 횟수는 테스트로 고정(시뮬레이션 또는 닫힌식 검증).

### 월드 드랍 (희소·거래템)

- 대상: **특수 망토** (투명·보호 등 — 종류별 개별 롤), **셔츠**, **ring_of_regeneration**. (amulet_of_life_saving은 상점 판매하는 소모품 phoenix_talisman으로 대체 — 희소·거래템 트랙이 아니라 아래 lumber_axe처럼 전역 보너스 테이블 0.2% 드랍)
- 던전 체스트 풀에서 완전히 제외. 티어 4~5 지역·던전의 일반 몬스터 처치 시 **0.1~0.5%** 확률로 드랍 (상위 지역일수록 가중).
- 확정 파밍 경로가 없어 희소성이 유지되고, 유저 간 거래의 축이 된다. "기대 ~5회" 목표의 의도적 예외.
- 귀속(soulbound) 없음 — 거래 가능해야 컨셉이 성립.
- [GATHERING.md](GATHERING.md)의 `lumber_axe`는 이 희소·거래템 트랙이 아니라
  기존 전역 보너스 테이블의 **1% 실용품 월드 드랍**이다. 상점에서도 판매하며,
  `chestTier` 정규 풀에는 넣지 않는다.

### 방패 배치

- raven_shield(guard 2): 티어 2 풀 20% 롤 (세트 완성 목표에는 미포함).
- 장신구는 위 장신구 표를 따른다. leather_belt는 가죽 세트 소속으로 티어 1 풀.

### 상점 안전장치

상점 품목은 최소한으로 유지한다. 세트 파츠는 **티어 1 던전에서 확정적으로 나오지 않는 것만 카탈로그에 둔다** — 확률 롤에 계속 실패한 플레이어의 보험이 목적이므로, 시그니처(확정 드랍)는 보험이 필요 없고 상위 티어 파츠는 파밍 루프를 지키기 위해 팔지 않는다.

- 판매: leather_pants·leather_belt (티어 1, 30% 롤).
- 비판매: leather_helmet (티어 1 시그니처), iron_boots 등 티어 2+ 파츠, iron_sword.

## 무기

원칙: **무기는 체스트에 넣지 않는다.** 획득 경로는 "그 무기를 든 몬스터가 드랍"으로 통일 — 몬스터가 든 무기가 곧 드랍템이라 시각적으로 예고된다. 시작 무기 worn_iron_sword(1d6)에서 출발해 티어당 한 단씩 오르며, 1d8(iron_sword)이 첫 파밍 목표다.

`items.csv`의 `category=weapon`은 피해·인챈트 판정에 쓰는 상위 분류이고,
`weaponType`은 무기군을 나타내는 하위 분류다. 현재 값은 `sword`,
`great_sword`, `dagger`, `spear`, `mace`, `club`, `bow`, `torch`이며 향후 무기를 위해
`axe`, `staff`, `crossbow`도 예약한다. `goblin_sword`와 `small_sword`는
`sword`에 속하며, 단검은 `dagger`로 구분한다. 사거리·양손 여부는 별도 속성이고,
타입별 애니메이션은 `weapon_animations.csv`에서 설정한다.

방패는 `category=armor`를 유지하면서 `armorType=shield`로 구분한다.
`wooden_shield`와 `raven_shield`에 적용하며 `equipSlot=off_hand`여야 한다.
`off_hand` 슬롯의 횃불은 `category=weapon`, `weaponType=torch`이므로 방패가 아니다.
아이템 툴팁에도 `Type: Shield`를 표시한다. 다른 방어구의 `armorType`은 비워 두며,
아이템 ID·피해 주사위·방어력·강화 계산은 그대로 유지한다.

### 티어별 무기

| 티어 | 무기 | dice | basePrice | 획득처 | 상태 |
|------|------|------|-----------|--------|------|
| 1 | goblin_sword·small_sword (1d4), spear (1d6) | 1d4~1d6 | 1,500~3,500 | 몬스터 10% 드랍, 상점 | 있음 |
| 2 | iron_sword | 1d8 | 10,000 | Orc Warrens 상자 확정, 월드 드랍 0.5% (상점 비판매) | 있음 |
| 3 | steel_longsword | 1d10 | 16,000 | 티어 3 던전 무장 몬스터 + Ogre Stronghold 상자 확정 | 있음. 일반 오거는 아직 greatclub(600c) 10% |
| 4 | great_sword | 2d8 | 28,000 | 티어 4 던전 무장 몬스터 + 상자 확정 | 모델·아이콘 등록; 양손무기; 던전 미구현 |
| 5 | rune_blade | 2d10 | 50,000 | Sundered Bastion 상자 전용 롤 20% (ring_of_protection과 동일 철학) | 있음. 양손무기, 바르칸도 들고 싸운다 |

단검(`weaponType=dagger`, DEX 판정, 한손, 사거리 1.5, 1,200ms)은 별도 사다리다. 상점 단검 뒤로 도적 던전 두 곳에서 한 자루씩 오른다.

| 티어 | 단검 | dice | basePrice | 획득처 | 상태 |
|------|------|------|-----------|--------|------|
| 1 | dagger | 1d4 | 2,500 | 상점 | 있음 |
| 4 | cutthroat_dirk | 1d6 | 11,000 | Thieves' Den 상자 확정, 티어 5 상자 이월 10% | 있음 |
| 5 | shadowfang | 1d8 | 20,000 | Shadow Guild 상자 전용 롤 20% | 있음 |

- basePrice는 평균 피해당 가격이 티어마다 오르도록 책정했다. iron_sword 2,200, steel_longsword 2,900, great_sword 3,100, rune_blade 4,500c/pt. 단검도 같은 티어의 값을 따른다(cutthroat_dirk 3,100, shadowfang 4,400c/pt). 대검 가격에 맞추면 두 티어 4 던전을 매일 도는 캐릭터의 상자 수입이 크게 늘어서다.
- 몬스터가 드랍하는 환금 무기는 **무게당 가격을 동일하게** (티어 1은 1,000c/kg: small_sword 1,500/w1.5, goblin_sword 2,000/w2) — 인벤토리가 무게로 차므로 무게당 가격이 다르면 그 무기를 주는 몬스터만 파밍하게 된다. 단검은 예외다. dagger(2,500/w1)는 상점에서만 팔고, 상위 단검은 상자에서만 나온다.
- 이름은 전부 가칭. 몬스터 애니메이션이 Sword 계열뿐이라 날붙이 형태로 제한.

### 드랍 방식: worn 이원화 (티어 3+)

무기는 방어구와 달리 한 자루면 끝이라, 프런티어 무기가 일반 몬스터에서 25%로 팔리는 정품으로 나오면 골드 파우셋이 된다 (킬당 기대 ~1,600c — 오크 ~220c의 7배). 기존 worn(무가격·환금 불가) 메커니즘을 재사용해 분리한다:

- **일반 무장 몬스터: worn 변형을 25% 드랍** — 자기용 획득은 ~4킬로 빠르지만 환금은 안 된다.
- **보스 상자: 정품 확정 드랍** (티어 5만 20% 롤). 보스 본인은 무기를 떨구지 않는다(`weaponDropChance` 0). 상자는 캐릭터당 하룻밤 1회라 캡은 밤당 1자루가 아니라 **열쇠를 쥔 캐릭터당 1자루**다.
- 티어 1~2 무기(600~800c 환금)는 10% 정품 드랍. 환금 파우셋의 최대 항목이라 확률을 보수적으로 유지한다 (수리비·내구도 없이 가는 대신 발행을 드랍율로 조절).

### 시작 무기

신규 캐릭터는 **worn_iron_sword(1d6, 무가격)**를 장착하고 시작한다 — ECONOMY.md의 "시작 장비 환금 불가" 원칙. "worn = 낡아서 무뎌짐"으로 정품 iron_sword(1d8)와 구분되며, 시작 무기가 사다리 최하단이 되어 모든 상위 무기가 업그레이드로 성립한다. spear류는 전용 공격 애니메이션이 없어 시작 무기로 쓰지 않는다.

## 남은 작업

1. 신규 애셋: 특수 망토, 셔츠, 장신구 아이콘 2종(life_saving·regeneration), steel_longsword·great_sword·rune_blade 애셋은 등록 완료. Meshy/ChatGPT 생성, `doc/assets/items.md`에 기록. 망토는 프로시저럴이라 3D 애셋이 필요 없다 — 새 망토는 items.csv 행 + 아이콘이면 된다.
2. ~~back 슬롯 캐릭터 부착 렌더링~~ 완료 (2026-08-17). shirt 칸은 해당 아이템 등장 시 추가.
3. 특수 망토·장신구 효과 시스템 — 투명은 서버 측 가시성 처리, 사망 방지·재생은 서버 전투 로직.
4. **유저 간 거래 시스템.** 현재 `trading.rs`는 NPC 상인뿐, P2P 거래 미구현 — 월드 드랍 희소템 컨셉의 전제.
5. 몬스터 월드 드랍 경로 — 사망 시 무기 드랍(`weapon_drop_chance`, `combat.rs`)을 희소템 롤로 확장.
6. 신규 던전 2개 — 티어 4 → 5 순 (드랍 구성은 티어 로드맵 참조). 던전 생성기 변경 시 골든 해시 게이트 준수, chestTier·chestChance 지정 시 `chest_tiers_gate_endgame_loot_by_dungeon` 테스트 갱신.
7. 월드 드랍 희소템 경제(특수 망토·셔츠·상위 장신구 + 유저 간 거래)는 마지막 단계로.
