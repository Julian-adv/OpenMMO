import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const worldDelta = (from, to) => ((to - from + 49152) % 32768) - 16384;
const regionName = (value) =>
  `${value < 0 ? "-" : "+"}${Math.abs(value).toString().padStart(2, "0")}`;

function readCsv(file) {
  const [header, ...lines] = fs.readFileSync(file, "utf8").trim().split("\n");
  const fields = header.split(",");
  return lines.map((line) =>
    Object.fromEntries(line.split(",").map((value, i) => [fields[i], value])),
  );
}

export function planReservation(terrainDir, dungeon, plots) {
  const x = Number(dungeon.x);
  const z = Number(dungeon.z);
  if (!Number.isFinite(x) || !Number.isFinite(z))
    throw new Error("Invalid dungeon coordinates");
  for (const plot of plots) {
    const { tile_x: tx, tile_z: tz, quadrant: q } = plot;
    if (![tx, tz, q].every(Number.isInteger) || q < 0 || q > 3)
      throw new Error("Invalid ownership row");
    const cx = tx * 64 - 16 + (q % 2) * 32;
    const cz = tz * 64 - 16 + Math.floor(q / 2) * 32;
    const distance = Math.hypot(
      Math.max(Math.abs(worldDelta(x, cx)) - 16, 0),
      Math.max(Math.abs(cz - z) - 16, 0),
    );
    if (distance <= 150)
      throw new Error(`Owned plot overlaps the 150m ring: ${tx}, ${tz}, ${q}`);
  }
  const changes = [];
  for (
    let rx = Math.floor((x - 150 + 32) / 1024);
    rx <= Math.floor((x + 150 + 32) / 1024);
    rx++
  ) {
    for (
      let rz = Math.floor((z - 150 + 32) / 1024);
      rz <= Math.floor((z + 150 + 32) / 1024);
      rz++
    ) {
      const canonicalRx = ((rx + 48) % 32) - 16;
      const file = path.join(
        terrainDir,
        "land-grades",
        `r${regionName(canonicalRx)}_${regionName(rz)}.bin`,
      );
      if (!fs.existsSync(file)) continue;
      const before = fs.readFileSync(file);
      if (before.length !== 1024 || before.some((grade) => grade > 2))
        throw new Error(`Invalid land grades: ${file}`);
      const after = Buffer.from(before);
      let count = 0;
      for (let index = 0; index < 1024; index++) {
        const tile = Math.floor(index / 4);
        const q = index % 4;
        const cx = (canonicalRx * 16 + (tile % 16)) * 64 - 16 + (q % 2) * 32;
        const cz =
          (rz * 16 + Math.floor(tile / 16)) * 64 - 16 + Math.floor(q / 2) * 32;
        const distance = Math.hypot(worldDelta(x, cx), cz - z);
        if (distance > 150) continue;
        const grade = distance < 60 || before[index] === 0 ? 0 : 2;
        if (before[index] !== grade) {
          after[index] = grade;
          count++;
        }
      }
      if (count) changes.push({ file, before, after, count });
    }
  }
  return changes;
}

function main() {
  const { values } = parseArgs({
    options: {
      dungeon: { type: "string" },
      ownership: { type: "string" },
      terrain: { type: "string", default: path.join(repo, "data/terrain") },
      apply: { type: "boolean", default: false },
    },
  });
  if (!values.dungeon || !values.ownership)
    throw new Error(
      "Use --dungeon ID --ownership owned-plots.json [--terrain DIR] [--apply]",
    );
  const dungeon = readCsv(path.join(repo, "data-src/dungeons.csv")).find(
    (row) => row.id === values.dungeon,
  );
  if (!dungeon) throw new Error(`Unknown dungeon: ${values.dungeon}`);
  const plots = JSON.parse(fs.readFileSync(values.ownership, "utf8"));
  if (!Array.isArray(plots)) throw new Error("Ownership must be a JSON array");
  const changes = planReservation(values.terrain, dungeon, plots);
  for (const change of changes) {
    console.log(
      `${values.apply ? "Apply" : "Preview"} ${change.file}: ${change.count} plots`,
    );
    if (!values.apply) continue;
    if (!fs.readFileSync(change.file).equals(change.before))
      throw new Error(`Land grades changed: ${change.file}`);
    fs.copyFileSync(
      change.file,
      `${change.file}.before-${dungeon.id}-${Date.now()}`,
      fs.constants.COPYFILE_EXCL,
    );
    const temporary = `${change.file}.${process.pid}.tmp`;
    try {
      fs.writeFileSync(temporary, change.after, { flag: "wx" });
      fs.renameSync(temporary, change.file);
    } finally {
      fs.rmSync(temporary, { force: true });
    }
  }
  console.log(
    "Regions without edited grade files use the dungeon registry defaults.",
  );
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
)
  main();
