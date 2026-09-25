import fs from "node:fs";
import {
  initSync,
  weather_rain_at,
  weather_set_sectors,
} from "../client/src/lib/wasm/onlinerpg_shared.js";

const root = new URL("../", import.meta.url);
const read = (path) => fs.readFileSync(new URL(path, root));
const years = Number(process.argv[2] ?? 20);
if (!Number.isInteger(years) || years < 1) {
  throw new Error(
    "Usage: node tools/measure-weather.mjs [positive integer years]",
  );
}

initSync({ module: read("client/src/lib/wasm/onlinerpg_shared_bg.wasm") });
const sectorsJson = read("data/terrain/weather-sectors.json").toString();
const { seed } = JSON.parse(sectorsJson);
const { x, z } = JSON.parse(read("data-src/world.json")).spawnPosition;
weather_set_sectors(sectorsJson);

const stepGameMinutes = 5;
const rainThreshold = 0.01;
const bias = 1;
const seasons = ["winter", "spring", "summer", "autumn"].map((name) => ({
  name,
  wet: 0,
  total: 0,
}));
for (let t = 0; t < years * 360 * 1440; t += stepGameMinutes) {
  const season = seasons[Math.floor(((t / 1440 + 1) % 360) / 90)];
  season.total++;
  if (weather_rain_at(seed, bias, t, x, z) > rainThreshold) season.wet++;
}

const total = seasons.reduce((sum, s) => sum + s.total, 0);
const wet = seasons.reduce((sum, s) => sum + s.wet, 0);
console.log(
  JSON.stringify(
    {
      years,
      seed,
      bias,
      position: { x, z },
      stepGameMinutes,
      rainThreshold,
      seasons: seasons.map(({ name, wet, total }) => ({
        name,
        wetPercent: (100 * wet) / total,
      })),
      annualWetPercent: (100 * wet) / total,
    },
    null,
    2,
  ),
);
