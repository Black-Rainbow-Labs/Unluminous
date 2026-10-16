"""Find light lines on the rims of round controls in a window screenshot (task-2281).

A glass control reads as having a white edge when its rim is brighter than both the glass just inside it
and the picture just outside it. This finds every round control in a band of the screenshot and measures
that, along 96 rays from each centre:

    rim excess = brightest point within a pixel and a half of the rim
                 - the brighter of the inside (four pixels in) and the outside (four pixels out)

in luminance levels (0 to 255): the median over every ray, and the median over the rays of the top
quarter, where a lit top edge shows. A rim that blends in is at or below zero all round. The glass theme's
soft light from the top reads about +4 along the top and is meant to.

    python tools/glass-rims.py <screenshot.png> [--band top|bottom|all] [--max 3] [--top-max 6]

It exits 1 when any control is above --max all round or above --top-max along the top, so it can be run
after a change as a check. Unluminous 0.91.0's title bar chips measured +27 all round and +44 along the top;
after task-2281's fix, +2 and +4.

Controls are found as circles: the picture is converted to luminance, and every circle of radius 13 to 33
pixels whose rim is a closed ring of strong gradient is taken. That is enough for the title bar's chips,
the rail's buttons and the chat's round buttons, which are the controls the report was about.
"""

import argparse
import math
import sys

import numpy as np
from PIL import Image


def luminance(path):
    """The picture as luminance, float, 0 to 255."""
    a = np.asarray(Image.open(path).convert("RGB")).astype(np.float32)
    return 0.2126 * a[..., 0] + 0.7152 * a[..., 1] + 0.0722 * a[..., 2]


def sample(lum, x, y):
    """Bilinear luminance at a point, or None off the picture."""
    h, w = lum.shape
    if x < 0 or y < 0 or x >= w - 1 or y >= h - 1:
        return None
    x0, y0 = int(x), int(y)
    fx, fy = x - x0, y - y0
    top = lum[y0, x0] * (1 - fx) + lum[y0, x0 + 1] * fx
    bottom = lum[y0 + 1, x0] * (1 - fx) + lum[y0 + 1, x0 + 1] * fx
    return top * (1 - fy) + bottom * fy


def ring_strength(grad, cx, cy, r):
    """How much of a circle's rim lies on strong gradient: the fraction of 48 rim points above the mark."""
    hits = 0
    for k in range(48):
        a = 2 * math.pi * k / 48
        x, y = int(round(cx + r * math.cos(a))), int(round(cy + r * math.sin(a)))
        if 0 <= y < grad.shape[0] and 0 <= x < grad.shape[1] and grad[y, x] > 6:
            hits += 1
    return hits / 48


def find_circles(lum, y_from, y_to):
    """Centres and radii of round controls between two rows: closed rings of gradient, best first."""
    gy, gx = np.gradient(lum)
    grad = np.hypot(gx, gy)
    # Smear the gradient by a pixel so a rim one pixel off a whole radius still counts.
    g = grad.copy()
    for dy in (-1, 0, 1):
        for dx in (-1, 0, 1):
            g = np.maximum(g, np.roll(np.roll(grad, dy, 0), dx, 1))
    found = []
    h, w = lum.shape
    for cy in range(max(y_from, 10), min(y_to, h - 10), 2):
        for cx in range(12, w - 12, 2):
            for r in range(13, 34):
                s = ring_strength(g, cx, cy, r)
                if s >= 0.9:
                    found.append((s, cx, cy, r))
    found.sort(reverse=True)
    kept = []
    for s, cx, cy, r in found:
        if all(math.hypot(cx - kx, cy - ky) > max(r, kr) for _, kx, ky, kr in kept):
            kept.append((s, cx, cy, r))
    return [(cx, cy, r) for _, cx, cy, r in kept]


def refine(lum, cx, cy, r):
    """The radius at which the ring's outline really is: where luminance changes most along the rays."""
    best, best_r = -1.0, r
    for rr in np.arange(r - 2, r + 2.01, 0.5):
        total = 0.0
        for k in range(48):
            a = 2 * math.pi * k / 48
            inner = sample(lum, cx + (rr - 1) * math.cos(a), cy + (rr - 1) * math.sin(a))
            outer = sample(lum, cx + (rr + 1) * math.cos(a), cy + (rr + 1) * math.sin(a))
            if inner is not None and outer is not None:
                total += abs(inner - outer)
        if total > best:
            best, best_r = total, rr
    return best_r


def rim_excess(lum, cx, cy, r):
    """The rim's brightness above the brighter of inside and outside: the median over 96 rays all round,
    and the median over the rays of the top quarter, where a lit top edge shows."""
    excess, top = [], []
    for k in range(96):
        a = 2 * math.pi * k / 96
        c, s = math.cos(a), math.sin(a)
        rim = [sample(lum, cx + t * c, cy + t * s) for t in np.arange(r - 2.0, r + 1.01, 0.25)]
        inside = sample(lum, cx + (r - 5) * c, cy + (r - 5) * s)
        outside = sample(lum, cx + (r + 4) * c, cy + (r + 4) * s)
        if None in rim or inside is None or outside is None:
            continue
        excess.append(max(rim) - max(inside, outside))
        if 225 <= math.degrees(a) <= 315:
            top.append(excess[-1])
    if not excess:
        return float("nan"), float("nan")
    return float(np.median(excess)), float(np.median(top)) if top else float("nan")


def main():
    p = argparse.ArgumentParser()
    p.add_argument("shot")
    p.add_argument("--band", default="top", choices=["top", "bottom", "all"])
    p.add_argument("--max", type=float, default=3.0)
    p.add_argument("--top-max", type=float, default=6.0)
    args = p.parse_args()
    lum = luminance(args.shot)
    h = lum.shape[0]
    y_from, y_to = {"top": (0, 70), "bottom": (h - 70, h), "all": (0, h)}[args.band]
    circles = find_circles(lum, y_from, y_to)
    worst, worst_top = -1e9, -1e9
    for cx, cy, r in circles:
        rr = refine(lum, cx, cy, r)
        e, t = rim_excess(lum, cx, cy, rr)
        worst, worst_top = max(worst, e), max(worst_top, t)
        print(f"control at ({cx},{cy}) radius {rr:.1f}: rim excess {e:+.1f} all round, {t:+.1f} along the top")
    print(f"{len(circles)} controls, worst {worst:+.1f} all round (limit {args.max:+.1f}), "
          f"{worst_top:+.1f} along the top (limit {args.top_max:+.1f})")
    sys.exit(1 if worst > args.max or worst_top > args.top_max else 0)


if __name__ == "__main__":
    main()
