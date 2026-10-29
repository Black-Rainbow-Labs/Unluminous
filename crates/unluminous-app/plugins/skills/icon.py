"""Draw the Skills Manager's icon: three blocks in an L in cyan and a fourth lifted away to the top right in
magenta, after Lucide's `blocks` (LuBlocks), flat on a transparent background in the colours the Mermaid, JSON,
Database, Realm and Spreadsheet icons use.

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
    cell = 300
    gap = 40
    radius = 56
    left = 120
    bottom = SIZE - 120
    for x, y in [(left, bottom - 2 * cell - gap), (left, bottom - cell), (left + cell + gap, bottom - cell)]:
        pen.rounded_rectangle((x, y, x + cell, y + cell), radius=radius, fill=CYAN)
    lifted = (SIZE - 120 - cell, 120)
    pen.rounded_rectangle((lifted[0], lifted[1], lifted[0] + cell, lifted[1] + cell), radius=radius, fill=MAGENTA)
    return image


if __name__ == "__main__":
    folder = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
    big = draw()
    big.resize((128, 128), Image.LANCZOS).save(folder / "icon-128.png")
    big.resize((32, 32), Image.LANCZOS).save(folder / "icon.png")
