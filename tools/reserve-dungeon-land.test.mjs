import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { planReservation } from "./reserve-dungeon-land.mjs";

test("reserves edited plots while preserving unrelated edits and existing reserved land", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "dungeon-land-"));
  try {
    fs.mkdirSync(path.join(dir, "land-grades"));
    const before = Buffer.alloc(1024, 1);
    before[9] = 0;
    before[1000] = 2;
    const file = path.join(dir, "land-grades/r+00_+00.bin");
    fs.writeFileSync(file, before);
    const [change] = planReservation(dir, { x: 0, z: 0 }, []);
    assert.equal(change.after[0], 0);
    assert.equal(change.after[8], 2);
    assert.equal(change.after[9], 0);
    assert.equal(change.after[1000], 2);
    assert.deepEqual(fs.readFileSync(file), before);
    fs.writeFileSync(file, change.after);
    assert.deepEqual(planReservation(dir, { x: 0, z: 0 }, []), []);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("refuses an owned plot even when only its edge intersects the ring", () => {
  assert.throws(
    () =>
      planReservation("/unused", { x: 0, z: 0 }, [
        { tile_x: 2, tile_z: 0, quadrant: 1 },
      ]),
    /Owned plot overlaps/,
  );
  assert.throws(
    () =>
      planReservation("/unused", { x: 16370, z: 0 }, [
        { tile_x: -256, tile_z: 0, quadrant: 1 },
      ]),
    /Owned plot overlaps/,
  );
});
