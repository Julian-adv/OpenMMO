"""Build client/public/sounds/merger-bells-<n>.ogg, one per recorded single
bell strike (doc/assets/sfx.md): a slow two-bell peal whose lower bell is
resampled from the same strike. The client layers the peals at random.

Usage: gen-merger-bells.py <strike.mp3|wav>...
"""

import subprocess
import sys
import tempfile
import wave
from pathlib import Path

import numpy as np

RATE = 44100
INTERVAL = 1.35
STRIKES = 6
LOWER_SEMITONES = 4
TAIL = 4.5


def load(path: Path) -> np.ndarray:
    raw = subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-i", str(path), "-ac", "1",
         "-ar", str(RATE), "-f", "s16le", "-"],
        check=True, capture_output=True,
    ).stdout
    return np.frombuffer(raw, dtype="<i2").astype(float) / 32768


def peal(source: Path) -> np.ndarray:
    high = load(source)
    start = max(0, int(np.argmax(np.abs(high) > 0.05 * np.abs(high).max())) - 64)
    high = high[start:]
    stretch = 2 ** (LOWER_SEMITONES / 12)
    low = np.interp(np.arange(0, len(high), 1 / stretch), np.arange(len(high)), high)
    mix = np.zeros(int(RATE * (INTERVAL * (STRIKES - 1) + TAIL)))
    for i in range(STRIKES):
        bell = high if i % 2 == 0 else low
        at = int(i * INTERVAL * RATE)
        ring = bell[: len(mix) - at]
        mix[at : at + len(ring)] += ring * (1.0 if i % 2 == 0 else 0.9)
    return mix / np.max(np.abs(mix))


def main() -> None:
    sounds = Path(__file__).resolve().parent.parent / "client/public/sounds"
    for n, source in enumerate(sys.argv[1:], start=1):
        mix = peal(Path(source))
        fade = int(RATE * 1.2)
        mix[-fade:] *= np.linspace(1.0, 0.0, fade) ** 2
        mix *= 0.7
        wav = Path(tempfile.mkdtemp()) / "merger-bells.wav"
        with wave.open(str(wav), "wb") as out:
            out.setnchannels(1)
            out.setsampwidth(2)
            out.setframerate(RATE)
            out.writeframes((mix * 32767).astype("<i2").tobytes())
        target = sounds / f"merger-bells-{n}.ogg"
        subprocess.run(
            ["ffmpeg", "-y", "-loglevel", "error", "-i", str(wav),
             "-af", "aecho=0.8:0.5:110|230:0.2|0.12,loudnorm=I=-18:TP=-2:LRA=11",
             "-ar", str(RATE), "-ac", "1", "-c:a", "libvorbis", "-q:a", "5", str(target)],
            check=True,
        )
        print(target)


if __name__ == "__main__":
    main()
