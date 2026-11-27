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

# Keep in step with minigames::shootout::config.
SHOOTOUT_BORDER: Final[float] = 8.0  # BORDER
GUNNER_RADIUS: Final[float] = 8.0  # GUNNER_RADIUS
PAINT_BALL_RADIUS: Final[float] = 3.0  # PAINT_BALL_RADIUS
PICKUP_RADIUS: Final[float] = 8.0  # PICKUP_RADIUS
# COVER: (centre x, centre y, width, height) of each block.
SHOOTOUT_COVER: Final[list[tuple[float, float, float, float]]] = [
    (0.0, 0.0, 48.0, 48.0),
    (-170.0, 80.0, 90.0, 16.0),
    (170.0, 80.0, 90.0, 16.0),
    (-170.0, -80.0, 90.0, 16.0),
    (170.0, -80.0, 90.0, 16.0),
    (-260.0, 0.0, 16.0, 70.0),
    (260.0, 0.0, 16.0, 70.0),
]
# SPAWNS, the first four.
SHOOTOUT_SPAWNS: Final[list[tuple[float, float]]] = [
    (-280.0, 140.0), (280.0, -140.0), (280.0, 140.0), (-280.0, -140.0),
]

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
FLOOR_A: Final[RGBA] = (62, 58, 54, 255)
FLOOR_B: Final[RGBA] = (66, 62, 57, 255)
FLOOR_LINE: Final[RGBA] = (54, 50, 47, 255)
WALL: Final[RGBA] = (112, 104, 96, 255)
WALL_TOP: Final[RGBA] = (138, 129, 119, 255)
BARREL: Final[RGBA] = (44, 44, 50, 255)

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


# Shootout


def draw_shootout_arena() -> None:
    """A tiled floor, the border wall, and the cover blocks, all seen from above."""
    canvas = Canvas(VIEW_W, VIEW_H, WALL)
    half_w, half_h = VIEW_W / 2 - SHOOTOUT_BORDER, VIEW_H / 2 - SHOOTOUT_BORDER
    tile = 32.0
    columns, rows = math.ceil(2 * half_w / tile), math.ceil(2 * half_h / tile)
    for column in range(columns):
        for row in range(rows):
            x0, y0 = -half_w + column * tile, -half_h + row * tile
            x1, y1 = min(x0 + tile, half_w), min(y0 + tile, half_h)
            canvas.rect(x0, y0, x1, y1, (FLOOR_A, FLOOR_B)[(column + row) % 2])
    for column in range(1, columns):
        x = -half_w + column * tile
        canvas.line(x, -half_h, x, half_h, FLOOR_LINE, 0.6)
    for row in range(1, rows):
        y = -half_h + row * tile
        canvas.line(-half_w, y, half_w, y, FLOOR_LINE, 0.6)
    # The border wall's inner edge, lit from above.
    canvas.rect(-half_w, -half_h, half_w, half_h, None, outline=WALL_TOP, width=1.5)
    for x, y, width, height in SHOOTOUT_COVER:
        canvas.rect(
            x - width / 2, y - height / 2, x + width / 2, y + height / 2, WALL,
            radius=2.0, outline=OUTLINE, width=1.0,
        )
        canvas.rect(
            x - width / 2 + 2.5, y - height / 2 + 2.5, x + width / 2 - 2.5, y + height / 2 - 2.5,
            WALL_TOP, radius=1.5,
        )
    canvas.save(ASSETS / "shootout/arena.png")


def draw_gunner() -> None:
    """Seen from above, aiming right, centred on the body. White and light grey parts are tinted
    per player; the barrel isn't."""
    size = 2 * GUNNER_RADIUS + 12.0  # visuals::GUNNER_IMAGE_SIZE
    canvas = Canvas(size, size)
    canvas.rect(3.0, -2.0, 13.0, 2.0, BARREL, radius=1.0, outline=OUTLINE, width=0.6)
    r = GUNNER_RADIUS - 0.4
    canvas.ellipse(-r, -r, r, r, WHITE, OUTLINE, 0.9)
    canvas.ellipse(-3.5, -3.5, 3.5, 3.5, LIGHT, LIGHT, 0.1)
    canvas.save(ASSETS / "shootout/gunner.png")


def draw_paint_ball() -> None:
    """A small round paint ball, drawn white with a dark outline so the game tints it per player."""
    size = 2 * PAINT_BALL_RADIUS
    canvas = Canvas(size, size)
    r = PAINT_BALL_RADIUS - 0.3
    canvas.ellipse(-r, -r, r, r, WHITE, OUTLINE, 0.8)
    canvas.save(ASSETS / "shootout/paint_ball.png")


def draw_paint_splat() -> None:
    """A flat paint splat with a few droplets around it, drawn white so the game tints it per
    player."""
    size = 20.0
    canvas = Canvas(size, size)
    # Main irregular splat from overlapping circles, plus small outer droplets.
    blobs = [
        (0.0, 0.0, 4.6),
        (2.8, 1.2, 2.7),
        (-2.6, 1.6, 2.5),
        (-1.8, -2.4, 2.8),
        (2.0, -2.2, 2.3),
        (5.6, 2.8, 1.3),
        (-5.8, -0.8, 1.2),
        (0.8, 6.0, 1.1),
        (-3.2, -5.6, 1.0),
        (4.8, -4.6, 0.9),
    ]
    for cx, cy, r in blobs:
        canvas.ellipse(cx - r, cy - r, cx + r, cy + r, WHITE, WHITE, 0.0)
    canvas.save(ASSETS / "shootout/paint_splat.png")


def draw_pickups() -> None:
    """Three circular centre-arena power-up badges: Spread (amber), Rapid Fire (cyan) and
    Shield (teal)."""
    size = 2 * PICKUP_RADIUS
    r = PICKUP_RADIUS - 0.6

    # Triple Spread: warm amber badge with three fanned paint dots.
    spread = Canvas(size, size)
    spread.ellipse(-r, -r, r, r, (245, 165, 45, 255), OUTLINE, 0.9)
    for dx, dy in ((3.0, 3.0), (3.8, 0.0), (3.0, -3.0)):
        spread.line(-3.5, 0.0, dx, dy, WHITE, 1.2)
        spread.ellipse(dx - 1.2, dy - 1.2, dx + 1.2, dy + 1.2, WHITE, WHITE, 0.0)
    spread.save(ASSETS / "shootout/pickup_spread.png")

    # Rapid Fire: electric cyan badge with a lightning bolt.
    rapid = Canvas(size, size)
    rapid.ellipse(-r, -r, r, r, (55, 205, 240, 255), OUTLINE, 0.9)
    bolt = [
        (0.8, 4.4),
        (-2.6, 0.2),
        (0.2, 0.2),
        (-1.0, -4.4),
        (2.8, -0.2),
        (0.0, -0.2),
    ]
    rapid.draw.polygon([rapid.point(x, y) for x, y in bolt], fill=WHITE)
    rapid.save(ASSETS / "shootout/pickup_rapid.png")

    # Shield: crisp mint/emerald badge with a bold plus sign.
    shield = Canvas(size, size)
    shield.ellipse(-r, -r, r, r, (70, 210, 145, 255), OUTLINE, 0.9)
    shield.rect(-1.4, -4.2, 1.4, 4.2, WHITE, radius=0.5)
    shield.rect(-4.2, -1.4, 4.2, 1.4, WHITE, radius=0.5)
    shield.save(ASSETS / "shootout/pickup_shield.png")


def draw_buff_ring() -> None:
    """A crisp white halo drawn behind a buffed gunner and tinted per buff in the game."""
    size = 2 * GUNNER_RADIUS + 10.0
    canvas = Canvas(size, size)
    r = GUNNER_RADIUS + 3.0
    canvas.ellipse(-r, -r, r, r, None, WHITE, 1.8)
    canvas.save(ASSETS / "shootout/buff_ring.png")


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


def draw_shootout_thumbnail() -> None:
    """Needs shootout/arena.png and gunner.png. Four gunners, at their spawns, aiming in."""
    folder = ASSETS / "shootout"
    base = Image.open(folder / "arena.png").convert("RGBA")
    gunner = Image.open(folder / "gunner.png").convert("RGBA")
    for (x, y), tint in zip(SHOOTOUT_SPAWNS, PLAYER_TINTS):
        place(base, tinted(gunner, tint), x, y, math.degrees(math.atan2(-y, -x)))
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
    draw_shootout_arena()
    draw_gunner()
    draw_paint_ball()
    draw_paint_splat()
    draw_pickups()
    draw_buff_ring()
    # After the art they're made from.
    draw_pong_thumbnail()
    draw_racing_thumbnail()
    draw_shootout_thumbnail()
    draw_gear()


if __name__ == "__main__":
    main()
