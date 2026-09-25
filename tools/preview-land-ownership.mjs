import fs from "node:fs/promises";

const snapshot = new URL(
  "../data/land-ownership-preview.json",
  import.meta.url,
);
const source = "https://openmmo.to.nexus/api/terrain/land-ownership";

async function main() {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === "--clear") {
    await fs.rm(snapshot, { force: true });
    console.log(
      "Land ownership preview cleared. Reopen the map to use local ownership.",
    );
    return;
  }
  if (args.length) {
    throw new Error("Usage: node tools/preview-land-ownership.mjs [--clear]");
  }

  const response = await fetch(source, {
    signal: AbortSignal.timeout(30_000),
    cache: "no-store",
  });
  if (!response.ok)
    throw new Error(`Ownership request failed: HTTP ${response.status}`);
  const plots = await response.json();
  if (
    !Array.isArray(plots) ||
    plots.some(
      (plot) =>
        !plot ||
        !Number.isInteger(plot.rx) ||
        plot.rx < -16 ||
        plot.rx > 15 ||
        !Number.isInteger(plot.rz) ||
        plot.rz < -16 ||
        plot.rz > 15 ||
        !Number.isInteger(plot.index) ||
        plot.index < 0 ||
        plot.index >= 1024 ||
        typeof plot.ownerName !== "string" ||
        !plot.ownerName,
    )
  ) {
    throw new Error(
      "Invalid land ownership response; the existing snapshot was preserved.",
    );
  }
  const addresses = new Set(
    plots.map((plot) => `${plot.rx},${plot.rz},${plot.index}`),
  );
  if (addresses.size !== plots.length)
    throw new Error("Duplicate plots in ownership response.");

  const temporary = new URL(`${snapshot.href}.${process.pid}.tmp`);
  try {
    await fs.writeFile(temporary, JSON.stringify(plots) + "\n", { flag: "wx" });
    await fs.rename(temporary, snapshot);
  } finally {
    await fs.rm(temporary, { force: true });
  }
  const owners = new Set(plots.map((plot) => plot.ownerName)).size;
  console.log(
    `Saved ${plots.length} plots from ${owners} owners to ${snapshot.pathname}`,
  );
  console.log(
    "Reopen the development map and enable Toggle Land Plots to preview.",
  );
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
