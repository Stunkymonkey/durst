#!/usr/bin/env python3
"""Find the notification boxes in a screenshot and print their geometry.

The background colour is taken from the top-left pixel. Boxes are the
connected regions of non-background pixels, which works as long as
notifications don't touch each other (gap > 0).

Output (JSON): {"size": [w, h], "boxes": [{"x","y","w","h","pad"}...], "gaps": [...]}
Boxes are sorted top to bottom, then left to right; `gaps` holds the vertical
distance between consecutive boxes. `pad` is [top, right, bottom, left]: the
distance from the box edge to its content (text, icons), i.e. everything that
differs from the box's own background. Clipped text shows up as a bottom pad
smaller than the top pad.
"""
import json
import sys

from PIL import Image


def main(path, tolerance=8):
    img = Image.open(path).convert("RGB")
    w, h = img.size
    px = img.load()
    bg = px[0, 0]

    def is_fg(x, y):
        p = px[x, y]
        return any(abs(p[i] - bg[i]) > tolerance for i in range(3))

    seen = bytearray(w * h)
    boxes = []
    for y in range(h):
        for x in range(w):
            i = y * w + x
            if seen[i] or not is_fg(x, y):
                continue
            # flood fill (iterative) to find the component's bounding box
            x0 = x1 = x
            y0 = y1 = y
            stack = [(x, y)]
            seen[i] = 1
            count = 0
            while stack:
                cx, cy = stack.pop()
                count += 1
                x0, x1 = min(x0, cx), max(x1, cx)
                y0, y1 = min(y0, cy), max(y1, cy)
                for nx, ny in ((cx + 1, cy), (cx - 1, cy), (cx, cy + 1), (cx, cy - 1)):
                    if 0 <= nx < w and 0 <= ny < h:
                        j = ny * w + nx
                        if not seen[j] and is_fg(nx, ny):
                            seen[j] = 1
                            stack.append((nx, ny))
            if count >= 50:  # ignore specks
                boxes.append({"x": x0, "y": y0, "w": x1 - x0 + 1, "h": y1 - y0 + 1})

    # merge boxes nested in others (e.g. text glyphs inside a box that isn't
    # connected to the border because of anti-aliasing)
    def inside(a, b):
        return (a is not b and a["x"] >= b["x"] and a["y"] >= b["y"]
                and a["x"] + a["w"] <= b["x"] + b["w"] and a["y"] + a["h"] <= b["y"] + b["h"])

    boxes = [a for a in boxes if not any(inside(a, b) for b in boxes)]
    for b in boxes:
        b["pad"] = content_pad(px, b, tolerance)
    boxes.sort(key=lambda b: (b["y"], b["x"]))
    gaps = [b["y"] - (a["y"] + a["h"]) for a, b in zip(boxes, boxes[1:])]
    print(json.dumps({"size": [w, h], "background": bg, "boxes": boxes, "gaps": gaps}))


def content_pad(px, b, tolerance, ring=6):
    """Distance from the box edges to the content inside it.

    The box background is sampled just inside the top edge, below the border;
    `ring` pixels along the edge (border + rounded corners) are skipped.
    """
    x0, y0, x1, y1 = b["x"], b["y"], b["x"] + b["w"] - 1, b["y"] + b["h"] - 1
    bg = px[(x0 + x1) // 2, y0 + ring]
    found = None
    for y in range(y0 + ring, y1 - ring + 1):
        for x in range(x0 + ring, x1 - ring + 1):
            p = px[x, y]
            if any(abs(p[i] - bg[i]) > tolerance * 4 for i in range(3)):
                if found is None:
                    found = [x, y, x, y]
                else:
                    found = [min(found[0], x), min(found[1], y), max(found[2], x), max(found[3], y)]
    if found is None:
        return None
    return [found[1] - y0, x1 - found[2], y1 - found[3], found[0] - x0]


if __name__ == "__main__":
    main(sys.argv[1])
