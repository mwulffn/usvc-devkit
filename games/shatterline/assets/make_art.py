"""Draw the Shatterline artwork.

Writes tiles.png + tiles.txt and sprites.png + frames.txt next to this file.
Those four files are the source assets: tools/gfx.py turns them into C. Run
this script only to regenerate the art from scratch; hand edits to the PNG
files are lost when it runs.

    cd tools && uv run python ../games/shatterline/assets/make_art.py

Colours are given as console levels: red 0-7, green 0-7, blue 0-3.
"""

from pathlib import Path

from PIL import Image

HERE = Path(__file__).parent
TILE = 8
SHEET_COLUMNS = 16

Colour = tuple[int, int, int]

BLACK: Colour = (0, 0, 0)
WHITE: Colour = (7, 7, 3)

# One-hit bricks, in the order levels refer to them (1-8).
BRICK_COLOURS: dict[str, Colour] = {
    "red": (6, 1, 0),
    "orange": (6, 3, 0),
    "yellow": (6, 6, 0),
    "green": (1, 5, 0),
    "cyan": (0, 5, 2),
    "blue": (1, 2, 3),
    "violet": (4, 1, 2),
    "pink": (6, 2, 2),
}


def rgba(c: Colour) -> tuple[int, int, int, int]:
    r, g, b = c
    return (round(r * 255 / 7), round(g * 255 / 7), round(b * 255 / 3), 255)


def shade(c: Colour, amount: int) -> Colour:
    """Lighter (positive) or darker (negative) version of a colour."""
    r, g, b = c
    blue_step = (amount + (1 if amount > 0 else 0)) // 2
    clamp = lambda v, hi: max(0, min(hi, v))
    return (clamp(r + amount, 7), clamp(g + amount, 7), clamp(b + blue_step, 3))


class Canvas:
    """An RGBA image addressed with console colours."""

    def __init__(self, width: int, height: int) -> None:
        self.image = Image.new("RGBA", (width, height), (0, 0, 0, 0))

    def put(self, x: int, y: int, c: Colour) -> None:
        self.image.putpixel((x, y), rgba(c))

    def fill(self, x0: int, y0: int, w: int, h: int, c: Colour) -> None:
        for y in range(y0, y0 + h):
            for x in range(x0, x0 + w):
                self.put(x, y, c)

    def clear(self, x: int, y: int) -> None:
        self.image.putpixel((x, y), (0, 0, 0, 0))


def draw_brick(cv: Canvas, x0: int, y0: int, base: Colour) -> None:
    """A 16x8 glass block: bevelled edge and a diagonal glint."""
    light, dark = shade(base, 2), shade(base, -2)
    for y in range(8):
        for x in range(16):
            c = base
            if y == 0 or x == 0:
                c = light
            if y == 7 or x == 15:
                c = dark
            if 1 <= y <= 6 and 1 <= x <= 14 and x + y in (5, 6):
                c = shade(base, 3)
            if 1 <= y <= 6 and x + y == 12 and x <= 14:
                c = shade(base, 1)
            cv.put(x0 + x, y0 + y, c)


def draw_steel(cv: Canvas, x0: int, y0: int, cracked: bool) -> None:
    base: Colour = (4, 4, 2)
    draw_brick(cv, x0, y0, base)
    for x, y in ((2, 2), (13, 2), (2, 5), (13, 5)):
        cv.put(x0 + x, y0 + y, shade(base, -3))
    if cracked:
        crack = [
            (7, 1),
            (8, 2),
            (7, 3),
            (8, 4),
            (9, 5),
            (8, 6),
            (6, 4),
            (5, 5),
            (10, 3),
        ]
        for x, y in crack:
            cv.put(x0 + x, y0 + y, (1, 1, 0))


def draw_gold(cv: Canvas, x0: int, y0: int) -> None:
    draw_brick(cv, x0, y0, (6, 5, 0))
    for x in range(3, 13):
        cv.put(x0 + x, y0 + 3, (7, 7, 1))
        cv.put(x0 + x, y0 + 5, (4, 3, 0))


def draw_background(cv: Canvas, x0: int, y0: int) -> None:
    """A 16x16 field of faint dots. Deliberately has no lines: lines pull the
    eye away from the ball."""
    dot: Colour = (0, 1, 1)
    star: Colour = (1, 2, 1)
    cv.fill(x0, y0, 16, 16, BLACK)
    for x, y in ((3, 3), (11, 3), (3, 11)):
        cv.put(x0 + x, y0 + y, dot)
    cv.put(x0 + 11, y0 + 11, star)


PIPE: list[Colour] = [
    (1, 1, 1),
    (2, 3, 2),
    (4, 5, 3),
    (6, 7, 3),
    (4, 5, 3),
    (2, 3, 2),
    (1, 2, 1),
    (0, 1, 1),
]


def draw_wall(cv: Canvas, x0: int, y0: int, kind: str) -> None:
    """Pipe-like playfield border: 'side', 'top', or 'corner'."""
    for y in range(8):
        for x in range(8):
            if kind == "side":
                c = PIPE[x]
                if y == 7:
                    c = shade(c, -1)
            elif kind == "top":
                c = PIPE[y]
                if x == 7:
                    c = shade(c, -1)
            else:
                c = PIPE[min(max(x, y), 7)] if max(x, y) < 7 else PIPE[7]
                if x in (3, 4) and y in (3, 4):
                    c = (7, 7, 3)
            cv.put(x0 + x, y0 + y, c)


def draw_life_icon(cv: Canvas, x0: int, y0: int) -> None:
    cv.fill(x0, y0, 8, 8, BLACK)
    for x in range(8):
        cv.put(x0 + x, y0 + 3, (6, 6, 3))
        cv.put(x0 + x, y0 + 4, (3, 3, 2))
    for y in (3, 4):
        cv.put(x0, y0 + y, (0, 6, 3))
        cv.put(x0 + 7, y0 + y, (0, 6, 3))


# Big-letter tiles for the title: a full block and four with one corner cut.
LOGO_SHAPES = {"full": None, "tl": (0, 0), "tr": (1, 0), "bl": (0, 1), "br": (1, 1)}
LOGO_COLOURS: dict[str, Colour] = {"a": (0, 5, 3), "b": (6, 1, 2)}


def draw_logo_block(cv: Canvas, base: Colour, cut: tuple[int, int] | None) -> None:
    """An 8x8 glass block, optionally with one corner cut off diagonally."""
    for y in range(8):
        for x in range(8):
            c = base
            if x == 0 or y == 0:
                c = shade(base, 2)
            if x == 7 or y == 7:
                c = shade(base, -2)
            if x + y in (4, 5) and 0 < x < 7 and 0 < y < 7:
                c = shade(base, 3)
            if cut is not None:
                # Distance from the cut corner, measured along the diagonal.
                dx = x if cut[0] == 0 else 7 - x
                dy = y if cut[1] == 0 else 7 - y
                if dx + dy < 7:
                    c = BLACK
                elif dx + dy == 7:
                    c = shade(base, 2)
            cv.put(x, y, c)


def make_tiles() -> None:
    """Draw every tile into a list, then lay them out on the sheet."""
    tiles: list[tuple[str, Image.Image]] = []

    def add(name: str, cv: Canvas, x: int = 0, y: int = 0) -> None:
        tiles.append((name, cv.image.crop((x, y, x + TILE, y + TILE))))

    blank = Canvas(8, 8)
    blank.fill(0, 0, 8, 8, BLACK)
    add("blank", blank)

    bg = Canvas(16, 16)
    draw_background(bg, 0, 0)
    for i, (x, y) in enumerate(((0, 0), (8, 0), (0, 8), (8, 8))):
        add(f"bg_{i}", bg, x, y)

    for kind in ("side", "top", "corner"):
        cv = Canvas(8, 8)
        draw_wall(cv, 0, 0, kind)
        add(f"wall_{kind}", cv)

    icon = Canvas(8, 8)
    draw_life_icon(icon, 0, 0)
    add("life", icon)

    def add_brick(name: str, cv: Canvas) -> None:
        add(f"{name}_l", cv, 0, 0)
        add(f"{name}_r", cv, 8, 0)

    # Brick tiles are consecutive pairs so that kind N starts at
    # TILE_BRICK_RED_L + 2 * (N - 1).
    for name, colour in BRICK_COLOURS.items():
        cv = Canvas(16, 8)
        draw_brick(cv, 0, 0, colour)
        add_brick(f"brick_{name}", cv)
    for name, cracked in (("steel", False), ("steel_cracked", True)):
        cv = Canvas(16, 8)
        draw_steel(cv, 0, 0, cracked)
        add_brick(name, cv)
    gold = Canvas(16, 8)
    draw_gold(gold, 0, 0)
    add_brick("gold", gold)

    # Five shapes per colour, in this order (screen.c relies on it).
    for colour_name, colour in LOGO_COLOURS.items():
        for shape, cut in LOGO_SHAPES.items():
            cv = Canvas(8, 8)
            draw_logo_block(cv, colour, cut)
            add(f"logo_{colour_name}_{shape}", cv)

    rows = -(-len(tiles) // SHEET_COLUMNS)
    sheet = Image.new("RGBA", (SHEET_COLUMNS * TILE, rows * TILE), (0, 0, 0, 255))
    for i, (_, tile) in enumerate(tiles):
        sheet.paste(tile, ((i % SHEET_COLUMNS) * TILE, (i // SHEET_COLUMNS) * TILE))
    sheet.save(HERE / "tiles.png")
    (HERE / "tiles.txt").write_text(
        "# One name per 8x8 tile of tiles.png, left to right, top to bottom.\n"
        + "".join(f"{name}\n" for name, _ in tiles)
    )


CAP_NORMAL: Colour = (0, 6, 3)
CAP_LASER: Colour = (7, 1, 0)
CAP_CATCH: Colour = (2, 7, 0)


def draw_paddle(cv: Canvas, x0: int, y0: int, width: int, cap_colour: Colour) -> None:
    body: list[Colour] = [
        (3, 3, 1),
        (6, 6, 3),
        (7, 7, 3),
        (5, 5, 2),
        (4, 4, 2),
        (3, 3, 1),
        (2, 2, 1),
        (1, 1, 0),
    ]
    cap = [shade(cap_colour, d) for d in (-3, 0, 3, 0, -1, -2, -3, -4)]
    for y in range(8):
        for x in range(width):
            edge = min(x, width - 1 - x)
            c = cap[y] if edge < 6 else body[y]
            if edge == 6:
                c = (1, 1, 0)
            cv.put(x0 + x, y0 + y, c)
    # Rounded ends.
    for x, y in ((0, 0), (1, 0), (0, 1), (0, 7), (1, 7), (0, 6)):
        cv.clear(x0 + x, y0 + y)
        cv.clear(x0 + width - 1 - x, y0 + y)


def draw_ball(cv: Canvas, x0: int, y0: int) -> None:
    rows = [
        ".2332.",
        "243321",
        "333321",
        "332221",
        "222211",
        ".1111.",
    ]
    palette: dict[str, Colour] = {
        "1": (1, 3, 2),
        "2": (3, 6, 3),
        "3": (6, 7, 3),
        "4": WHITE,
    }
    for y, row in enumerate(rows):
        for x, ch in enumerate(row):
            if ch != ".":
                cv.put(x0 + x, y0 + y, palette[ch])


GLYPHS = {
    "E": ["XXXX", "X...", "XXX.", "X...", "XXXX"],
    "S": [".XXX", "X...", ".XX.", "...X", "XXX."],
    "M": ["X...X", "XX.XX", "X.X.X", "X...X", "X...X"],
    "L": ["X...", "X...", "X...", "X...", "XXXX"],
    "Z": ["XXXX", "...X", ".XX.", "X...", "XXXX"],
    "C": [".XXX", "X...", "X...", "X...", ".XXX"],
}

CAPSULES: dict[str, Colour] = {
    "E": (1, 2, 3),
    "S": (6, 3, 0),
    "M": (0, 5, 2),
    "L": (5, 1, 2),
    "Z": (6, 1, 0),
    "C": (2, 5, 0),
}


def draw_capsule(cv: Canvas, x0: int, y0: int, letter: str) -> None:
    base = CAPSULES[letter]
    for y in range(8):
        for x in range(16):
            c = base
            if y <= 1:
                c = shade(base, 2)
            if y >= 6:
                c = shade(base, -2)
            cv.put(x0 + x, y0 + y, c)
    for x, y in ((0, 0), (1, 0), (0, 1), (0, 7), (1, 7), (0, 6)):
        cv.clear(x0 + x, y0 + y)
        cv.clear(x0 + 15 - x, y0 + y)
    glyph = GLYPHS[letter]
    gx = x0 + (16 - len(glyph[0])) // 2
    for y, row in enumerate(glyph):
        for x, ch in enumerate(row):
            if ch == "X":
                cv.put(gx + x, y0 + 1 + y, WHITE)


def draw_shot(cv: Canvas, x0: int, y0: int) -> None:
    """A 2x6 laser bolt."""
    colours: list[Colour] = [
        WHITE,
        (7, 7, 1),
        (7, 5, 0),
        (7, 3, 0),
        (7, 1, 0),
        (5, 0, 0),
    ]
    for y, c in enumerate(colours):
        cv.put(x0, y0 + y, c)
        cv.put(x0 + 1, y0 + y, c)


def draw_wisp(cv: Canvas, x0: int, y0: int, frame: int) -> None:
    """A 12x12 drifting crystal; the two frames swap its lit facets."""
    base: Colour = (5, 1, 3)
    for y in range(12):
        for x in range(12):
            dx, dy = abs(2 * x - 11), abs(2 * y - 11)
            if dx + dy > 13:
                continue
            lit = (x < 6) == (y < 6)
            c = shade(base, 2 if lit == (frame == 0) else -1)
            if dx + dy > 10:
                c = shade(base, -3)
            if dx + dy <= 2:
                c = WHITE
            cv.put(x0 + x, y0 + y, c)


def make_sprites() -> None:
    cv = Canvas(96, 56)
    frames = []
    # Paddle frames are in the order of the paddle modes in game.c.
    paddles = (
        ("paddle", 32, CAP_NORMAL),
        ("paddle_wide", 48, CAP_NORMAL),
        ("paddle_laser", 32, CAP_LASER),
        ("paddle_catch", 32, CAP_CATCH),
    )
    for i, (name, width, cap) in enumerate(paddles):
        draw_paddle(cv, 0, i * 8, width, cap)
        frames.append(f"{name} 0 {i * 8} {width} 8 0 0")
    draw_ball(cv, 0, 32)
    frames.append("ball 0 32 6 6 0 0")
    draw_shot(cv, 8, 32)
    frames.append("shot 8 32 2 6 0 0")
    for i in range(2):
        draw_wisp(cv, 16 + i * 12, 32, i)
        frames.append(f"wisp_{i} {16 + i * 12} 32 12 12 0 0")
    # Capsule frames are in the order of the capsule types in game.c.
    for i, letter in enumerate(CAPSULES):
        draw_capsule(cv, i * 16, 48, letter)
        frames.append(f"capsule_{letter.lower()} {i * 16} 48 16 8 0 0")
    cv.image.save(HERE / "sprites.png")
    (HERE / "frames.txt").write_text(
        "# name x y width height handle_x handle_y (in sprites.png)\n"
        + "".join(f"{f}\n" for f in frames)
    )


if __name__ == "__main__":
    make_tiles()
    make_sprites()
    print(f"wrote tiles.png, tiles.txt, sprites.png, frames.txt in {HERE}")
