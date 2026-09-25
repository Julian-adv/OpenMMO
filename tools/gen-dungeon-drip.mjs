import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const clips = [
  {
    source: "ocons_6gEs_tJpkIY_158.556-158.796.wav",
    output: "dungeon-drip.ogg",
    duration: 0.24,
    fadeOut: 0.04,
  },
  {
    source: "ocons_6gEs_tJpkIY_68.932-69.332.wav",
    output: "dungeon-drip-2.ogg",
    duration: 0.4,
    fadeOut: 0.08,
  },
];

function ffmpeg(args, input) {
  const result = spawnSync("ffmpeg", ["-v", "error", ...args], { input });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr.toString());
  return result.stdout;
}

const sampleRate = 44100;
for (const clip of process.argv[2] ? clips.slice(0, 1) : clips) {
  const source =
    process.argv[2] ??
    fileURLToPath(
      new URL(
        `../assets/sfx/dungeon-drip-ocons-2026-09-18/${clip.source}`,
        import.meta.url,
      ),
    );
  const output = fileURLToPath(
    new URL(`../client/public/sounds/${clip.output}`, import.meta.url),
  );
  const fadeStart = (clip.duration - clip.fadeOut).toFixed(3);
  const pcm = ffmpeg([
    "-i",
    source,
    "-ac",
    "1",
    "-ar",
    String(sampleRate),
    "-af",
    `atrim=start=0:duration=${clip.duration},asetpts=PTS-STARTPTS,afade=t=in:d=0.001,afade=t=out:st=${fadeStart}:d=${clip.fadeOut}`,
    "-f",
    "f32le",
    "pipe:1",
  ]);
  const sampleCount = pcm.length / 4;
  const measuredSamples = Math.min(sampleCount, Math.round(sampleRate * 0.12));
  let peak = 0;
  let energy = 0;
  for (let i = 0; i < sampleCount; i++) {
    const sample = pcm.readFloatLE(i * 4);
    peak = Math.max(peak, Math.abs(sample));
    if (i < measuredSamples) energy += sample * sample;
  }
  if (peak === 0) throw new Error(`The selected drip is silent: ${source}`);
  const rms = Math.sqrt(energy / measuredSamples);
  const gain = Math.min(10 ** (-21 / 20) / rms, 0.75 / peak);
  for (let i = 0; i < pcm.length; i += 4)
    pcm.writeFloatLE(pcm.readFloatLE(i) * gain, i);

  ffmpeg(
    [
      "-y",
      "-f",
      "f32le",
      "-ar",
      String(sampleRate),
      "-ac",
      "1",
      "-i",
      "pipe:0",
      "-c:a",
      "libvorbis",
      "-q:a",
      "5",
      output,
    ],
    pcm,
  );
  console.log(output);
}
