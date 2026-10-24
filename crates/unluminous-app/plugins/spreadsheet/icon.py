"""Draw the Spreadsheet plugin's icon: a sheet of cells, cyan, with its header row and first column in
magenta, flat on a transparent background in the colours the Mermaid, JSON, Database and Realm icons use.

python icon.py <folder> writes icon.png (32 by 32) and icon-128.png beside it.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw

CYAN = (0x22, 0xB8, 0xE6, 255)
MAGENTA = (0xD9, 0x46, 0xEF, 255)
SIZE = 1024


def draw() -> Image.Image:
    image = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    pen = ImageDraw.Draw(image)
    margin = 96
    left, top, right, bottom = margin, margin + 40, SIZE - margin, SIZE - margin - 40
    radius = 120
    stroke = 64
    # The header row and the first column, filled.
    header = top + (bottom - top) // 4
    first = left + (right - left) // 4
    pen.rounded_rectangle((left, top, right, header), radius=radius, fill=MAGENTA)
    pen.rectangle((left, top + radius, right, header), fill=MAGENTA)
    pen.rounded_rectangle((left, top, first, bottom), radius=radius, fill=MAGENTA)
    pen.rectangle((left + radius, top, first, bottom), fill=MAGENTA)
    # The sheet's outline and its grid.
    pen.rounded_rectangle((left, top, right, bottom), radius=radius, outline=CYAN, width=stroke)
    for i in (2, 3):
        x = left + (right - left) * i // 4
        pen.line((x, header, x, bottom - stroke // 2), fill=CYAN, width=stroke // 2 + 8)
        y = top + (bottom - top) * i // 4
        pen.line((first, y, right - stroke // 2, y), fill=CYAN, width=stroke // 2 + 8)
    pen.line((first, header, right - stroke // 2, header), fill=CYAN, width=stroke // 2 + 8)
    pen.line((first, header, first, bottom - stroke // 2), fill=CYAN, width=stroke // 2 + 8)
    return image


def main() -> None:
    folder = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
    image = draw()
    image.resize((128, 128), Image.LANCZOS).save(folder / "icon-128.png")
    image.resize((32, 32), Image.LANCZOS).save(folder / "icon.png")


if __name__ == "__main__":
    main()
