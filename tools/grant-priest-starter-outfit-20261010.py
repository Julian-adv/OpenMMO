"""Grant the priest starter outfit to existing male priests, once, with the
game and NPC services stopped. Preview first; --apply re-runs the preview and
refuses if the plan changed."""
import argparse
import json
import sqlite3
from pathlib import Path


STARTER_OUTFIT = (
    "worn_priest_helmet",
    "worn_priest_top",
    "worn_priest_pants",
    "worn_priest_boots",
)


def plan_grants(conn, npc_names, apply=False):
    conn.execute("BEGIN IMMEDIATE")
    grants = []
    try:
        characters = conn.execute(
            "SELECT id, account_name, character_name FROM characters "
            "WHERE gender = 'male' AND class = 'priest' ORDER BY id"
        ).fetchall()
        for character_id, account, name in characters:
            if account.startswith("npc_") and name in npc_names:
                continue
            existing = {
                row[0]
                for row in conn.execute(
                    "SELECT item_def_id FROM character_items WHERE character_id = ?",
                    (character_id,),
                )
            }
            for item_id in STARTER_OUTFIT:
                if item_id in existing:
                    continue
                grant = {"character_id": character_id, "item_id": item_id}
                if apply:
                    cursor = conn.execute(
                        "INSERT INTO character_items (character_id, item_def_id, quantity) "
                        "VALUES (?, ?, 1)",
                        (character_id, item_id),
                    )
                    grant["item_row_id"] = cursor.lastrowid
                grants.append(grant)
        if apply:
            conn.commit()
        else:
            conn.rollback()
    except Exception:
        conn.rollback()
        raise
    return grants


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("db", type=Path)
    parser.add_argument("--apply", action="store_true", help="Write items; default is preview.")
    parser.add_argument(
        "--expected-preview",
        type=Path,
        help="Preview JSON this run must match before applying.",
    )
    parser.add_argument(
        "--data",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "data",
        help="Game data dir holding npcs.json and items.json.",
    )
    args = parser.parse_args()
    npc_names = {
        npc["npcName"] for npc in json.loads((args.data / "npcs.json").read_text()).values()
    }
    items = json.loads((args.data / "items.json").read_text())
    for item_id in STARTER_OUTFIT:
        item = items[item_id]
        if not item.get("untradeable") or item.get("basePrice") is not None:
            raise ValueError(f"Invalid starter outfit definition: {item_id}")
    with sqlite3.connect(f"{args.db.resolve().as_uri()}?mode=rw", uri=True, timeout=10) as conn:
        conn.execute("PRAGMA foreign_keys = ON")
        if args.apply:
            if args.expected_preview is None:
                raise ValueError("--apply requires --expected-preview")
            expected = json.loads(args.expected_preview.read_text())
            if expected["applied"] or plan_grants(conn, npc_names) != expected["grants"]:
                raise ValueError("Grant plan changed since preview")
        grants = plan_grants(conn, npc_names, args.apply)
    print(json.dumps({"applied": args.apply, "count": len(grants), "grants": grants}))


if __name__ == "__main__":
    main()
