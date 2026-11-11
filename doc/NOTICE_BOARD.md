# Notice Board

A village object where players post notices and comment on them.

## Placement

- Catalog entry `notice_board` (`data/object_catalog.json`, `"noticeBoard": true`) uses the procedural model in `client/src/lib/utils/notice-board.ts`. The readable face is +Z.
- Place it with the map editor. Point the face south or west; north- and east-facing fronts are hidden in the isometric view.
- A board is identified by its region file and editor object id. Moving it keeps its posts. Deleting it and placing a new one starts an empty board unless the editor reuses the id.
- The board is `solid`. Having no GLB to measure, its catalog entry gives the footprint (1.84×0.8 m), which `tools/measure-furniture-footprints.mjs` copies into `data/furniture_footprints.json`. A cell is normally sealed at 40% coverage; the board sets `cellCoverage` 0.15 so the cells along its whole length are sealed wherever it stands on the 1 m grid.

## Paper

- Each post spends one Notice Paper (`notice_paper`, 200c). Aldwin the land registrar sells it; his sales burn the gold, so the price is the stamp duty. Comments are free.
- The paper is taken when the post is accepted and returned if the database write fails.
- A post is pinned where the writer drags it on the board (`position`, the paper centre in thousandths of the face, y down). Papers may overlap, and newer ones lie on top, but a new paper may cover at most 20% of another writer's current paper; your own and expired papers may be covered freely. Without a position the server picks an allowed spot; when none is left the board is full and the post is refused before any paper is spent.
- Each character may have at most 3 current posts on a board.
- Every paper is 0.3×0.36 m, with or without a picture. Positions are clamped so the whole paper stays on the 1.66×1.4 m face. Each paper gets a small tilt derived from its id.

## Pictures

- A post may carry one picture: the client fits it inside 512 px and uploads it to the cape texture store (`/api/cape-texture`, `server/src/cape_texture.rs`), which re-encodes it and returns its hash. The post stores the hash; blocking a hash there also stops it showing on boards.
- The body may be empty when a picture is attached (a wanted poster with a title is enough).

## Expiry

- A post expires 30 game days after it was written (a game month, about 90 real hours). Expired papers show yellowed with an "Expired" stamp.
- Aldwin takes them down. His schedule (`agent-client/data/npcs/steward/schedule.json`, `notice_rounds`) makes his driver peek boards within 30 m every 5 minutes during the day shift; when one shows an expired paper he leaves his desk, walks to the board, sends `TakeDownNotices`, and the schedule walks him back. Only the land registrar may take notices down, and only expired ones.
- If he misses them, posts older than 31 game days are hidden anyway. Taken-down posts keep their rows with `removed_at` set.

## Removal

- Posts cannot be edited. The writer may take down their own post, current or expired, while standing at the board (`RemoveNoticePost`); anyone else is refused with `not_yours`.
- The paper is not returned, and the post's comments stop being shown with it. The row keeps its content with `removed_at` set, as for a take-down.
- A removed post no longer counts toward the 3-post limit.

## Rules

- Opening, reading and writing all require standing within reach of the board (same check as the altar).
- Title up to 60 characters, body up to 2,000, comment up to 300 (`shared/src/notice_board.rs`).
- Each character may post once every 30 seconds and comment once every 5 seconds. Muted players cannot post or comment.
- A board lists its newest 50 current posts. Older posts stay in the database. A post holds at most 100 comments.
- Posts and comments store the author's character id, the name at the time of writing, and the game date and time they were written. The real time is stored too but only the game time is sent to clients.

## Storage

`notice_posts` and `notice_comments` in the game database (`server/src/auth_notice_board.rs`). `notice_posts` gained `pos_x`, `pos_y`, `image` and `removed_at`; rows from before have no position and get one derived from their id. There is no moderation command yet; operators set `removed_at` by SQL. Posts written more than 60 real days ago are deleted with their comments when the server starts, so every deploy clears them.

## Board model

The 3D board shows the real papers as blank sheets (posters with a dark block for the picture; no picture textures load in the world). Each placed board sends `PeekNoticeBoard` when it is built and every 60 s while it stays loaded; the server answers from an in-memory cache per board, cleared on every post, removal and take-down, and only to players within 80 m.

## Agents

agent-client lists notice boards within 30 m in the world state with their `board_id`, and offers `open_board`, `read_post`, `write_post`, `comment_post` and `remove_post` (`agent-client/src/driver/action.rs`). Each walks to the board first. Text is checked against the same limits before walking. Agents need Notice Paper to post like players do; the server picks the spot. Listed posts note a picture and EXPIRED.

- The prompt shows the newest 10 posts, the open post's body cut to 400 characters, and its last 10 comments cut to 150. The open board drops out of the prompt once the agent is more than 6 m away.
- Server refusals arrive as `[BoardFailed]` events.
- NPCs pinned in place cannot walk to a board and so cannot use it.

## Family boards

`family_board` is estate furniture (Aldwin, 2,000c) that only goes on an estate attached to a family (doc/FAMILY_SYSTEM.md §11). It uses the same tables under `BoardKey::estate(furniture_id)`. Only members of the board's family read it, and only while it stands on that family's estate. Writing and commenting need `BOARD_WRITE`; `BOARD_MODERATE` takes down anyone's post. Posts need no Notice Paper and never expire; the newest 50 are listed. The registrar does not take them down. Agents use village boards only.

## Protocol (v146)

Every message names its board with `NoticeBoardRef`: `Village { object_id }` or `Estate { furniture_id }` (since v146; `TakeDownNotices` keeps `object_id`).

Client: `OpenNoticeBoard`, `ReadNoticePost`, `WriteNoticePost` (with `image` and `position`), `WriteNoticeComment`, `RemoveNoticePost`, `PeekNoticeBoard`, `TakeDownNotices`.
Server: `NoticeBoard` (post list; also follows a removal), `NoticePostView` (post with comments; also the reply to a write), `NoticeBoardRefused`, `NoticeBoardPapers` (papers for the model), `NoticesTakenDown`, `NoticePostRemoved`.
