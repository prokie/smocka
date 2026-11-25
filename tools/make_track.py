"""Draw assets/minigames/racing/track.png to match the track in config::minigames::racing.

The constants below must match the ones in src/config.rs. When you change the track's shape
there, change them here too and run, from the repository root:

    python3 tools/make_track.py

Then press F3 in a race to check that the road edges line up with the art. Needs Pillow.
"""

import math
import sys
from collections.abc import Iterator
from typing import Final, TypeAlias

from PIL import Image

RGBA: TypeAlias = tuple[int, int, int, int]

# Keep in step with config::minigames::racing.
HALF_LENGTH: Final[float] = 140.0  # TRACK_HALF_LENGTH
INNER: Final[float] = 100.0  # TRACK_INNER_RADIUS
OUTER: Final[float] = 160.0  # TRACK_OUTER_RADIUS
START_LINE_X: Final[float] = 0.0  # START_LINE_X
GRID_ROW: Final[float] = 24.0  # GRID_ROW_SPACING
GRID_LANE: Final[float] = 14.0  # GRID_LANE_OFFSET

W: Final[int] = 640  # config::display::VIEW_WIDTH
H: Final[int] = 360  # config::display::VIEW_HEIGHT
BOTTOM_Y: Final[float] = -(INNER + OUTER) / 2
KERB: Final[float] = 3.0  # kerb width, world units
KERB_SEGMENT: Final[float] = 8.0  # length of each red/white piece

GRASS_A: Final[RGBA] = (52, 110, 58, 255)
GRASS_B: Final[RGBA] = (60, 122, 64, 255)
ROAD: Final[RGBA] = (74, 74, 82, 255)
ROAD_SPECK: Final[RGBA] = (82, 82, 90, 255)
KERB_RED: Final[RGBA] = (196, 56, 56, 255)
WHITE: Final[RGBA] = (232, 232, 232, 255)
BLACK: Final[RGBA] = (24, 24, 28, 255)

DEFAULT_OUTPUT: Final[str] = "assets/minigames/racing/track.png"


def centre_dist(x: float, y: float) -> float:
    """Distance from (x, y) to the track's centre line, as track::surface_at measures it."""
    nx = max(-HALF_LENGTH, min(HALF_LENGTH, x))
    return math.hypot(x - nx, y)


def along(x: float, y: float, radius: float) -> float:
    """Distance along the edge at `radius`, for alternating kerb colours."""
    if abs(x) <= HALF_LENGTH:
        return x if y < 0 else -x
    cx = HALF_LENGTH if x > 0 else -HALF_LENGTH
    return math.atan2(y, x - cx) * radius


def grid_slots() -> Iterator[tuple[float, float]]:
    """The same positions as track::grid_position, for 8 cars."""
    for slot in range(8):
        row, outside = slot // 2, slot % 2 == 1
        x = START_LINE_X - GRID_ROW * (row + 1) - (GRID_ROW / 2 if outside else 0)
        yield x, BOTTOM_Y + (-1 if outside else 1) * GRID_LANE


def world(px: int, py: int) -> tuple[float, float]:
    """World coordinates of the centre of pixel (px, py). World y points up."""
    return px - W / 2 + 0.5, H / 2 - py - 0.5


def main(output: str) -> None:
    img = Image.new("RGBA", (W, H))
    pixels = img.load()
    assert pixels is not None

    for py in range(H):
        for px in range(W):
            x, y = world(px, py)
            d = centre_dist(x, y)
            if INNER <= d <= OUTER:
                if d < INNER + KERB or d > OUTER - KERB:
                    r = INNER if d < INNER + KERB else OUTER
                    red = math.floor(along(x, y, r) / KERB_SEGMENT) % 2
                    c = KERB_RED if red else WHITE
                else:
                    c = ROAD_SPECK if (px * 7 + py * 13) % 23 == 0 else ROAD
            else:
                c = GRASS_A if (px // 16) % 2 else GRASS_B
            pixels[px, py] = c

    # Start line: a 2-square-wide chequered strip across the road.
    for py in range(H):
        for px in range(W):
            x, y = world(px, py)
            d = centre_dist(x, y)
            on_road = INNER + KERB <= d <= OUTER - KERB
            if y < 0 and on_road and START_LINE_X - 4 <= x < START_LINE_X + 4:
                square = (math.floor(x / 4) + math.floor(y / 4)) % 2
                pixels[px, py] = WHITE if square else BLACK

    # Grid slot marks: a short white bracket just in front of each car's nose.
    for gx, gy in grid_slots():
        nose = gx + 10
        for wy in range(int(gy - 7), int(gy + 7)):
            pixels[int(nose + W / 2), int(H / 2 - wy - 1)] = WHITE
        for wx in range(int(nose - 4), int(nose)):
            for edge_y in (gy - 7, gy + 6):
                pixels[int(wx + W / 2), int(H / 2 - edge_y - 1)] = WHITE

    img.save(output)
    print(f"Wrote {output}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else DEFAULT_OUTPUT)
