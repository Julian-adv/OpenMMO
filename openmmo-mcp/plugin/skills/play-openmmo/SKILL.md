---
name: play-openmmo
description: How to play OpenMMO with the openmmo tools (login, enter, look, act, attack, walk_to, say, leave). Use whenever the user asks you to play, explore, fight, trade or talk in OpenMMO.
---

# Playing OpenMMO

You control one character in a live MMO. Other people may be watching it.

## Each turn

1. `look` before deciding. It lists your HP, floor, rooms, monsters with ids,
   nearby players and what happened since the last look.
2. Do one thing: `act` for most actions, `attack` for a fight, `walk_to` for a
   coordinate, `say` to talk.
3. Read the reply before the next step. Moves can stop early; the reply says why.

`actions` lists what `act` accepts; `actions` with `about` (e.g. "dungeon",
"buy", "eat") shows the full usage. Check it before an action you have not used.

## Rules of thumb

- `attack` only chases monsters within 20 m. Walk closer first.
- Monster HP shows only while you wear a stethoscope, as for human players;
  otherwise you know a monster is alive, not how hurt it is.
- Keep HP above half. Back off with `{"type": "move", "depth": 0}` in a
  dungeon, eat food (`{"type": "use", "target": "apple"}`), or rest in town.
- If you are defeated, `{"type": "respawn"}`; you wake in town.
- Leave only when safe: a character that logs out mid-fight stays in the world
  for a while and can still die.
- Answer chat and whispers (`[Chat]`, `[Whisper]` lines) with `say` or a whisper.
- Only player chat commands work; operator commands are blocked.
- Keep your own messages short; every tool reply costs context.

## Starting

`login` (show the URL and code if one appears), then `enter` with the
character the user names, then `look`.
