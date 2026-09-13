# UI Assets

## Icon

- https://icon-sets.iconify.design/fa6-solid/people-group/
- https://icon-sets.iconify.design/icon-park-solid/backpack/
- https://icon-sets.iconify.design/fa6-solid/handshake-simple/ — social corner button in GameHud.svelte
- https://icon-sets.iconify.design/fa6-solid/face-smile/ — Emotes entry in the social flyout, GameHud.svelte
- GitHub mark (octicon mark-github, MIT) — inline SVG in LoginScreen.svelte
- **[미사용]** Chat expand/collapse filled icons — Font Awesome Free 6.7.2 by Fonticons, Inc., [circle-chevron-right](https://github.com/FortAwesome/Font-Awesome/blob/6.x/svgs/solid/circle-chevron-right.svg) and [circle-chevron-down](https://github.com/FortAwesome/Font-Awesome/blob/6.x/svgs/solid/circle-chevron-down.svg); [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), free tier, added and replaced 2026-09-06 with outline icons.
- Chat expand/collapse outline icons — original inline SVG circles and chevrons authored with OpenAI Codex in `ChatPanel.svelte`; workspace-provided tier (exact tier not exposed), 2026-09-06; project-owned, no external source. Transparent fill with strokes matching the surrounding text colors, displayed at 16px.
- Chat translation dropdown chevrons — original SVG chevrons authored with OpenAI Codex as CSS backgrounds in `ChatPanel.svelte`; workspace-provided tier (exact tier not exposed), 2026-09-06; project-owned, no external source. Right when closed, down when open, displayed at 12px.

## Skill icons

- `client/public/icons/skills/radiance.png` — Radiance 조명 버프 아이콘. OpenAI Codex built-in ImageGen, workspace-provided tier (정확한 등급은 도구에서 공개되지 않음), 생성 2026-09-13. 프로젝트용 생성 이미지이며 OpenAI 서비스 약관의 출력물 이용 조건에 따른다. 기존 Double Slash와 공통 생성 프롬프트의 은백색·옅은 아이보리 붓선 스타일을 사용했다. 사용자가 공격 스킬로 오인할 수 있다고 지적한 원형 궤적을 제거하고, 검은 바탕 위에 빛 문양 하나만 남겼다. [생성·수정 프롬프트](radiance-icon-prompt.txt).
- `client/public/icons/skills/dagger-double-slash-v2.png` — Double Slash 단검 2연격 스킬 아이콘. OpenAI Codex built-in ImageGen edit, workspace-provided tier (exact tier not exposed), generated 2026-09-13; project-owned generated asset, 별도 CC 라이선스 지정 없음. 아래 v1을 참고하여 38px 퀵슬롯용으로 잔선을 줄이고 두 궤적을 분리하며 단검을 크게 만든 버전이다. 검은 배경, 은백색·옅은 아이보리 단색의 거친 빛 붓선과 짧은 잔광을 유지한다. 생성 PNG를 그대로 보관하고 스킬 목록과 공용 퀵슬롯에서 함께 사용한다. [실제 수정 프롬프트](dagger-icon-refinement-prompt.txt).
- **[미사용·시안 보관]** `doc/images/skills/dagger-double-slash-v1.png` — 사용자가 선택한 최초 아이콘. OpenAI Codex built-in ImageGen, workspace-provided tier (exact tier not exposed), generated 2026-09-13; project-owned generated asset, 별도 CC 라이선스 지정 없음. 작은 슬롯에서 선이 복잡하게 보이는 문제로 같은 날 v2로 교체했다.
- 최초 스타일 참고: 사용자가 제공한 [리니지M 인벤 스킬 DB](https://lineagem.inven.co.kr/db/skill/) 아이콘 스크린샷. 외부 참고 이미지의 권리는 원 권리자에게 있으며, 해당 이미지는 게임 에셋에 포함하지 않는다. 이후 생성에는 위 프로젝트 생성 이미지를 기준으로 사용한다.
- [스킬 아이콘 생성 프롬프트](skill-icon-prompts.md) — 기준 이미지, 공통 스타일 프롬프트, Double Slash와 방패 강타 예시, 수정 요청 문구.

## World map

- `client/public/textures/ui/world-map/dark-wood.webp` — OpenAI Codex built-in ImageGen, workspace-provided tier (exact tier not exposed), 2026-08-24; user target image used as the style reference; project-owned generated asset. Shipped as 768² WebP q88 (2026-08-26): the source 1254² PNG was 2.0 MB but is only drawn as a 220 px button tile and a 52 px header bar
- `client/public/textures/ui/world-map/ornate-frame.webp` — OpenAI Codex built-in ImageGen edit, workspace-provided tier (exact tier not exposed), 2026-08-24; user target image used as the style reference, then ffmpeg color-keyed to restore real alpha transparency; project-owned generated asset. Shipped as 1254² WebP q90 (2026-08-26), same resolution as the 1.3 MB source PNG
- Settlement crest marker + player self-marker — hand-authored inline SVG in WorldMapDialog.svelte (OpenAI Codex, 2026-08-24), original shapes with no external source

## Instrument UI

- `client/public/textures/ui/instrument/mandolin-ornament.webp` — OpenAI Codex built-in ImageGen, workspace-provided tier (exact tier not exposed), 2026-08-30; project-owned generated asset. Source 1983×793 PNG, color-keyed, cropped, and scaled to a transparent 1600×323 WebP for the free-play HUD
- **[미사용·삭제]** `client/public/textures/ui/instrument/mandolin-emblem.webp` — OpenAI Codex built-in ImageGen using the mandolin ornament as a material and palette reference, workspace-provided tier (exact tier not exposed), 2026-08-30; project-owned generated asset. Source 1254² PNG, background-keyed and scaled to a transparent 256² WebP; removed from the free-play HUD in favor of clean negative space, then deleted from the repository on 2026-08-30

## Party UI design mockups

- `doc/design/party-ui/openmmo-current-game.jpg` — local OpenMMO gameplay capture from `localhost:10004`, 2026-08-05
- `doc/design/party-ui/concept-a-compact-tactical.png` — OpenAI Codex built-in ImageGen image edit/composite, workspace-provided tier (exact tier not exposed), 2026-08-05
- **[미사용]** `doc/design/party-ui/concept-a-v2-class-icon-leading-no-hp-numbers.png` — OpenAI Codex built-in ImageGen precise-object edit using a preview of the project-owned party class icons, workspace-provided tier (exact tier not exposed), 2026-08-05; replaced by exact SVG composites
- `doc/design/party-ui/concept-b-portrait-cards.png` — OpenAI Codex built-in ImageGen image edit/composite, workspace-provided tier (exact tier not exposed), 2026-08-05
- `doc/design/party-ui/concept-c-horizontal-combat-strip.png` — OpenAI Codex built-in ImageGen image edit/composite, workspace-provided tier (exact tier not exposed), 2026-08-05
- Inputs for the three mockups: the local gameplay capture and the existing `female_knight.png`, `ranger.png`, `female_priest.png`, and `rogue.png` character concept assets
- `doc/design/party-ui/concept-01-fixed-column.png` — deterministic local composite using the gameplay capture, project character concepts, and exact party SVG assets; project-owned, 2026-08-05
- **[미사용]** `doc/design/party-ui/concept-02-portrait-badge.png` — deterministic local composite using the gameplay capture, project character concepts, and exact party SVG assets; replaced by the inset-icon revision, project-owned, 2026-08-05
- `doc/design/party-ui/concept-02-portrait-inset-red-hp.png` — deterministic local composite using the gameplay capture, project character concepts, and exact party SVG assets; icon-only portrait inset with red HP bars, project-owned, 2026-08-05
- `doc/design/party-ui/concept-03-icon-tile.png` — deterministic local composite using the gameplay capture, project character concepts, and exact party SVG assets; project-owned, 2026-08-05
- `doc/design/party-ui/concept-04-class-rail.png` — deterministic local composite using the gameplay capture, project character concepts, and exact party SVG assets; project-owned, 2026-08-05
- `doc/design/party-ui/concept-05-portrait-side-tab.png` — deterministic local composite using the gameplay capture, project character concepts, and exact party SVG assets; project-owned, 2026-08-05

## Chat panel examples

- `doc/design/chat-panel/expanded.png` — local OpenMMO gameplay capture supplied by the contributor, showing the expanded chat panel; project-owned, 2026-09-05
- `doc/design/chat-panel/collapsed.png` — local OpenMMO gameplay capture supplied by the contributor, showing the collapsed chat panel; project-owned, 2026-09-05

## Party class icons

- `client/public/icons/party/class-knight.svg` — original OpenMMO vector asset authored with OpenAI GPT Image 2, ChatGPT Pro tier, 2026-08-05; project-owned
- `client/public/icons/party/class-barbarian.svg` — original OpenMMO vector asset authored with OpenAI GPT Image 2, ChatGPT Pro tier, 2026-08-05; project-owned
- `client/public/icons/party/class-caveman.svg` — original OpenMMO vector asset authored with OpenAI GPT Image 2, ChatGPT Pro tier, 2026-08-05; project-owned
- `client/public/icons/party/class-valkyrie.svg` — original OpenMMO vector asset authored with OpenAI GPT Image 2, ChatGPT Pro tier, 2026-08-05; project-owned
- `client/public/icons/party/class-ranger.svg` — original OpenMMO vector asset authored with OpenAI Codex, workspace-provided tier (exact tier not exposed), 2026-08-05; project-owned
- `client/public/icons/party/class-priest.svg` — original OpenMMO vector asset authored with OpenAI GPT Image 2, ChatGPT Pro tier, 2026-08-05; project-owned
- `client/public/icons/party/class-rogue.svg` — original OpenMMO vector asset authored with OpenAI GPT Image 2, ChatGPT Pro tier, 2026-08-05; project-owned
- `client/public/icons/party/class-bard.svg` — original OpenMMO mandolin vector asset authored with OpenAI Codex, workspace-provided tier (exact tier not exposed), 2026-08-06; project-owned
- `client/public/icons/party/leader-crown.svg` — original OpenMMO vector asset authored with OpenAI Codex, workspace-provided tier (exact tier not exposed), 2026-08-05; project-owned

## NPC 거래 초상화

- `client/public/portraits/karl.webp` — 경비병 Karl 초상화; OpenAI ChatGPT 이미지 생성, ChatGPT Pro tier, 2026-06-12; project-owned. 원본(배경 있는 1122×1402 PNG)은 `../images/characters/karl-portrait.png`; 배포본은 커밋 edf3acd7의 배경 제거된 1122² PNG를 축소한 것 (원본에서 어떻게 정사각형으로 다듬었는지는 기록 없음)
- `client/public/portraits/rica.webp` — 상인 Rica 초상화; OpenAI ChatGPT 이미지 생성, ChatGPT Pro tier, 2026-06-10; project-owned. 원본(배경 있는 1300×1210 PNG)은 `../images/characters/rica-portrait.png`, 배경 제거·정사각 크롭한 1210² PNG는 커밋 aa29e30f에 남아 있다 (비율 왜곡 없음)
- `client/public/portraits/wick.webp` — 야간 상인 Wick의 거래 창 초상화; OpenAI ChatGPT 이미지 생성, ChatGPT Pro tier, 2026-08-27; project-owned. 원본(배경 있는 1122×1402 PNG)은 `../images/characters/wick-portrait.png`; 배포본은 그 위쪽 정사각 크롭(`crop=1122:1122:0:0`)을 배경 제거한 것
- `client/public/portraits/miriel.webp` — 여관 메이드 Miriel의 거래 창 초상화; 2026-09-02 추가, 생성 도구·등급 미기록(원본 메타데이터에는 Paint.NET 5.1.12 편집 흔적만 있음); project-owned. 원본(배경 제거된 1230² PNG)은 `../images/characters/miriel-portrait.png`; 배포본은 512² WebP q88, alpha 유지, 비율 그대로
- `client/public/portraits/cocoly.webp` — 여관 메이드 Cocoly의 거래 창 초상화; 2026-09-02 추가, 생성 도구·등급 미기록(원본 메타데이터에는 Paint.NET 5.1.12 편집 흔적만 있음); project-owned. 원본(배경 제거된 1056² PNG)은 `../images/characters/cocoly-portrait.png`; 배포본은 512² WebP q88, alpha 유지, 비율 그대로
- `client/public/portraits/steward.webp` — 영지 관리인 Aldwin의 거래 창 초상화; 사용자 제공 OpenAI ChatGPT 생성 이미지, ChatGPT Pro x20 등급, 생성 2026-09-07(제공 파일명 기준); project-owned. 원본은 `../images/characters/steward-portrait.png`에 보관(1337×1177 PNG, alpha 포함). 배포본은 512×451 WebP q88, alpha와 원본 비율 유지(2026-09-07 등록). `traderId=steward`로 자동 연결
- `client/public/portraits/estate_architect.webp` — 영지 건축가 Rowan의 거래 창 초상화; 사용자 제공 OpenAI ChatGPT 생성 이미지, ChatGPT Pro x20 등급, 생성 2026-09-07(제공 파일명 기준); project-owned. 제공 파일 `doc/images/characters/ChatGPT Image 2026년 9월 7일 오후 03_31_23.png`는 `../images/characters/estate-architect-portrait.png`로 이름을 바꿔 원본 보관(1222×1287 PNG, alpha 포함). 배포본은 512×539 WebP q88, alpha와 원본 비율 유지(2026-09-07 등록). `traderId=estate_architect`로 자동 연결

세 장 모두 512² WebP q88(알파 유지)로 축소해 배포한다 (2026-08-27) — 비율은 건드리지 않고 크기만 줄인다. `TradeWindow.svelte`가 폭 160px로 그리므로 1122~1210² 원본은 3배 넘게 과했다. 합계 4.8MB → 194KB. 파일명이 곧 `traderId`라 `/portraits/{traderId}.webp` 규칙으로 자동 해석된다 (새 초상화 추가 시 코드 변경 불필요).

## Skill icons

- `client/public/icons/skills/guardian-ward.png` — Guardian Ward (검의 수호) 능력 및 퀵슬롯 아이콘. OpenAI Codex built-in ImageGen, workspace-provided tier (정확한 등급은 도구에서 공개되지 않음), 생성 2026-09-13. 프로젝트를 위해 생성한 이미지이며 OpenAI 서비스 약관의 출력물 이용 조건에 따른다. 기존 Double Slash 아이콘/생성 프롬프트를 스타일 기준으로 사용했다. 검은 바탕, 옅은 금빛 방패와 보호 궤적, 짧은 빛 번짐. [실제 생성 프롬프트](guardian-ward-icon-prompt.txt). PNG는 게임 Git 저장소에서 직접 관리한다.
- `client/public/icons/skills/true-aim.png` — True Aim 어빌리티·퀵슬롯 아이콘. OpenAI Codex built-in ImageGen, workspace-provided tier (정확한 등급은 도구에 공개되지 않음), 생성 2026-09-13. 프로젝트용 생성 이미지이며 OpenAI 서비스 약관의 출력물 이용 조건에 따른다. 기존 Double Slash 아이콘을 스타일 참조로 사용한 검은 바탕의 은백색 조준 문양. [실제 생성 프롬프트](true-aim-icon-prompt.txt).
