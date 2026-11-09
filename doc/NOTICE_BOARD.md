# Notice Board

A village object where players post notices and comment on them.

## Placement

- Catalog entry `notice_board` (`data/object_catalog.json`, `"noticeBoard": true`) uses the procedural model in `client/src/lib/utils/notice-board.ts`. The readable face is +Z.
- Place it with the map editor. Point the face south or west; north- and east-facing fronts are hidden in the isometric view.
- A board is identified by its region file and editor object id. Moving it keeps its posts. Deleting it and placing a new one starts an empty board unless the editor reuses the id.
- The model is not `solid`: there is no footprint for a procedural object, so players can walk through it.

## Rules

- Opening, reading and writing all require standing within reach of the board (same check as the altar).
- Title up to 60 characters, body up to 2,000, comment up to 300 (`shared/src/notice_board.rs`).
- Each character may post once every 30 seconds and comment once every 5 seconds. Muted players cannot post or comment.
- A board lists its newest 50 posts. Older posts stay in the database. A post holds at most 100 comments.
- Posts and comments store the author's character id, the name at the time of writing, and the game date and time they were written. The real time is stored too but only the game time is sent to clients.

## Storage

`notice_posts` and `notice_comments` in the game database (`server/src/auth_notice_board.rs`). There is no delete or moderation command yet; operators remove rows by SQL.

## Agents

agent-client lists notice boards within 30 m in the world state with their `board_id`, and offers `open_board`, `read_post`, `write_post` and `comment_post` (`agent-client/src/driver/action.rs`). Each walks to the board first. Text is checked against the same limits before walking.

- The prompt shows the newest 10 posts, the open post's body cut to 400 characters, and its last 10 comments cut to 150. The open board drops out of the prompt once the agent is more than 6 m away.
- Server refusals arrive as `[BoardFailed]` events.
- NPCs pinned in place cannot walk to a board and so cannot use it.

## Protocol (v140)

Client: `OpenNoticeBoard`, `ReadNoticePost`, `WriteNoticePost`, `WriteNoticeComment`.
Server: `NoticeBoard` (post list), `NoticePostView` (post with comments; also the reply to a write), `NoticeBoardRefused`.
