"""Make the placeholder sounds for every minigame and the menus: short synthesised beeps, chimes
and thumps.

Each sound is built from sine tones and noise, shaped by a quick fade in and a decay, and saved as
a 16-bit mono WAV file next to the minigame's art (assets/minigames/<minigame>/), or in
assets/menu/. The engine sound loops, so it holds a whole number of cycles of every tone in it.
Music isn't made here. Run from the repository root:

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
NYQUIST: Final[float] = RATE / 2.0
# Every sound fades in this fast, so it starts without a click.
ATTACK: Final[float] = 0.004
# Keeps sounds that add up several tones below full scale.
HEADROOM: Final[float] = 0.8

ASSETS: Final[Path] = Path("assets/minigames")
MENU_ASSETS: Final[Path] = Path("assets/menu")

# Note frequencies, in Hz.
C5, D5, E5, G5, A5 = 523.25, 587.33, 659.25, 783.99, 880.00
C6, D6, E6 = 1046.50, 1174.66, 1318.51
G4 = 392.00


def silence(seconds: float) -> Signal:
    return [0.0] * max(0, round(seconds * RATE))


def tone(
    frequency: float,
    seconds: float,
    decay: float,
    harmonics: Sequence[float] = (1.0,),
    glide: float = 1.0,
    bright_decay: float | None = None,
) -> Signal:
    """A note that fades in over ATTACK and dies away, falling to 1/e every `decay` seconds.

    `harmonics` are the strengths of the 1st, 2nd, 3rd... partials, so (1.0,) is a pure sine and
    more of them sound brighter. If `bright_decay` is set, upper partials decay faster than the
    fundamental for a crisper attack without harshness. The pitch slides from `frequency` to
    `frequency * glide`.
    """
    samples = max(0, round(seconds * RATE))
    if samples == 0:
        return []

    active_harmonics = [
        (n, strength)
        for n, strength in enumerate(harmonics, 1)
        if strength != 0.0 and max(frequency, frequency * glide) * n < NYQUIST
    ]
    if not active_harmonics:
        active_harmonics = [(1, 1.0)]

    total = sum(abs(strength) for _, strength in active_harmonics)
    high_decay = bright_decay if bright_decay is not None else decay
    phase = 0.0
    out: Signal = []

    for i in range(samples):
        t = i / RATE
        progress = t / seconds if seconds > 0.0 else 1.0
        pitch = frequency * (glide**progress)
        phase += 2.0 * math.pi * pitch / RATE

        base_env = envelope(t, seconds, decay)
        high_env = envelope(t, seconds, high_decay)

        wave_value = 0.0
        for n, strength in active_harmonics:
            env = base_env if n == 1 else high_env
            wave_value += strength * env * math.sin(n * phase)

        out.append(wave_value / total)

    return out


def noise(seconds: float, decay: float, smoothing: float, seed: int) -> Signal:
    """A burst of noise that dies away like `tone`. Higher `smoothing` (0 to 1) muffles it."""
    samples = max(0, round(seconds * RATE))
    if samples == 0:
        return []

    clamped_smoothing = max(0.0, min(0.999, smoothing))
    rng = random.Random(seed)
    level = 0.0
    out: Signal = []
    for i in range(samples):
        t = i / RATE
        level = clamped_smoothing * level + (1.0 - clamped_smoothing) * rng.uniform(-1.0, 1.0)
        out.append(level * envelope(t, seconds, decay))
    return out


def envelope(t: float, seconds: float, decay: float) -> float:
    """Fade in over ATTACK, die away with `decay`, and reach 0 exactly at `seconds`."""
    if seconds <= 0.0:
        return 0.0
    fade_in = min(1.0, max(0.0, t / ATTACK)) if ATTACK > 0.0 else 1.0
    fade_out = min(1.0, max(0.0, (seconds - t) / ATTACK)) if ATTACK > 0.0 else 1.0
    decay_factor = math.exp(-t / decay) if decay > 0.0 else 1.0
    return fade_in * fade_out * decay_factor


def mix(*signals: Signal) -> Signal:
    if not signals:
        return []
    max_len = max((len(signal) for signal in signals), default=0)
    out = [0.0] * max_len
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
    peak = max((abs(value) for value in signal), default=0.0)
    gain = HEADROOM / peak if peak > HEADROOM else 1.0
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as file:
        file.setnchannels(1)
        file.setsampwidth(2)
        file.setframerate(RATE)
        frames = bytearray()
        for value in signal:
            sample = round(value * gain * 32767.0)
            clamped = max(-32768, min(32767, sample))
            frames.extend(struct.pack("<h", clamped))
        file.writeframes(bytes(frames))
    print(f"{path}: {len(signal) / RATE:.2f} s")


BRIGHT: Final[tuple[float, ...]] = (1.0, 0.5, 0.3, 0.15)
SOFT: Final[tuple[float, ...]] = (1.0, 0.25)
WARM_CHORD: Final[tuple[float, ...]] = (1.0, 0.35, 0.12)


def pong_sounds() -> dict[str, Signal]:
    # The game plays each paddle hit and wall bounce a little higher or lower, so a rally doesn't
    # repeat one sound: one file of each is enough.
    return {
        # Short crisp knock with quick high-overtone decay and slight pitch drop.
        "paddle_hit": tone(A5, 0.09, 0.03, BRIGHT, glide=0.85, bright_decay=0.012),
        # Softer and lower than a paddle hit.
        "wall_bounce": scaled(
            0.7,
            tone(D5 / 2.0, 0.07, 0.025, SOFT, glide=0.9, bright_decay=0.01),
        ),
        # Two rising notes.
        "goal": mix(
            tone(E5, 0.18, 0.08, BRIGHT, bright_decay=0.035),
            at(0.12, tone(A5, 0.35, 0.12, BRIGHT, bright_decay=0.05)),
        ),
    }


def racing_sounds() -> dict[str, Signal]:
    sounds: dict[str, Signal] = {
        # Two quick rising notes when your car completes a lap.
        "lap": mix(
            tone(A5, 0.12, 0.06, SOFT, bright_decay=0.025),
            at(0.08, tone(E6, 0.22, 0.09, SOFT, bright_decay=0.035)),
        ),
        # A rising arpeggio when your car crosses the line on its last lap.
        "finish": mix(
            *(at(0.09 * i, tone(note, 0.18, 0.08, BRIGHT, bright_decay=0.035)) for i, note in enumerate((C5, E5, G5))),
            at(0.27, tone(C6, 0.6, 0.3, BRIGHT, bright_decay=0.11)),
        ),
        # Muffled thud: low-passed noise over a falling low tone. Like Pong's bounces, the game
        # varies its pitch each time.
        "bump": mix(
            noise(0.16, 0.04, smoothing=0.88, seed=12),
            scaled(0.8, tone(90.0, 0.16, 0.05, SOFT, glide=0.6, bright_decay=0.02)),
        ),
        "engine": engine(),
    }
    return sounds


def engine() -> Signal:
    """A warmer looping engine hum.

    Uses a gentler harmonic roll-off with a subtle low-mid resonance and an asymmetric firing
    pulse so it stays warm rather than buzzy when sped up. Because `seconds = 0.5` and
    `base_freq = 60.0` Hz, every harmonic and pulse cycle completes an exact integer number of
    cycles within the loop.
    """
    base_freq = 60.0
    seconds = 0.5
    pulse_freq = base_freq / 2.0  # 30 Hz cylinder firing cadence
    samples = max(0, round(seconds * RATE))

    # Shape harmonics with a low-mid body emphasis (around 110-250 Hz) and smooth roll-off.
    weights: list[tuple[int, float]] = []
    for n in range(1, 17):
        freq = base_freq * n
        if freq >= NYQUIST:
            break
        roll_off = 1.0 / (n**1.45)
        body_boost = 1.35 if 110.0 <= freq <= 250.0 else 1.0
        weights.append((n, roll_off * body_boost))

    norm = sum(w for _, w in weights) or 1.0
    out: Signal = []

    for i in range(samples):
        t = i / RATE
        harm_sum = sum(w * math.sin(2.0 * math.pi * base_freq * n * t) for n, w in weights) / norm

        # Asymmetric pulse: slightly sharper compression/firing stroke, smoother tail.
        pulse_phase = (pulse_freq * t) % 1.0
        firing = 0.78 + 0.22 * (math.sin(2.0 * math.pi * pulse_phase) ** 3)

        out.append(0.65 * harm_sum * firing)

    return out


def shootout_sounds() -> dict[str, Signal]:
    return {
        # Punchy pneumatic paintball pop: brief noise puff over a quick falling tone.
        "shot": mix(
            scaled(0.55, noise(0.06, 0.018, smoothing=0.6, seed=21)),
            tone(380.0, 0.08, 0.025, SOFT, glide=0.45, bright_decay=0.01),
        ),
        # Soft paint splat against a wall or cover block.
        "wall_hit": scaled(
            0.65,
            mix(
                noise(0.07, 0.02, smoothing=0.78, seed=22),
                scaled(0.5, tone(180.0, 0.06, 0.02, SOFT, glide=0.6)),
            ),
        ),
        # Crisp splat/thwack when a gunner takes a hit.
        "hit": mix(
            noise(0.09, 0.025, smoothing=0.68, seed=23),
            scaled(0.8, tone(260.0, 0.1, 0.035, BRIGHT, glide=0.65, bright_decay=0.015)),
        ),
        # Heavier two-step descending thump when a gunner is knocked out.
        "knockout": mix(
            noise(0.18, 0.05, smoothing=0.84, seed=24),
            tone(330.0, 0.16, 0.06, BRIGHT, glide=0.55, bright_decay=0.025),
            at(0.11, tone(185.0, 0.3, 0.11, SOFT, glide=0.5, bright_decay=0.04)),
        ),
        # Bright rising chime when someone grabs a centre pickup.
        "pickup": mix(
            tone(E5, 0.12, 0.05, BRIGHT, bright_decay=0.025),
            at(0.07, tone(C6, 0.24, 0.09, BRIGHT, bright_decay=0.04)),
        ),
        # Urgent two-tone warning chime when the sudden-death border starts closing in.
        "sudden_death": mix(
            tone(A5, 0.16, 0.08, BRIGHT, bright_decay=0.03),
            at(0.15, tone(E5, 0.16, 0.08, BRIGHT, bright_decay=0.03)),
            at(0.30, tone(A5, 0.32, 0.14, BRIGHT, bright_decay=0.05)),
        ),
    }


def minigame_sounds() -> dict[str, Signal]:
    return {
        # The countdown before a minigame starts: one beep per second, then a higher, longer one
        # at the start.
        "countdown": tone(D5, 0.2, 0.15, BRIGHT, bright_decay=0.06),
        "go": tone(D6, 0.6, 0.4, BRIGHT, bright_decay=0.14),
        # When a minigame is won: up through a chord, then a warmer chord together.
        "minigame_won": then(
            mix(
                *(
                    at(0.1 * i, tone(note, 0.16, 0.08, BRIGHT, bright_decay=0.035))
                    for i, note in enumerate((G4, C5, E5))
                ),
            ),
            mix(
                *(tone(note, 0.8, 0.35, WARM_CHORD, bright_decay=0.12) for note in (C5, E5, G5, C6)),
            ),
        ),
    }


def menu_sounds() -> dict[str, Signal]:
    return {
        # Played at the new volume when the player changes it on the Sound screen.
        "volume_check": tone(A5, 0.15, 0.06, SOFT, bright_decay=0.025),
    }


SOUNDS: Final[dict[Path, Callable[[], dict[str, Signal]]]] = {
    ASSETS / "two_player_pong": pong_sounds,
    ASSETS / "racing": racing_sounds,
    ASSETS / "shootout": shootout_sounds,
    ASSETS: minigame_sounds,
    MENU_ASSETS: menu_sounds,
}


def main() -> None:
    for folder, make in SOUNDS.items():
        for name, signal in make().items():
            save(folder / f"{name}.wav", signal)


if __name__ == "__main__":
    main()
