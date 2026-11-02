# Dungeon System — 층과 Y의 권위 모델

던전에서 **누가 층을 정하고 누가 Y를 정하는가**를 다룬다. 지형 생성과 맵 포맷은
[MAP_DESIGN.md](MAP_DESIGN.md), 몬스터 배치는 [NPC_MONSTER_AI.md](NPC_MONSTER_AI.md), 보스 상자 규칙은 [DUNGEON_REWARD.md](DUNGEON_REWARD.md)에 있다.

[티어 5 던전과 필드 레이드 연계 설계](TIER5_FIELD_RAID.md): 던전 상자의 아뮬렛을 착용해 효과를 받거나 제단에 봉헌해 테라클래시를 소환하는 미구현 디자인 초안.

## Skeleton Crypt — 티어 4

- 입구: `world(-1064, 0.95, 4248)`, 서쪽 도로를 향하는 입구(`entranceDir=w`). 20층, 최종 보스 `skeleton_knight`(레벨 25·HP 350).
- 1–5층: 잊힌 망자(Forgotten Dead, `skeleton_weak`, 레벨 8·HP 40·방어 14·피해 3d6). 기존 Skeleton 모델·애니메이션·소리를 재사용한다.
- 6–10층: 기존 Skeleton만 배치한다(기본 레벨 16·방어 20·명중 보너스 +20·피해 6d8, 깊이 보정 적용). 11–15층: Skeleton과 Skeleton Warrior(가중치 3:2). 16–20층: Skeleton Warrior. 기사는 20층에 한 마리만 배치한다. 모두 선공형이다.
- `dungeons.csv`의 `spawnGroup=skeleton`과 `dungeon_spawns.csv`의 `group`으로 전용 몬스터 풀을 연결한다. 빈 그룹은 기존 던전의 공용 풀이다.
- 5·10·15·20층 문에 각각 `skeleton_key_5/10/15/20`을 사용한다. 각 문 바로 앞 네 층에서 기존 확률(몬스터 5%, 잡동사니 1%)로 열쇠를 얻는다.
- 최종 상자: `breastplate`·`great_sword` 확정, `plate_helmet`·`plate_gauntlets` 각 30%, 하위 티어 장비 각 10%, 10,000–30,000c. 20층 열쇠를 포함해 소지한 해당 던전 열쇠를 소모한다. 캐릭터별 게임 하룻밤 1회이며 보스 생사는 무관하다.
- 기사 처치 칭호: `skeleton_slayer`, 단독 처치 `skeleton_slayer_solo`.
- 입구 주변 60m 미만은 Reserved, 150m 이내는 Crown 기본 규칙을 따른다. 편집된 토지 등급 파일은 기본값을 덮으므로 `tools/reserve-dungeon-land.mjs`로 해당 구획만 갱신한다. 소유지와 150m 범위가 겹치면 중단하며 기존 Reserved와 범위 밖 편집은 유지한다.

운영 반영 시 서버 정지 중 최신 `land_plots`를 JSON으로 추출하고 아래 명령을 실행한다. `--apply`를 빼면 미리보기이며, 적용 시 원본 백업을 남긴다. 개발 토지 등급 파일 전체를 운영으로 복사하지 않는다.

```sh
node tools/reserve-dungeon-land.mjs --dungeon skeleton_crypt --ownership /tmp/owned-plots.json --apply
```

`dungeons.csv`에 아직 없는 장소는 `--dungeon` 대신 `--x=X --z=Z --label NAME`으로 같은 규칙을 적용한다(음수 좌표 때문에 `=`로 붙인다). 등록 전이라 기본값에 링이 없으므로, 등급 파일이 없는 지역은 서버 기본값(지도 라벨·던전 입구 링)으로 새 파일을 만든 뒤 예약을 더한다. 파일이 생기면 그 지역은 이후 기본값 변경을 따르지 않으니, 나중에 입구 좌표가 바뀌면 `--dungeon`으로 다시 실행한다.

## Sundered Bastion — 티어 5

설계는 [TIER5_FIELD_RAID.md](TIER5_FIELD_RAID.md). 1단계(던전 본체)만 구현했다.

- 입구: `world(-1950, 1.1, 5456)`, 남향(`entranceDir=s`). 25층, 최종 보스 `barkan`(레벨 30·HP 600·guard 36·피해 7d12). 모델은 `barkan.glb`이며 기준 높이 2.5m·스케일 1이다. 이동·추적은 Orc Walk로 3.0m/s이며, 오른손에 룬 블레이드를 장착한다. 평타·사망은 기존 Mixamo 동작을 옮긴 임시 모션이다. 15초마다(첫 시전은 교전 6초 뒤) 평타 대신 룬 검격을 쓴다. 검을 들어 올리는 동안 정면을 고정하고, 2.55초에 검이 옆으로 지나갈 때 전방 90°(±45°)·4m 안의 플레이어에게 명중 굴림 없이 평타 피해 ×5를 준다. 시전(4.2초) 중에는 경직되지 않고, 끝난 뒤 평타 쿨다운(2.8초) 동안은 반격할 틈이 있다. 규칙은 `shared/src/monster_ai/skill.rs`.
- 입구 주변 80×80m 범위의 지형은 모두 입구 높이 − 1m 이상이어야 한다. 서버는 이 범위에서 `입구 높이 − 4m × 층` ±2.5m를 그 층으로 판정하므로, 낮은 지면은 지하로 취급된다. 그래서 경사지가 아닌 평지에 둔다.
- 몬스터는 `data-src/dungeon_spawns.csv`의 `bastion` 그룹이다. 모든 던전의 스폰은 이 파일에서 그룹·몬스터·깊이 범위별 행으로 정의하므로, 같은 몬스터를 던전마다 다른 깊이에 둘 수 있다. 모두 선공형이다.
- 층수 상한은 `MAX_FLOORS`(25)이고, `floors`가 비어 있는 던전의 무작위 층수 범위 `MIN_DRAWN_FLOORS..=MAX_DRAWN_FLOORS`(5–20)는 기존 배치를 지키기 위해 그대로 둔다. 깊이 보정 몬스터 레벨 상한은 25다.
- 5·10·15·20·25층 문에 `bastion_key_5…25`. 아이콘·모델은 기존 열쇠를 재사용한다.
- 최종 상자: 확정 보상 없음, 전용 롤(`chestRolls`) `rune_blade` 20%·`earth_amulet` 10%, 티어 5 확률 보상(`ring_of_protection` 20%)과 하위 티어 장비, 12,500–37,500c.
- 바르칸 처치 칭호: `barkan_slayer`, 단독 처치 `barkan_slayer_solo`.

## Thieves' Den — 티어 4

- 입구: `world(-862, 1.2, 3822)`, 서쪽 도로를 향한다(`entranceDir=w`). Skeleton Crypt에서 북동쪽으로 약 470m. 20층, 최종 보스 `thief_king`(레벨 25·HP 350·guard 32).
- 보스 모델은 2.1m `monsters/thief_king.glb`다. 동작 6종과 오른손 단검 파지를 GLB에 구웠고 2초마다 단검으로 찌른다(`dagger_stab`)([애셋 기록](assets/monsters.md)).
- 몬스터는 `den` 그룹이다. 기존 모델에 Skeleton Crypt 같은 층 몬스터의 레벨·guard·명중·피해를 입힌 변형이다. 무기와 드롭은 원래 몬스터를 따른다.
  - 1–5층: Kobold Cutpurse(`kobold_cutpurse`). 잊힌 망자 수치(레벨 8·HP 40·guard 14·2d6).
  - 6–10층: Hobgoblin Thug·Gnoll Stalker. Skeleton 수치(레벨 16·guard 20·명중 +20·3d6). Stalker는 출혈 공격을 유지한다.
  - 11–15층: Thug·Stalker와 Bugbear Enforcer를 3:2. Enforcer는 Skeleton Warrior 수치(레벨 20·guard 24·3d6).
  - 16–20층: Enforcer.
- 5·10·15·20층 문에 `den_key_5…20`. 아이콘·모델은 기존 열쇠를 재사용한다.
- 최종 상자: `breastplate`·`cutthroat_dirk` 확정, `plate_helmet`·`plate_gauntlets` 각 30%, 하위 티어 장비 각 10%, 10,000–30,000c. `great_sword`는 Skeleton Crypt의 확정 보상이라 여기서는 나오지 않는다.
- 처치 칭호: `thief_king_slayer`, 단독 처치 `thief_king_slayer_solo`.

## Shadow Guild — 티어 5

- 입구: `world(-2384, 1.05, 5962)`, 남향(`entranceDir=s`). Sundered Bastion에서 도로를 따라 남서쪽으로 약 670m. 25층, 최종 보스 `shadow_master`(레벨 30·HP 600·guard 36). 보스 모델은 [애셋 기록](assets/monsters.md)을 참고한다.
- 몬스터는 `guild` 그룹이다. Sundered Bastion 같은 층 몬스터의 수치를 입힌 변형이고, 층별 비율도 같다.
  - Stone Golem 자리: Orc Bruiser(레벨 10·guard 17·2d10).
  - Lizardfolk 자리: Lizardfolk Assassin(같은 수치).
  - Cyclop 자리: Ogre Smuggler(레벨 15·guard 20·3d8).
  - Skeleton 자리: Thug·Stalker. Skeleton Warrior 자리: Enforcer.
- 5·10·15·20·25층 문에 `guild_key_5…25`.
- 최종 상자: 확정 보상 없음, 전용 롤 `shadowfang` 20%, 티어 5 확률 보상(`ring_of_protection` 20%), 하위 티어 장비 각 10%(`cutthroat_dirk`·`great_sword` 포함), 12,500–37,500c. `rune_blade`와 `earth_amulet`은 Sundered Bastion 전용 롤이라 나오지 않는다([DUNGEON_REWARD.md](DUNGEON_REWARD.md)).
- 처치 칭호: `shadow_master_slayer`, 단독 처치 `shadow_master_slayer_solo`.

두 입구 모두 80×80m 범위 전체가 입구 높이 − 1m 이상이다(Thieves' Den 0.3–1.75m, Shadow Guild 0.7–1.5m). 2026-11-01 운영 `land_plots` 기준으로 150m 안에 소유지가 없었고, 운영에는 두 지역(`r-01_+03`, `r-03_+05`)의 토지 등급 파일이 없어 등록하면 기본 링이 적용된다. 배포 때 최신 추출로 `--dungeon thieves_den`과 `--dungeon shadow_guild`를 다시 돌린다.

## Blackvein Mine — 티어 5

[제안 이슈 #204](https://github.com/Julian-adv/OpenMMO/issues/204)의 최소판이다. 맵은 기존 생성기를 쓰고, 몬스터는 기존 모델에 제안서의 이름을 붙인 대역이다.

- 입구: `world(-1884, 1.2, 6360)`, 서쪽 도로를 향한다(`entranceDir=w`). Shadow Guild에서 북동쪽으로 약 640m, 동쪽 산맥이 시작되는 기슭이다. 25층, 최종 보스 `mine_foreman`(Rampaging Foreman, 레벨 30·HP 600·guard 36).
- 80×80m 범위는 0.55–1.95m다. 2026-11-03 운영 `land_plots` 기준 가장 가까운 소유지는 446m 밖이고, 이 범위의 운영 지형 타일은 개발과 같다. 배포 때 최신 추출로 `--dungeon blackvein_mine`을 돌린다.
- 몬스터는 `mine` 그룹이고 5층마다 한 종이다. 수치는 Sundered Bastion 같은 층 몬스터를 따른다.
  - 1–5층: Lingering Miner(광부 잔존자, `mine_remnant`). Hobgoblin 모델, Stone Golem 수치.
  - 6–10층: Crystallized Miner(결정화 광부, `mine_crystal`). Orc 모델, Cyclop 수치.
  - 11–15층: Twisted Miner(뒤틀린 광부, `mine_twisted`). Troll 모델, Skeleton 수치. 출혈 공격을 유지한다.
  - 16–20층: Veinfused(광맥 융합체, `mine_veinfused`). Stone Golem 모델, Skeleton Warrior 수치.
  - 21–24층: Deep Predator(심층 포식자, `mine_predator`). SCP-939 모델, Skeleton Warrior 수치.
  - 25층: 보스만 나온다. 서버는 마지막 층에 한해 빈 스폰 표를 허용한다.
  - 보스: Cyclop 모델, Shadow Master 수치.
- 5·10·15·20·25층 문에 `mine_key_5…25`. 아이콘·모델은 기존 열쇠를 재사용한다.
- 최종 상자: 확정 보상·전용 롤 없음. 티어 5 확률 보상(`ring_of_protection` 20%)과 하위 티어 장비, 12,500–37,500c.
- 처치 칭호: `foreman_slayer`, 단독 처치 `foreman_slayer_solo`.
- 맵은 `layoutStyle=mine` 생성기다(`shared/src/dungeon/gen/mine.rs`). 비어 있으면 기존 방 생성기를 쓴다.
  - 일반 층: 도착 방에서 작은 방 2–3개를 거쳐 계단 방까지 이어지는 긴 2칸 통로. 이어지는 방의 중심은 맨해튼 거리 18칸 이상 떨어진다.
  - 5·10·15·20층: 도착 방에서 갈림목 방으로, 갈림목에서 넓은 작업장 셋으로 통로가 갈린다. 내려가는 계단은 그중 한 곳에만 있고, 몬스터가 3마리 더 나온다.
  - 25층: 일반 층처럼 이어지다 넓은 최심부 작업장에서 끝나며, 보스와 상자가 있다.
  - 통로는 이어 주는 두 방 말고 다른 방을 지나거나 스치지 않고, 다른 통로와는 둘이 함께 닿는 방 2칸 안에서만 만난다. 그래서 방 사이에 갈림길이 생기지 않는다.
  - 층 이동은 계단 그대로다. 승강기는 광산 소품 단계에서 계단 자리에 넣는다.
- 제안서에서 남은 것: 광산 소품(승강기 포함), 전용 몬스터·보스 모델.

## 리셋과 재접속

- 게임 내 일몰마다 접속 중인 던전 참가자는 경고 후 해당 던전의 지상 입구로 이동한다.
- 오프라인 캐릭터도 같은 규칙을 따른다. 위치와 함께 저장한 `dungeon_epoch`보다 현재
  `night_epoch`가 크면, 재접속 시 원래 던전 입구에서 시작한다. 같은 주기 안의 재접속은
  저장 위치를 유지하며, 서버 재시작만으로 퇴장시키지는 않는다.
- 리셋 경고 중 저장에는 퇴장이 완료된 이전 주기를 사용한다. 경고를 듣고 로그아웃해도
  퇴장을 피할 수 없다. 로그인과 퇴장 처리는 세션 잠금으로 순서를 보장한다.
- 주기 기록이 없는 기존 지하 캐릭터는 첫 재접속 시 입구로 이동한다. 체력·소지품과
  파티 소환 규칙은 변경하지 않는다.

## 모델

- **층은 서버가 들고 있는 상태다.** 클라이언트는 `PlayerFloorChanged`로 요청할 뿐이고,
  바뀌는 경로는 계단 게이트를 통과한 전이 하나뿐이다.
- **Y는 `(층, XZ)`에서 계산하는 파생값이다.** 층 하나의 지면은
  `entrance.y - depth × DUNGEON_FLOOR_HEIGHT`로 평평하고, 계단 shaft 안에서만 램프로
  기울어진다. 클라이언트가 보낸 Y는 던전에서 항상 서버 계산값으로 덮어쓴다
  (`resolve_dungeon_floor` → `verdict.y`).
- **거절은 지오메트리 게이트만 한다.** Y가 어긋난다고 이동이나 층 전이를 거절하지 않는다.

Y를 거절 근거로 쓰지 않는 이유는 그것이 층을 가려주지 못하기 때문이다. 계단 shaft의 램프는
**위아래 두 층이 공유하는 지오메트리**다. 8층의 `down_shaft`와 9층의 `up_shaft`는 같은
사각형이고, 그 안에서 `floor_height_at`은 어느 depth로 물어도 같은 램프 높이를 돌려준다.
층이 갈라지는 것은 shaft 밖뿐이다.

서버 이동이 벽을 따라 미끄러지면 실제 도착 XZ와 충돌 판정에 사용한 층으로 `ground_y_for_floor`를 다시 구한다. 막힌 대각선 이동의 보간 Y를 적용해 계단 아래로 내려앉는 것을 방지한다. 미끄러짐 중 거의 전진하지 못하는 경로는 [이동 재동기화](MOVEMENT_SYNC.md)로 비우고 다시 계획한다.

이동 중 새 경로를 받았을 때 서버가 아직 계단 중간이면, 새 첫 경유점까지 필요한 연결 경로를 서버 A*로 다시 구한다. 계단참 경유점이 빠져 옆벽으로 직진하는 것을 방지한다. 연결 중 클라이언트의 층 알림이 도착해도 서버 높이를 먼저 다른 층으로 옮기지 않는다.

```
x=-1594.7  d8=-34.08  d9=-34.08   ← shaft 안: 두 층이 같은 램프
x=-1589.7  d8=-30.95  d9=-30.95
x=-1588.7  d8=-30.95  d9=-34.95   ← shaft 밖: 여기서 갈라진다
```

## 계단 게이트

층 전이 요청은 네 조건을 모두 만족해야 한다(`resolve_dungeon_floor`).

1. 얕은 쪽이 지하 또는 지상이다.
2. 두 층이 **정확히 인접**하다. 건너뛰기가 없다.
3. 전이를 실은 이동 구간이 `FLOOR_CHANGE_LEG_MAX`(shaft 길이의 두 배) 이내다.
4. 그 구간이 연결 shaft의 footprint(여유 `SHAFT_CHANGE_MARGIN` 1셀)를 **실제로 지난다**.

하나라도 어긋나면 `floor change A -> B off the stairs` 경고와 함께 거절하고, 클라이언트에는
`PositionCorrected`로 권위 층을 돌려준다. 클라이언트는 이 메시지의 `floor_level`을
`dungeonManager.syncFromFloorLevel`로 반영한다 — 반영하지 않으면 거절당한 층을 계속
렌더링하며 매 프레임 재요청한다.

## 왜 이렇게 바꿨나

원래는 층 전이에 Y 일치까지 요구했다(`FLOOR_Y_TOLERANCE` 2.5m). 두 사고가 그 검사에서
나왔다.

- **2026-08-19**: 계단을 내려가는 순간 저장 Y가 아직 떠나는 층을 가리켜 전이가 거절되고,
  플레이어가 이전 층에 붙잡혀 새 층의 브로드캐스트를 받지 못했다. 보스방 문이 열리지 않던
  증상의 원인이다.
- **2026-08-23**: 위 수정("떠나는 층의 지면과 맞으면 수용") 이후에도 한 플레이어에게서
  23시간에 49건이 남았다. shaft 안에서는 떠나는 층과 도착 층의 계산값이 **같은 값**이라
  그 구제가 성립하지 않았다. 램프 한 구간의 높이차가 4.0m인데 허용치가 2.5m라, 램프를
  오르는 중에는 어느 층과도 맞지 않는 구간이 생긴다.

두 번째 사고에서 확인한 사실이 하나 더 있다. 검사에 쓰이던 "reported" Y는 클라이언트가
보낸 값이 아니라 **서버 자신의 저장 Y**였다(`update_player_floor`이 저장 위치를 `from`과
`to` 양쪽에 넘긴다). 그 값은 이동 시뮬레이션이 웨이포인트 사이를 선형 보간해 써넣고 이전
판정이 스냅해 덮어쓴 결과라, 램프의 평지–경사–평지 프로파일과 어긋난다. 즉 서버가 자기
시뮬레이션 오차를 근거로 정상 전이를 거절하고 있었다.

변조 방어는 이 검사 없이도 남는다. 계단 게이트가 "인접 층, 짧은 구간, shaft 위"로 묶고,
Y는 어차피 서버 계산값으로 덮어쓰므로 원하는 높이를 주장해 얻을 것이 없다.

## 남은 관측

평지(shaft 밖)에서 저장 Y가 층 지면에서 한 층 높이 이상 벗어나면 `Y drift` 경고를 남긴다.
거절하지는 않는다. 서버 위치가 자기가 들고 있는 층에서 떠내려간 신호이므로, 위 두 사고를
잡아낸 관측점을 유지하는 용도다.
