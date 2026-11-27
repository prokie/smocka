"""Draw the placeholder art for every minigame, in the game's flat style, for a 1080p screen, plus
each minigame's thumbnail for its tile in the lobby, and the menus' icons.

Everything is laid out in world units (the 640x360 virtual screen, y up, (0, 0) in the middle) and
drawn at ART_SCALE texture pixels per unit. Shapes are drawn SUPERSAMPLE times bigger still and
then shrunk, for smooth edges.

Shapes that gameplay depends on (the racing track, Pong's walls) use the same constants as each
minigame's config.rs (src/minigames/<minigame>/config.rs): keep them in step. Run from the repository root:

    python3 tools/make_art.py

Then press F3 in a race to check that the road edges line up with the art. Needs Pillow.
"""

import math
from collections.abc import Iterator
from pathlib import Path
from typing import Final, TypeAlias

from PIL import Image, ImageChops, ImageDraw

RGBA: TypeAlias = tuple[int, int, int, int]

ART_SCALE: Final[int] = 3  # config::display::ART_SCALE
SUPERSAMPLE: Final[int] = 4
VIEW_W: Final[float] = 640.0  # config::display::VIEW_WIDTH
VIEW_H: Final[float] = 360.0  # config::display::VIEW_HEIGHT

# Keep in step with minigames::racing::config.
HALF_LENGTH: Final[float] = 140.0  # TRACK_HALF_LENGTH
INNER: Final[float] = 100.0  # TRACK_INNER_RADIUS
OUTER: Final[float] = 160.0  # TRACK_OUTER_RADIUS
START_LINE_X: Final[float] = 0.0  # START_LINE_X
GRID_ROW: Final[float] = 24.0  # GRID_ROW_SPACING
GRID_LANE: Final[float] = 14.0  # GRID_LANE_OFFSET
CAR_LENGTH: Final[float] = 16.0  # CAR_LENGTH
CAR_WIDTH: Final[float] = 10.0  # CAR_WIDTH

# Keep in step with minigames::two_player_pong::config.
WALL_THICKNESS: Final[float] = 4.0  # WALL_THICKNESS
PADDLE_WIDTH: Final[float] = 8.0  # PADDLE_WIDTH
PADDLE_HEIGHT: Final[float] = 48.0  # PADDLE_HEIGHT
BALL_SIZE: Final[float] = 8.0  # BALL_SIZE

BOTTOM_Y: Final[float] = -(INNER + OUTER) / 2
KERB: Final[float] = 4.0  # kerb width, inside the road
KERB_SEGMENT: Final[float] = 10.0  # length of each red/white piece
EDGE: Final[float] = 2.0  # dark outline just outside the road

# Colours. Anything drawn white or light grey is tinted per player in the game.
OUTLINE: Final[RGBA] = (28, 30, 40, 255)
WHITE: Final[RGBA] = (255, 255, 255, 255)
LIGHT: Final[RGBA] = (214, 216, 224, 255)
GLASS: Final[RGBA] = (62, 72, 96, 255)
TYRE: Final[RGBA] = (30, 30, 36, 255)
GRASS_A: Final[RGBA] = (92, 176, 84, 255)
GRASS_B: Final[RGBA] = (102, 188, 92, 255)
GRASS_EDGE: Final[RGBA] = (52, 104, 56, 255)
ROAD: Final[RGBA] = (78, 80, 92, 255)
KERB_RED: Final[RGBA] = (226, 70, 70, 255)
KERB_WHITE: Final[RGBA] = (245, 245, 245, 255)
LINE_DARK: Final[RGBA] = (30, 30, 36, 255)
ARENA: Final[RGBA] = (24, 24, 32, 255)
ARENA_WALL: Final[RGBA] = (96, 100, 124, 255)
ARENA_MARK: Final[RGBA] = (52, 54, 70, 255)

ASSETS: Final[Path] = Path("assets/minigames")
MENU_ASSETS: Final[Path] = Path("assets/menu")

# A minigame's tile in the lobby shows a 16:9 thumbnail, made from its art at a quarter of 1080p.
THUMBNAIL_SIZE: Final[tuple[int, int]] = (480, 270)
# Keep in step with players::PLAYER_COLORS: the first four, to tint players' things in thumbnails.
PLAYER_TINTS: Final[list[tuple[float, float, float]]] = [
    (0.35, 0.65, 1.0),  # blue
    (1.0, 0.4, 0.4),  # red
    (0.45, 0.9, 0.45),  # green
    (1.0, 0.85, 0.3),  # yellow
]
PADDLE_X_OFFSET: Final[float] = 296.0  # minigames::two_player_pong::config::PADDLE_X_OFFSET
# Players' things are drawn this much bigger in thumbnails, which the lobby shows small, so they
# still read: true to size, a car would be 4 pixels long on its tile.
THUMBNAIL_SPRITE_SCALE: Final[float] = 2.5


class Canvas:
    """A `width` x `height` drawing area in world units, y up, with (0, 0) in the middle."""

    def __init__(self, width: float, height: float, background: RGBA = (0, 0, 0, 0)) -> None:
        self.width = width
        self.height = height
        self.scale = ART_SCALE * SUPERSAMPLE
        size = (round(width * self.scale), round(height * self.scale))
        self.image = Image.new("RGBA", size, background)
        self.draw = ImageDraw.Draw(self.image)

    def point(self, x: float, y: float) -> tuple[float, float]:
        return ((x + self.width / 2) * self.scale, (self.height / 2 - y) * self.scale)

    def box(self, x0: float, y0: float, x1: float, y1: float) -> list[float]:
        left, top = self.point(min(x0, x1), max(y0, y1))
        right, bottom = self.point(max(x0, x1), min(y0, y1))
        return [left, top, right, bottom]

    def rect(
        self,
        x0: float,
        y0: float,
        x1: float,
        y1: float,
        fill: RGBA | None,
        radius: float = 0.0,
        outline: RGBA | None = None,
        width: float = 0.0,
    ) -> None:
        self.draw.rounded_rectangle(
            self.box(x0, y0, x1, y1),
            radius=radius * self.scale,
            fill=fill,
            outline=outline,
            width=round(width * self.scale),
        )

    def stadium(self, radius: float, fill: RGBA) -> None:
        """Every point within `radius` of the track's centre line: the same shape as
        track::surface_at uses."""
        self.rect(-HALF_LENGTH - radius, -radius, HALF_LENGTH + radius, radius, fill, radius)

    def wedge(self, cx: float, cy: float, radius: float, a0: float, a1: float, fill: RGBA) -> None:
        """A slice of the circle around (cx, cy), from angle `a0` to `a1` (radians, anticlockwise)."""
        # Pillow measures angles in degrees, clockwise, with y down.
        self.draw.pieslice(
            self.box(cx - radius, cy - radius, cx + radius, cy + radius),
            -math.degrees(a1),
            -math.degrees(a0),
            fill=fill,
        )

    def line(self, x0: float, y0: float, x1: float, y1: float, fill: RGBA, width: float) -> None:
        self.draw.line(
            [self.point(x0, y0), self.point(x1, y1)], fill=fill, width=round(width * self.scale)
        )

    def ellipse(self, x0: float, y0: float, x1: float, y1: float, fill: RGBA, outline: RGBA, width: float) -> None:
        self.draw.ellipse(
            self.box(x0, y0, x1, y1), fill=fill, outline=outline, width=round(width * self.scale)
        )

    def save(self, path: Path) -> None:
        size = (round(self.width * ART_SCALE), round(self.height * ART_SCALE))
        self.image.resize(size, Image.Resampling.LANCZOS).save(path)
        print(f"Wrote {path} ({size[0]}x{size[1]})")


# Racing


def grid_slots() -> Iterator[tuple[float, float]]:
    """The same positions as track::grid_position, for 8 cars."""
    for slot in range(8):
        row, outside = slot // 2, slot % 2 == 1
        x = START_LINE_X - GRID_ROW * (row + 1) - (GRID_ROW / 2 if outside else 0)
        yield x, BOTTOM_Y + (-1 if outside else 1) * GRID_LANE


def kerb_ring(canvas: Canvas, radius: float, band: tuple[float, float]) -> None:
    """Alternating red and white pieces all the way round, out to `radius` from the centre line.
    `band` is the kerb's (inner, outer) distance from the centre line on the straights."""
    colours = (KERB_RED, KERB_WHITE)
    # Straights.
    count = math.ceil(2 * HALF_LENGTH / KERB_SEGMENT)
    for i in range(count):
        x0 = -HALF_LENGTH + i * KERB_SEGMENT
        x1 = min(x0 + KERB_SEGMENT, HALF_LENGTH)
        for sign in (1, -1):
            canvas.rect(x0, sign * band[0], x1, sign * band[1], colours[i % 2])
    # Bends: slices of the circle around each end of the centre line.
    step = KERB_SEGMENT / radius
    count = math.ceil(math.pi / step)
    for cx, start in ((HALF_LENGTH, -math.pi / 2), (-HALF_LENGTH, math.pi / 2)):
        for i in range(count):
            a0 = start + i * step
            a1 = min(a0 + step, start + math.pi)
            canvas.wedge(cx, 0, radius, a0, a1, colours[i % 2])


def draw_track() -> None:
    canvas = Canvas(VIEW_W, VIEW_H)

    # Grass with mowing stripes, kept to fill the infield again later.
    stripe = 20.0
    for i in range(math.ceil(VIEW_W / stripe)):
        x0 = -VIEW_W / 2 + i * stripe
        canvas.rect(x0, -VIEW_H / 2, x0 + stripe, VIEW_H / 2, (GRASS_A, GRASS_B)[i % 2])
    grass = canvas.image.copy()

    # Outside in: dark edge, outer kerb, road, inner kerb, dark edge, infield.
    canvas.stadium(OUTER + EDGE, GRASS_EDGE)
    kerb_ring(canvas, OUTER, (OUTER - KERB, OUTER))
    canvas.stadium(OUTER - KERB, ROAD)
    kerb_ring(canvas, INNER + KERB, (INNER, INNER + KERB))
    canvas.stadium(INNER, GRASS_EDGE)
    infield = Canvas(VIEW_W, VIEW_H)
    infield.stadium(INNER - EDGE, WHITE)
    canvas.image.paste(grass, (0, 0), infield.image)

    # Start line: a chequered strip across the road, two squares deep.
    square = 4.0
    y = -(OUTER - KERB)
    row = 0
    while y < -(INNER + KERB):
        top = min(y + square, -(INNER + KERB))
        for column in range(2):
            x0 = START_LINE_X - square + column * square
            colour = KERB_WHITE if (row + column) % 2 else LINE_DARK
            canvas.rect(x0, y, x0 + square, top, colour)
        y = top
        row += 1

    # Grid boxes: a white bracket in front of each starting slot.
    for gx, gy in grid_slots():
        nose = gx + CAR_LENGTH / 2 + 2
        half = CAR_WIDTH / 2 + 2
        canvas.line(nose, gy - half, nose, gy + half, KERB_WHITE, 1.2)
        for edge in (gy - half, gy + half):
            canvas.line(nose - 6, edge, nose, edge, KERB_WHITE, 1.2)

    canvas.save(ASSETS / "racing/track.png")


def draw_car() -> None:
    """Pointing right. White and light grey parts are tinted per player."""
    canvas = Canvas(CAR_LENGTH, CAR_WIDTH)
    half_l, half_w = CAR_LENGTH / 2, CAR_WIDTH / 2
    for x0, x1 in ((-5.6, -2.6), (2.4, 5.4)):
        for y0, y1 in ((half_w - 1.6, half_w), (-half_w, -half_w + 1.6)):
            canvas.rect(x0, y0, x1, y1, TYRE, radius=0.6)
    canvas.rect(
        -half_l + 0.4, -half_w + 0.8, half_l - 0.4, half_w - 0.8, WHITE,
        radius=2.6, outline=OUTLINE, width=0.7,
    )
    canvas.rect(-3.6, -0.8, 1.6, 0.8, LIGHT)  # roof stripe
    canvas.rect(1.6, -2.9, 3.9, 2.9, GLASS, radius=0.8)  # windscreen
    canvas.rect(-5.0, -2.5, -3.7, 2.5, GLASS, radius=0.5)  # rear window
    canvas.save(ASSETS / "racing/car.png")


# Pong


def draw_arena() -> None:
    canvas = Canvas(VIEW_W, VIEW_H, ARENA)
    for sign in (1, -1):
        canvas.rect(-VIEW_W / 2, sign * VIEW_H / 2, VIEW_W / 2, sign * (VIEW_H / 2 - WALL_THICKNESS), ARENA_WALL)
    # Centre line and circle.
    dash, gap = 12.0, 10.0
    y = VIEW_H / 2 - WALL_THICKNESS - gap / 2
    while y - dash > -VIEW_H / 2 + WALL_THICKNESS:
        canvas.rect(-1.5, y - dash, 1.5, y, ARENA_MARK, radius=1.5)
        y -= dash + gap
    canvas.ellipse(-40, -40, 40, 40, None, ARENA_MARK, 3.0)
    canvas.save(ASSETS / "two_player_pong/arena.png")


def draw_paddle() -> None:
    """Tinted per side in the game."""
    canvas = Canvas(PADDLE_WIDTH, PADDLE_HEIGHT)
    half_w, half_h = PADDLE_WIDTH / 2, PADDLE_HEIGHT / 2
    canvas.rect(-half_w + 0.3, -half_h + 0.3, half_w - 0.3, half_h - 0.3, WHITE, radius=3.0, outline=OUTLINE, width=0.9)
    canvas.rect(-1.0, -half_h + 5, 1.0, half_h - 5, LIGHT, radius=1.0)
    canvas.save(ASSETS / "two_player_pong/paddle.png")


def draw_ball() -> None:
    canvas = Canvas(BALL_SIZE, BALL_SIZE)
    half = BALL_SIZE / 2 - 0.3
    canvas.ellipse(-half, -half, half, half, WHITE, OUTLINE, 0.9)
    canvas.save(ASSETS / "two_player_pong/ball.png")


# Thumbnails: a minigame's art with a few players' things on it, as if mid-game.


def tinted(sprite: Image.Image, tint: tuple[float, float, float]) -> Image.Image:
    """`sprite` multiplied by `tint`, as the game tints a sprite with its player's colour."""
    colour = Image.new("RGBA", sprite.size, tuple(round(c * 255) for c in tint) + (255,))
    return ImageChops.multiply(sprite, colour)


def place(base: Image.Image, sprite: Image.Image, x: float, y: float, degrees: float = 0.0) -> None:
    """Draw `sprite` on `base` (a full-screen piece of art) centred on world point (`x`, `y`),
    turned `degrees` anticlockwise, and THUMBNAIL_SPRITE_SCALE times its size."""
    size = (round(sprite.width * THUMBNAIL_SPRITE_SCALE), round(sprite.height * THUMBNAIL_SPRITE_SCALE))
    sprite = sprite.resize(size, Image.Resampling.LANCZOS)
    if degrees:
        sprite = sprite.rotate(degrees, resample=Image.Resampling.BICUBIC, expand=True)
    left = (x + VIEW_W / 2) * ART_SCALE - sprite.width / 2
    top = (VIEW_H / 2 - y) * ART_SCALE - sprite.height / 2
    base.alpha_composite(sprite, (round(left), round(top)))


def save_thumbnail(base: Image.Image, path: Path) -> None:
    base.resize(THUMBNAIL_SIZE, Image.Resampling.LANCZOS).save(path)
    print(f"Wrote {path} ({THUMBNAIL_SIZE[0]}x{THUMBNAIL_SIZE[1]})")


def draw_pong_thumbnail() -> None:
    """Needs arena.png, paddle.png and ball.png."""
    folder = ASSETS / "two_player_pong"
    base = Image.open(folder / "arena.png").convert("RGBA")
    paddle = Image.open(folder / "paddle.png").convert("RGBA")
    ball = Image.open(folder / "ball.png").convert("RGBA")
    place(base, tinted(paddle, PLAYER_TINTS[0]), -PADDLE_X_OFFSET, 40.0)
    place(base, tinted(paddle, PLAYER_TINTS[1]), PADDLE_X_OFFSET, -70.0)
    place(base, ball, 150.0, -20.0)
    save_thumbnail(base, folder / "thumbnail.png")


def draw_racing_thumbnail() -> None:
    """Needs track.png and car.png. Four cars racing along the bottom straight, past the line."""
    folder = ASSETS / "racing"
    base = Image.open(folder / "track.png").convert("RGBA")
    car = Image.open(folder / "car.png").convert("RGBA")
    lanes = (BOTTOM_Y + GRID_LANE, BOTTOM_Y - GRID_LANE)
    for i, (tint, x) in enumerate(zip(PLAYER_TINTS, (110.0, 60.0, 5.0, -50.0))):
        place(base, tinted(car, tint), x, lanes[i % 2])
    save_thumbnail(base, folder / "thumbnail.png")


# Menu icons


def draw_gear() -> None:
    """The settings icon on a minigame's tile in the lobby. White, on nothing."""
    size = 256
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    centre = size / 2
    outer, body, hole = size * 0.47, size * 0.35, size * 0.14
    teeth = 8
    half_tooth = math.pi / teeth * 0.5
    for i in range(teeth):
        angle = 2 * math.pi * i / teeth
        corners = [
            (body, angle - half_tooth * 1.4),
            (outer, angle - half_tooth),
            (outer, angle + half_tooth),
            (body, angle + half_tooth * 1.4),
        ]
        draw.polygon(
            [(centre + r * math.cos(a), centre + r * math.sin(a)) for r, a in corners], fill=WHITE
        )
    draw.ellipse([centre - body, centre - body, centre + body, centre + body], fill=WHITE)
    draw.ellipse([centre - hole, centre - hole, centre + hole, centre + hole], fill=(0, 0, 0, 0))
    MENU_ASSETS.mkdir(parents=True, exist_ok=True)
    path = MENU_ASSETS / "gear.png"
    image.resize((64, 64), Image.Resampling.LANCZOS).save(path)
    print(f"Wrote {path} (64x64)")


def main() -> None:
    draw_track()
    draw_car()
    draw_arena()
    draw_paddle()
    draw_ball()
    # After the art they're made from.
    draw_pong_thumbnail()
    draw_racing_thumbnail()
    draw_gear()


if __name__ == "__main__":
    main()
