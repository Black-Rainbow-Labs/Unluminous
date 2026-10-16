"""Draw the picture the Unluminous Glass theme is shown over: glossy spheres on fine diagonal pinstripes.

task-2281. The Claude Design canvas "Unluminous", page "Agent-Chat 2", shows its glass over a generated
picture of glossy coloured spheres on fine diagonal pinstripes, sharp and busy, because glass that bends a
flat colour shows nothing. That picture is portrait and made for one pane. This draws the same thing in
the shape of a window, deterministically, so it can be made again:

    python tools/glass-spheres.py

It writes design/background-images/glass-spheres.png and the JPEG the binary carries,
crates/unluminous-app/backgrounds/glass-spheres.jpg. It needs numpy and Pillow.
"""

import os

import numpy as np
from PIL import Image

WIDTH, HEIGHT = 1920, 1088
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

# Centre x, centre y, radius, and the colour lit from the top left, read off the design's backdrop.
SPHERES = [
    (1480, 250, 230, (139, 107, 255)),   # violet
    (250, 430, 200, (238, 108, 88)),     # coral
    (930, 300, 52, (47, 207, 166)),      # small mint
    (1020, 640, 70, (240, 176, 80)),     # small amber
    (1720, 820, 250, (74, 152, 240)),    # blue
    (520, 880, 220, (47, 207, 166)),     # mint
    (1260, 980, 60, (238, 108, 88)),     # small coral
    (1880, 70, 120, (255, 182, 72)),     # amber, half off the edge
    (40, 1060, 150, (139, 107, 255)),    # violet, half off the corner
]


def ground():
    """The page: a near black with a pinstripe every eleven pixels, running down to the right."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float32)
    base = np.array([11, 13, 20], np.float32)
    stripe = np.array([44, 52, 80], np.float32)
    phase = np.mod(x + y, 11.0)
    # A line a pixel and a half wide, antialiased.
    line = np.clip(1.0 - np.abs(phase - 5.5) / 1.1, 0.0, 1.0)[..., None]
    return base + (stripe - base) * line


def sphere(image, cx, cy, radius, colour):
    """A glossy sphere lit from the top left: diffuse shading, a dark side, a hot highlight and a rim."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float32)
    dx, dy = (x - cx) / radius, (y - cy) / radius
    r2 = dx * dx + dy * dy
    inside = r2 <= 1.0
    if not inside.any():
        return
    z = np.sqrt(np.clip(1.0 - r2, 0.0, 1.0))
    light = np.array([-0.45, -0.55, 0.70], np.float32)
    light /= np.linalg.norm(light)
    lambert = np.clip(dx * light[0] + dy * light[1] + z * light[2], 0.0, 1.0)
    reflect_z = 2.0 * lambert * z - light[2]
    specular = np.clip(reflect_z, 0.0, 1.0) ** 40
    rim = np.clip(1.0 - z, 0.0, 1.0) ** 3
    base = np.array(colour, np.float32)
    shade = base * (0.12 + 0.95 * lambert[..., None])
    shade += 255.0 * 0.85 * specular[..., None]
    shade += base * 0.35 * rim[..., None]
    # Antialias the edge over a pixel.
    edge = np.clip((1.0 - np.sqrt(r2)) * radius, 0.0, 1.0)[..., None]
    image[:] = image * (1.0 - edge) + np.clip(shade, 0, 255) * edge


def shadow(image, cx, cy, radius):
    """A soft dark fall under and right of a sphere, so it sits above the page."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float32)
    d = np.sqrt((x - cx - radius * 0.12) ** 2 + (y - cy - radius * 0.18) ** 2) / (radius * 1.15)
    fall = np.clip(1.0 - d, 0.0, 1.0) ** 2 * 0.55
    image[:] = image * (1.0 - fall[..., None])


def main():
    image = ground()
    for cx, cy, radius, colour in SPHERES:
        shadow(image, cx, cy, radius)
    for cx, cy, radius, colour in SPHERES:
        sphere(image, cx, cy, radius, colour)
    picture = Image.fromarray(np.clip(image, 0, 255).astype(np.uint8))
    picture.save(os.path.join(ROOT, "design", "background-images", "glass-spheres.png"), optimize=True)
    picture.save(
        os.path.join(ROOT, "crates", "unluminous-app", "backgrounds", "glass-spheres.jpg"),
        quality=88,
        optimize=True,
        progressive=True,
    )


if __name__ == "__main__":
    main()
