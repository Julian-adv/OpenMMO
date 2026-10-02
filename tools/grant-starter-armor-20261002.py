import argparse
import json
import sqlite3
from pathlib import Path


STARTER_ARMOR = {
    "knight": (
        "worn_plate_helmet",
        "worn_breastplate",
        "worn_plate_greaves",
        "worn_plate_boots",
        "worn_plate_gauntlets",
    ),
    "barbarian": (
        "worn_barbarian_helmet",
        "worn_barbarian_armor",
        "worn_barbarian_pants",
        "worn_barbarian_boots",
        "worn_barbarian_bracers",
    ),
    "rogue": (
        "worn_rogue_top",
        "worn_rogue_pants",
        "worn_rogue_gloves",
        "worn_rogue_boots",
    ),
}


def grant_armor(conn, npc_names, apply=False):
    conn.execute("BEGIN IMMEDIATE")
    grants = []
    try:
        characters = conn.execute(
            "SELECT id, account_name, character_name, class FROM characters "
            "WHERE gender = 'male' AND class IN ('knight', 'barbarian', 'rogue') "
            "ORDER BY id"
        ).fetchall()
        for character_id, account, name, character_class in characters:
            if account.startswith("npc_") and name in npc_names:
                continue
            existing = {
                row[0]
                for row in conn.execute(
                    "SELECT item_def_id FROM character_items WHERE character_id = ?",
                    (character_id,),
                )
            }
            for item_id in STARTER_ARMOR[character_class]:
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
    parser = argparse.ArgumentParser(
        description="Grant male starter armor once with game and NPC services stopped."
    )
    parser.add_argument("db", type=Path)
    parser.add_argument("--apply", action="store_true", help="Write items; default is preview.")
    args = parser.parse_args()
    data = Path(__file__).resolve().parents[1] / "data"
    npc_names = {npc["npcName"] for npc in json.loads((data / "npcs.json").read_text()).values()}
    items = json.loads((data / "items.json").read_text())
    for item_ids in STARTER_ARMOR.values():
        for item_id in item_ids:
            item = items[item_id]
            if not item.get("untradeable") or item.get("basePrice") is not None:
                raise ValueError(f"Invalid starter armor definition: {item_id}")
    with sqlite3.connect(f"{args.db.resolve().as_uri()}?mode=rw", uri=True, timeout=10) as conn:
        conn.execute("PRAGMA foreign_keys = ON")
        grants = grant_armor(conn, npc_names, args.apply)
    print(json.dumps({"applied": args.apply, "count": len(grants), "grants": grants}))


if __name__ == "__main__":
    main()
