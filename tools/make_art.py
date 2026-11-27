"""Draw the placeholder art for every minigame, in the game's flat style, for a 1080p screen, plus
each minigame's thumbnail for its tile in the lobby, and the menus' icons.

Everything is laid out in world units (the 640x360 virtual screen, y up, (0, 0) in the middle) and
drawn at ART_SCALE texture pixels per unit. Shapes are drawn SUPERSAMPLE times bigger still and
then shrunk, for smooth edges.

Shapes that gameplay depends on (the racing track, Pong's walls) use the same constants as each
minigame's config.rs (src/minigames/<minigame>/config.rs), and the racing track the same pieces as
src/minigames/racing/track.rs: keep them in step. Run from the repository root:

    python3 tools/make_art.py

Then press F3 in a race to check that the road edges line up with the art. Needs Pillow.
"""

import math
from collections.abc import Iterator
from pathlib import Path
from dataclasses import dataclass
from typing import Final, TypeAlias

from PIL import Image, ImageChops, ImageDraw

RGBA: TypeAlias = tuple[int, int, int, int]

ART_SCALE: Final[int] = 3  # config::display::ART_SCALE
SUPERSAMPLE: Final[int] = 4
VIEW_W: Final[float] = 640.0  # config::display::VIEW_WIDTH
VIEW_H: Final[float] = 360.0  # config::display::VIEW_HEIGHT

# Keep in step with minigames::racing::config.
GRID_ROW: Final[float] = 24.0  # GRID_ROW_SPACING
GRID_LANE: Final[float] = 14.0  # GRID_LANE_OFFSET
CAR_LENGTH: Final[float] = 16.0  # CAR_LENGTH
CAR_WIDTH: Final[float] = 10.0  # CAR_WIDTH



@dataclass(frozen=True)
class TrackLayout:
    """Keep in step with minigames::racing::track::Layout."""

    name: str  # the art's file name, in assets/minigames/racing/
    start_line: tuple[float, float]
    road_half_width: float
    # Its pieces, leaving out the checkpoints: ("straight", length) or ("turn", radius,
    # degrees), with positive degrees turning left.
    pieces: list[tuple]
    crossing: tuple[float, float] | None = None
    # Which way cars face on the grid. Only 0 (right) is drawn.
    start_heading: float = 0.0


# Keep in step with minigames::racing::track: FIGURE_EIGHT and OVAL.
FIGURE_EIGHT: Final[TrackLayout] = TrackLayout(
    name="figure_eight",
    start_line=(-60.0, -40.0),
    road_half_width=24.0,
    crossing=(50.0, -40.0),
    pieces=[
        ("straight", 280.0),
        ("turn", 45.0, 90.0),
        ("straight", 85.0),
        ("turn", 45.0, 90.0),
        ("straight", 20.0),
        ("turn", 25.0, 90.0),
        ("straight", 20.0),
        ("turn", 32.0, -90.0),
        ("turn", 32.0, -90.0),
        ("straight", 20.0),
        ("turn", 25.0, 90.0),
        ("straight", 306.0),
        ("turn", 32.0, 90.0),
        ("turn", 32.0, 90.0),
        ("straight", 230.0),
        ("turn", 40.0, -90.0),
        ("straight", 106.0),
        ("straight", 15.0),
        ("turn", 50.0, -90.0),
        ("straight", 105.0),
        ("straight", 105.0),
        ("turn", 50.0, -90.0),
        ("turn", 50.0, -90.0),
        ("straight", 150.0),
    ],
)
OVAL: Final[TrackLayout] = TrackLayout(
    name="oval",
    start_line=(0.0, -130.0),
    road_half_width=30.0,
    pieces=[
        ("straight", 140.0),
        ("turn", 130.0, 90.0),
        ("turn", 130.0, 90.0),
        ("straight", 140.0),
        ("straight", 140.0),
        ("turn", 130.0, 90.0),
        ("turn", 130.0, 90.0),
        ("straight", 140.0),
    ],
)
TRACKS: Final[list[TrackLayout]] = [FIGURE_EIGHT, OVAL]

# Keep in step with minigames::racing::checkpoint_guide::CHEVRON_SIZE.
CHEVRON_SIZE: Final[tuple[float, float]] = (10.0, 12.0)

# Keep in step with minigames::pong_royale::config.
ROYALE_ARENA_HALF: Final[float] = 172.0  # ARENA_HALF
ROYALE_CORNER: Final[float] = 40.0  # CORNER_SIZE
ROYALE_PADDLE_DISTANCE: Final[float] = ROYALE_ARENA_HALF - 14.0  # PADDLE_DISTANCE

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

KERB: Final[float] = 4.0  # kerb width, inside the road on the bends
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
ARENA_OUTSIDE: Final[RGBA] = (16, 16, 22, 255)
FLOOR_A: Final[RGBA] = (62, 58, 54, 255)
FLOOR_B: Final[RGBA] = (66, 62, 57, 255)
FLOOR_LINE: Final[RGBA] = (54, 50, 47, 255)
WALL: Final[RGBA] = (112, 104, 96, 255)
WALL_TOP: Final[RGBA] = (138, 129, 119, 255)
BARREL: Final[RGBA] = (44, 44, 50, 255)
BOX_JUNCTION: Final[RGBA] = (240, 196, 64, 255)

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

    def polygon(
        self,
        points: list[tuple[float, float]],
        fill: RGBA,
        outline: RGBA | None = None,
        width: float = 0.0,
    ) -> None:
        self.draw.polygon(
            [self.point(x, y) for x, y in points],
            fill=fill,
            outline=outline,
            width=round(width * self.scale),
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


def grid_slots(track: TrackLayout) -> Iterator[tuple[float, float]]:
    """The same positions as Track::grid_position, for 8 cars."""
    fx, fy = math.cos(track.start_heading), math.sin(track.start_heading)
    sx, sy = track.start_line
    for slot in range(8):
        row, outside = slot // 2, slot % 2 == 1
        back = GRID_ROW * (row + 1) + (GRID_ROW / 2 if outside else 0)
        lane = -GRID_LANE if outside else GRID_LANE
        yield sx - fx * back - fy * lane, sy - fy * back + fx * lane


# A point on the track's centre line: x, y, and which way the track runs there (radians).
TrackPoint: TypeAlias = tuple[float, float, float]


def track_pieces(track: TrackLayout) -> list[tuple[tuple, list[TrackPoint]]]:
    """Each of the track's pieces, with points along it at most 1 unit apart, both ends included,
    followed on from the start line as track.rs does."""
    x, y = track.start_line
    heading = track.start_heading
    pieces = []
    for piece in track.pieces:
        kind, *size = piece
        if kind == "straight":
            (length,) = size
            count = max(1, math.ceil(length))
            points = [
                (x + math.cos(heading) * length * i / count,
                 y + math.sin(heading) * length * i / count,
                 heading)
                for i in range(count + 1)
            ]
        else:
            radius, degrees = size
            sweep = math.radians(degrees)
            side = 1.0 if sweep > 0 else -1.0
            # The circle's centre is off to the side the car turns towards.
            cx = x - math.sin(heading) * radius * side
            cy = y + math.cos(heading) * radius * side
            start = math.atan2(y - cy, x - cx)
            count = max(2, math.ceil(abs(sweep) * radius))
            points = []
            for i in range(count + 1):
                angle = start + sweep * i / count
                points.append(
                    (cx + math.cos(angle) * radius, cy + math.sin(angle) * radius,
                     heading + sweep * i / count)
                )
        pieces.append((piece, points))
        x, y, heading = points[-1]
    return pieces


def offset(point: TrackPoint, distance: float) -> tuple[float, float]:
    """The point `distance` to the left of `point` on the centre line (to the right if negative)."""
    x, y, heading = point
    return x - math.sin(heading) * distance, y + math.cos(heading) * distance


def band(canvas: Canvas, points: list[TrackPoint], near: float, far: float, fill: RGBA) -> None:
    """The strip between `near` and `far` to the left of the line through `points` (to the right
    if negative)."""
    for a, b in zip(points, points[1:]):
        canvas.polygon([offset(a, near), offset(b, near), offset(b, far), offset(a, far)], fill)


def kerbs(
    canvas: Canvas, half_width: float, radius: float, degrees: float, points: list[TrackPoint]
) -> None:
    """Red and white kerbs along the edges of a bend, inside the road, in pieces about
    KERB_SEGMENT long along the edge. A bend so tight that its inside edge nearly meets the
    circle's centre gets none on the inside, where they'd only be a dot."""
    inside = 1.0 if degrees > 0 else -1.0
    for sign in (1.0, -1.0):
        if sign == inside and radius - half_width < 2 * KERB:
            continue
        near, far = sign * (half_width - KERB), sign * half_width
        piece: list[TrackPoint] = [points[0]]
        colour = 0
        travelled = 0.0
        for a, b in zip(points, points[1:]):
            piece.append(b)
            travelled += math.dist(offset(a, far), offset(b, far))
            if travelled >= KERB_SEGMENT:
                band(canvas, piece, near, far, (KERB_RED, KERB_WHITE)[colour])
                piece, colour, travelled = [b], 1 - colour, 0.0
        if len(piece) > 1:
            band(canvas, piece, near, far, (KERB_RED, KERB_WHITE)[colour])


def box_junction(canvas: Canvas, crossing: tuple[float, float], half: float) -> None:
    """Yellow criss-cross lines over the square where the road crosses itself, so it reads as a
    crossing at a glance. The square is `half` either side of `crossing`."""
    cx, cy = crossing
    hatch = Canvas(VIEW_W, VIEW_H)
    spacing = 8.0
    reach = 2 * half
    steps = math.ceil(reach / spacing)
    for i in range(-steps, steps + 1):
        d = i * spacing
        hatch.line(cx + d - reach, cy - reach, cx + d + reach, cy + reach, BOX_JUNCTION, 1.0)
        hatch.line(cx + d - reach, cy + reach, cx + d + reach, cy - reach, BOX_JUNCTION, 1.0)
    inset = 3.0
    box = [round(v) for v in canvas.box(cx - half + inset, cy - half + inset, cx + half - inset, cy + half - inset)]
    canvas.image.alpha_composite(hatch.image.crop(box), (box[0], box[1]))
    canvas.rect(cx - half + inset, cy - half + inset, cx + half - inset, cy + half - inset,
                None, outline=BOX_JUNCTION, width=1.2)


def draw_track(track: TrackLayout) -> None:
    assert track.start_heading == 0.0, "the start line and grid are only drawn facing right"
    half_width = track.road_half_width
    canvas = Canvas(VIEW_W, VIEW_H)

    # Grass with mowing stripes.
    stripe = 20.0
    for i in range(math.ceil(VIEW_W / stripe)):
        x0 = -VIEW_W / 2 + i * stripe
        canvas.rect(x0, -VIEW_H / 2, x0 + stripe, VIEW_H / 2, (GRASS_A, GRASS_B)[i % 2])

    # Dark edges all round, then the bends' kerbs, then the road over everything, so where the
    # road crosses itself it runs straight over the other stretch's edges.
    pieces = track_pieces(track)
    for _, points in pieces:
        band(canvas, points, -(half_width + EDGE), half_width + EDGE, GRASS_EDGE)
    for (kind, *size), points in pieces:
        if kind == "turn":
            kerbs(canvas, half_width, *size, points)
    for (kind, *_), points in pieces:
        half = half_width - (KERB if kind == "turn" else 0.0)
        band(canvas, points, -half, half, ROAD)
    if track.crossing:
        box_junction(canvas, track.crossing, half_width)

    # Start line: a chequered strip across the road, two squares deep.
    square = 4.0
    sx, sy = track.start_line
    y = sy - half_width
    row = 0
    while y < sy + half_width:
        top = min(y + square, sy + half_width)
        for column in range(2):
            x0 = sx - square + column * square
            colour = KERB_WHITE if (row + column) % 2 else LINE_DARK
            canvas.rect(x0, y, x0 + square, top, colour)
        y = top
        row += 1

    # Grid boxes: a white bracket in front of each starting slot.
    for gx, gy in grid_slots(track):
        nose = gx + CAR_LENGTH / 2 + 2
        half = CAR_WIDTH / 2 + 2
        canvas.line(nose, gy - half, nose, gy + half, KERB_WHITE, 1.2)
        for edge in (gy - half, gy + half):
            canvas.line(nose - 6, edge, nose, edge, KERB_WHITE, 1.2)

    canvas.save(ASSETS / f"racing/{track.name}.png")


def draw_chevron() -> None:
    """Pointing right. White, tinted per player in the game, with a dark outline so it shows on
    the road and the grass."""
    length, width = CHEVRON_SIZE
    canvas = Canvas(length, width)
    x, y = length / 2 - 0.6, width / 2 - 0.6
    arm = 4.0
    canvas.polygon(
        [(-x, y), (-x + arm, y), (x, 0.0), (-x + arm, -y), (-x, -y), (x - arm, 0.0)],
        WHITE,
        outline=OUTLINE,
        width=0.8,
    )
    canvas.save(ASSETS / "racing/chevron.png")


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


def draw_royale_arena() -> None:
    """A square arena in the middle, a goal on each side and a block in each corner. The walls
    that close goals are drawn by the game, in ARENA_WALL like the corner blocks."""
    half, corner = ROYALE_ARENA_HALF, ROYALE_CORNER
    canvas = Canvas(VIEW_W, VIEW_H, ARENA_OUTSIDE)
    canvas.rect(-half, -half, half, half, ARENA)
    # A dashed line along each goal, between the corner blocks.
    dash, gap = 10.0, 8.0
    goal = half - corner
    for sign in (1, -1):
        along = -goal + gap / 2
        while along + dash < goal:
            line = half - 1.0
            canvas.rect(sign * line - 1.0, along, sign * line + 1.0, along + dash, ARENA_MARK)
            canvas.rect(along, sign * line - 1.0, along + dash, sign * line + 1.0, ARENA_MARK)
            along += dash + gap
    for x in (-1, 1):
        for y in (-1, 1):
            canvas.rect(x * half, y * half, x * (half - corner), y * (half - corner), ARENA_WALL,
                        radius=3.0)
    canvas.ellipse(-36, -36, 36, 36, None, ARENA_MARK, 3.0)
    canvas.save(ASSETS / "pong_royale/arena.png")


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


def draw_royale_thumbnail() -> None:
    """Needs pong_royale/arena.png and Pong's paddle.png and ball.png. A paddle on every side."""
    folder = ASSETS / "pong_royale"
    base = Image.open(folder / "arena.png").convert("RGBA")
    pong = ASSETS / "two_player_pong"
    paddle = Image.open(pong / "paddle.png").convert("RGBA")
    ball = Image.open(pong / "ball.png").convert("RGBA")
    d = ROYALE_PADDLE_DISTANCE
    # (x, y, degrees) for the left, right, bottom and top paddles, each part way along its side.
    paddles = [(-d, 50.0, 0.0), (d, -60.0, 0.0), (40.0, -d, 90.0), (-70.0, d, 90.0)]
    for tint, (x, y, degrees) in zip(PLAYER_TINTS, paddles):
        place(base, tinted(paddle, tint), x, y, degrees)
    place(base, ball, 60.0, 30.0)
    save_thumbnail(base, folder / "thumbnail.png")


def draw_racing_thumbnails() -> None:
    """Needs each track's art and car.png. Racing's lobby tile shows the picked track's:
    <track>_thumbnail.png (see Layout::thumbnail in track.rs)."""
    folder = ASSETS / "racing"
    car = Image.open(folder / "car.png").convert("RGBA")
    for track in TRACKS:
        base = Image.open(folder / f"{track.name}.png").convert("RGBA")
        sx, sy = track.start_line
        lanes = (sy + GRID_LANE, sy - GRID_LANE)
        if track.crossing:
            # Three cars on the start straight, heading into the crossing, and a fourth coming
            # down at them.
            cx, cy = track.crossing
            cars = [(5.0, lanes[0], 0.0), (-45.0, lanes[1], 0.0), (-95.0, lanes[0], 0.0),
                    (cx, cy + 62.0, -90.0)]
        else:
            # Four cars racing along the start straight, past the line.
            cars = [(sx + x, lanes[i % 2], 0.0) for i, x in enumerate((110.0, 60.0, 5.0, -50.0))]
        for tint, (x, y, degrees) in zip(PLAYER_TINTS, cars):
            place(base, tinted(car, tint), x, y, degrees)
        save_thumbnail(base, folder / f"{track.name}_thumbnail.png")


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


def draw_leaf() -> None:
    """A stylized flat-vector leaf drawn white so the menu tints it in twilight foliage shades."""
    size = 16.0
    canvas = Canvas(size, size)
    # Tapered leaf silhouette made from a smooth polygon, pointing right (+x).
    outline: list[tuple[float, float]] = []
    steps = 24
    for i in range(steps + 1):
        t = i / steps
        x = -6.5 + 13.0 * t
        width = 3.8 * math.sin(math.pi * t) * (1.0 - 0.15 * t)
        outline.append((x, width))
    for i in range(steps, -1, -1):
        t = i / steps
        x = -6.5 + 13.0 * t
        width = 3.8 * math.sin(math.pi * t) * (1.0 - 0.15 * t)
        outline.append((x, -width))
    canvas.draw.polygon([canvas.point(x, y) for x, y in outline], fill=WHITE)
    # Subtle centre vein.
    canvas.line(-6.0, 0.0, 5.2, 0.0, LIGHT, 0.55)
    MENU_ASSETS.mkdir(parents=True, exist_ok=True)
    canvas.save(MENU_ASSETS / "leaf.png")


def draw_frond() -> None:
    """A stylized leafy branch silhouette for the menu corners, drawn white so the game tints
    and sways it gently in the breeze."""
    canvas = Canvas(64.0, 36.0)
    # Curved stem entering from the left edge (-32, 6) arching down-right.
    stem = [(-32.0, 6.0), (-18.0, 4.0), (-4.0, 0.5), (12.0, -4.5), (26.0, -9.5)]
    for (x0, y0), (x1, y1) in zip(stem, stem[1:]):
        canvas.line(x0, y0, x1, y1, WHITE, 1.8)
    # Paired leaves along the branch, getting smaller toward the tip.
    leaves = [
        (-24.0, 5.0, 0.55, 8.5),
        (-22.0, 4.5, -0.75, 7.8),
        (-13.0, 3.0, 0.42, 8.2),
        (-12.0, 2.5, -0.82, 7.6),
        (-2.0, 0.0, 0.32, 7.5),
        (-1.0, -0.5, -0.88, 7.0),
        (9.0, -3.5, 0.22, 6.6),
        (10.0, -4.0, -0.92, 6.2),
        (19.0, -7.0, 0.12, 5.4),
        (20.0, -7.5, -0.95, 5.0),
        (26.5, -9.8, -0.35, 5.2),
    ]
    for lx, ly, angle, length in leaves:
        cos_a, sin_a = math.cos(angle), math.sin(angle)
        pts: list[tuple[float, float]] = []
        steps = 12
        for side in (1.0, -1.0):
            rng = range(steps + 1) if side > 0 else range(steps, -1, -1)
            for i in rng:
                t = i / steps
                local_x = t * length
                local_y = side * (length * 0.34) * math.sin(math.pi * t)
                px = lx + local_x * cos_a - local_y * sin_a
                py = ly + local_x * sin_a + local_y * cos_a
                pts.append(canvas.point(px, py))
        canvas.draw.polygon(pts, fill=WHITE)
    MENU_ASSETS.mkdir(parents=True, exist_ok=True)
    canvas.save(MENU_ASSETS / "frond.png")


def main() -> None:
    for track in TRACKS:
        draw_track(track)
    draw_car()
    draw_chevron()
    draw_arena()
    draw_royale_arena()
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
    draw_royale_thumbnail()
    draw_racing_thumbnails()
    draw_shootout_thumbnail()
    draw_gear()
    draw_leaf()
    draw_frond()


if __name__ == "__main__":
    main()
