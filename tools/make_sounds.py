"""Make the placeholder sounds for every minigame: short synthesised beeps, chimes and thumps.

Each sound is built from sine tones and noise, shaped by a quick fade in and a decay, and saved as
a 16-bit mono WAV file next to the minigame's art (assets/minigames/<minigame>/). The engine sound
loops, so it holds a whole number of cycles of every tone in it. Run from the repository root:

    python3 tools/make_sounds.py

Needs nothing outside Python's standard library. The noise is seeded, so the files come out the
same each time.
"""

import math
import random
import struct
import wave
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Final, TypeAlias

Signal: TypeAlias = list[float]

RATE: Final[int] = 44_100
# Every sound fades in this fast, so it starts without a click.
ATTACK: Final[float] = 0.004
# Keeps sounds that add up several tones below full scale.
HEADROOM: Final[float] = 0.8

ASSETS: Final[Path] = Path("assets/minigames")

# Note frequencies, in Hz.
C5, D5, E5, G5, A5 = 523.25, 587.33, 659.25, 783.99, 880.00
C6, D6, E6 = 1046.50, 1174.66, 1318.51
G4 = 392.00


def silence(seconds: float) -> Signal:
    return [0.0] * round(seconds * RATE)


def tone(
    frequency: float,
    seconds: float,
    decay: float,
    harmonics: Sequence[float] = (1.0,),
    glide: float = 1.0,
) -> Signal:
    """A note that fades in over ATTACK and dies away, falling to 1/e every `decay` seconds.

    `harmonics` are the strengths of the 1st, 2nd, 3rd... partials, so (1.0,) is a pure sine and
    more of them sound brighter. The pitch slides from `frequency` to `frequency * glide`.
    """
    samples = round(seconds * RATE)
    total = sum(harmonics)
    phase = 0.0
    out = []
    for i in range(samples):
        t = i / RATE
        pitch = frequency * glide ** (t / seconds)
        phase += 2.0 * math.pi * pitch / RATE
        wave_value = sum(
            strength * math.sin(n * phase) for n, strength in enumerate(harmonics, 1)
        )
        out.append(wave_value / total * envelope(t, seconds, decay))
    return out


def noise(seconds: float, decay: float, smoothing: float, seed: int) -> Signal:
    """A burst of noise that dies away like `tone`. Higher `smoothing` (0 to 1) muffles it."""
    rng = random.Random(seed)
    level = 0.0
    out = []
    for i in range(round(seconds * RATE)):
        t = i / RATE
        level = smoothing * level + (1.0 - smoothing) * rng.uniform(-1.0, 1.0)
        out.append(level * envelope(t, seconds, decay))
    return out


def envelope(t: float, seconds: float, decay: float) -> float:
    """Fade in over ATTACK, die away with `decay`, and reach 0 exactly at `seconds`."""
    fade_in = min(1.0, t / ATTACK)
    fade_out = min(1.0, (seconds - t) / ATTACK)
    return fade_in * fade_out * math.exp(-t / decay)


def mix(*signals: Signal) -> Signal:
    out = [0.0] * max(len(signal) for signal in signals)
    for signal in signals:
        for i, value in enumerate(signal):
            out[i] += value
    return out


def then(*signals: Signal) -> Signal:
    """The signals one after another."""
    return [value for signal in signals for value in signal]


def at(seconds: float, signal: Signal) -> Signal:
    """`signal`, starting `seconds` in."""
    return silence(seconds) + signal


def scaled(volume: float, signal: Signal) -> Signal:
    return [volume * value for value in signal]


def save(path: Path, signal: Signal) -> None:
    peak = max(abs(value) for value in signal)
    gain = HEADROOM / peak if peak > HEADROOM else 1.0
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as file:
        file.setnchannels(1)
        file.setsampwidth(2)
        file.setframerate(RATE)
        file.writeframes(
            b"".join(struct.pack("<h", round(value * gain * 32767)) for value in signal)
        )
    print(f"{path}: {len(signal) / RATE:.2f} s")


BRIGHT: Final[tuple[float, ...]] = (1.0, 0.5, 0.3, 0.15)
SOFT: Final[tuple[float, ...]] = (1.0, 0.25)


def pong_sounds() -> dict[str, Signal]:
    return {
        # A short, bright knock that drops a little in pitch.
        "paddle_hit": tone(A5, 0.09, 0.025, BRIGHT, glide=0.85),
        # Softer and lower than a paddle hit.
        "wall_bounce": scaled(0.7, tone(D5 / 2, 0.07, 0.02, SOFT, glide=0.9)),
        # Two rising notes.
        "goal": mix(
            tone(E5, 0.18, 0.08, BRIGHT),
            at(0.12, tone(A5, 0.35, 0.12, BRIGHT)),
        ),
    }


def racing_sounds() -> dict[str, Signal]:
    return {
        # One beep per second of the countdown, then a higher, longer one at the start.
        "countdown": tone(D5, 0.2, 0.15, BRIGHT),
        "go": tone(D6, 0.6, 0.4, BRIGHT),
        # Two quick rising notes when your car completes a lap.
        "lap": mix(tone(A5, 0.12, 0.06, SOFT), at(0.08, tone(E6, 0.22, 0.09, SOFT))),
        # A rising arpeggio when your car crosses the line on its last lap.
        "finish": mix(
            *(
                at(0.09 * i, tone(note, 0.18, 0.08, BRIGHT))
                for i, note in enumerate((C5, E5, G5))
            ),
            at(0.27, tone(C6, 0.6, 0.3, BRIGHT)),
        ),
        # A muffled thud: low noise over a falling low tone.
        "bump": mix(
            noise(0.16, 0.04, smoothing=0.85, seed=1),
            scaled(0.8, tone(90.0, 0.16, 0.05, glide=0.6)),
        ),
        "engine": engine(),
    }


def engine() -> Signal:
    """A looping engine hum. The game plays it faster as the car speeds up, which raises its pitch.

    A low buzz (many harmonics, getting weaker), pulsing at half its frequency like an engine's
    cylinders firing. The clip holds a whole number of cycles of both, so the loop is
    seamless.
    """
    frequency = 60.0
    seconds = 0.5
    pulse = frequency / 2.0
    harmonics = [1.0 / n for n in range(1, 13)]
    total = sum(harmonics)
    out = []
    for i in range(round(seconds * RATE)):
        t = i / RATE
        buzz = sum(
            strength * math.sin(2.0 * math.pi * frequency * n * t)
            for n, strength in enumerate(harmonics, 1)
        )
        throb = 0.75 + 0.25 * math.sin(2.0 * math.pi * pulse * t)
        out.append(0.6 * buzz / total * throb)
    return out


def minigame_sounds() -> dict[str, Signal]:
    return {
        # When a minigame is won: up through a chord, then the chord together.
        "minigame_won": then(
            mix(
                *(
                    at(0.1 * i, tone(note, 0.16, 0.08, BRIGHT))
                    for i, note in enumerate((G4, C5, E5))
                ),
            ),
            mix(*(tone(note, 0.8, 0.35, BRIGHT) for note in (C5, E5, G5, C6))),
        ),
    }


SOUNDS: Final[dict[Path, Callable[[], dict[str, Signal]]]] = {
    ASSETS / "two_player_pong": pong_sounds,
    ASSETS / "racing": racing_sounds,
    ASSETS: minigame_sounds,
}


def main() -> None:
    for folder, make in SOUNDS.items():
        for name, signal in make().items():
            save(folder / f"{name}.wav", signal)


if __name__ == "__main__":
    main()
