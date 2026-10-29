"""Draw the Page plugin's icon: a page with its top right corner folded over, a magenta heading and four cyan
lines of writing, flat on a transparent background in the colours the Spreadsheet, Mermaid, JSON, Database and
Realm icons use.

It fills its square the way the Spreadsheet icon does (task-2402). The picture before it was a small white page
on a navy tile, and on a dark theme the tile vanished and the page read at half the size of every icon beside it.

python icon.py <folder> writes icon.png (32 by 32) and icon-128.png beside it.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw

CYAN = (0x22, 0xB8, 0xE6, 255)
MAGENTA = (0xD9, 0x46, 0xEF, 255)
SIZE = 1024


def page(left: float, top: float, right: float, bottom: float, fold: float, radius: float) -> list:
    """The page's outline as points: three rounded corners and the top right cut off where it folds."""
    import math

    def corner(cx: float, cy: float, start: float) -> list:
        return [
            (cx + radius * math.cos(math.radians(start + step)), cy + radius * math.sin(math.radians(start + step)))
            for step in range(0, 91, 5)
        ]

    return (
        corner(left + radius, top + radius, 180)
        + [(right - fold, top), (right, top + fold)]
        + corner(right - radius, bottom - radius, 0)
        + corner(left + radius, bottom - radius, 90)
    )


def draw() -> Image.Image:
    image = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    stroke = 64
    left, top, right, bottom = 160, 64, SIZE - 160, SIZE - 64
    fold = 240
    radius = 90
    # The outline is the page filled, with the page one stroke smaller cut out of it, so every corner is one
    # smooth curve rather than an arc meeting two lines.
    outline = Image.new("L", (SIZE, SIZE), 0)
    shape = ImageDraw.Draw(outline)
    shape.polygon(page(left, top, right, bottom, fold, radius), fill=255)
    inner_fold = fold - stroke * 0.41
    shape.polygon(
        page(left + stroke, top + stroke, right - stroke, bottom - stroke, inner_fold, radius - stroke * 0.6),
        fill=0,
    )
    # The flap folded down: a triangle under the cut corner.
    shape.polygon(
        [(right - fold, top), (right - fold, top + fold), (right, top + fold)],
        fill=255,
    )
    image.paste(Image.new("RGBA", (SIZE, SIZE), CYAN), (0, 0), outline)
    pen = ImageDraw.Draw(image)
    # A heading in magenta and four lines of writing, the last one short.
    inset = left + 130
    pen.rounded_rectangle((inset, 330, right - 300, 410), radius=40, fill=MAGENTA)
    for y, end in ((510, right - 130), (610, right - 130), (710, right - 130), (810, right - 330)):
        pen.rounded_rectangle((inset, y - 28, end, y + 28), radius=28, fill=CYAN)
    return image


def main() -> None:
    folder = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
    image = draw()
    image.resize((128, 128), Image.LANCZOS).save(folder / "icon-128.png")
    image.resize((32, 32), Image.LANCZOS).save(folder / "icon.png")


if __name__ == "__main__":
    main()
