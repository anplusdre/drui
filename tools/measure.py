# measure.py img: prints color runs along rows / columns (in logical px = physical / scale)
import sys
from PIL import Image

im = Image.open(sys.argv[1]).convert("RGB")
S = float(sys.argv[2])
px = im.load()

def hexc(c):
    return "#%02x%02x%02x" % c

def runs_row(y, x0, x1, min_len=2):
    yy = int(y * S)
    out, start, cur = [], int(x0 * S), px[int(x0 * S), yy]
    for x in range(int(x0 * S), int(x1 * S)):
        c = px[x, yy]
        if max(abs(a - b) for a, b in zip(c, cur)) > 6:
            if x - start >= min_len:
                out.append((round(start / S, 1), round((x - start) / S, 1), hexc(cur)))
            start, cur = x, c
    out.append((round(start / S, 1), round((int(x1 * S) - start) / S, 1), hexc(cur)))
    return out

def runs_col(x, y0, y1, min_len=2):
    xx = int(x * S)
    out, start, cur = [], int(y0 * S), px[xx, int(y0 * S)]
    for y in range(int(y0 * S), int(y1 * S)):
        c = px[xx, y]
        if max(abs(a - b) for a, b in zip(c, cur)) > 6:
            if y - start >= min_len:
                out.append((round(start / S, 1), round((y - start) / S, 1), hexc(cur)))
            start, cur = y, c
    out.append((round(start / S, 1), round((int(y1 * S) - start) / S, 1), hexc(cur)))
    return out

for spec in sys.argv[3:]:
    kind, a, b, c = spec.split(":")
    a, b, c = float(a), float(b), float(c)
    print(spec)
    for r in (runs_row(a, b, c) if kind == "row" else runs_col(a, b, c)):
        print("   ", r)
