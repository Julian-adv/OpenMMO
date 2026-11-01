import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def monster(monster_id):
    return json.loads((ROOT / 'data/monsters.json').read_text())[monster_id]


def weapon_models(monster_id):
    items = json.loads((ROOT / 'data/items.json').read_text())
    item = items[monster(monster_id)['weapon']]
    return list(dict.fromkeys(row['worldModel'] for row in items.values() if row.get('weaponType') == item['weaponType'] and row.get('worldModel')))


def primary_weapon(monster_id):
    items = json.loads((ROOT / 'data/items.json').read_text())
    return items[monster(monster_id)['weapon']]['worldModel']


def attachment(monster_id):
    row = monster(monster_id)
    offset, x, z = (float(row[key]) for key in ('weaponOffset', 'weaponOffsetX', 'weaponOffsetZ'))
    rotation = [math.radians(float(value)) for value in row['weaponRotation'].split('|')]
    assert len(rotation) == 3
    return [x, offset, z], rotation
