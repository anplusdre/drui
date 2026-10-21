# ink.py img scale x0 x1 y0 y1 : rows of text ink (bright pixels) inside a box, in logical px, with the
# horizontal extent and brightest color of each ink band
import sys
from PIL import Image

im = Image.open(sys.argv[1]).convert("RGB")
S = float(sys.argv[2])
x0, x1, y0, y1 = (int(float(v) * S) for v in sys.argv[3:7])
thr = int(sys.argv[7]) if len(sys.argv) > 7 else 110
px = im.load()
lum = lambda c: (c[0] * 3 + c[1] * 6 + c[2]) // 10
band = None
def flush(b, y):
    if b:
        ys, xs_min, xs_max, best = b
        print("ink y %.2f-%.2f (h %.2f) x %.2f-%.2f  brightest #%02x%02x%02x" % (ys / S, y / S, (y - ys) / S, xs_min / S, (xs_max + 1) / S, *best))
for y in range(y0, y1):
    xs = [x for x in range(x0, x1) if lum(px[x, y]) > thr]
    if xs:
        best = max((px[x, y] for x in xs), key=lum)
        if band is None:
            band = [y, min(xs), max(xs), best]
        else:
            band[1] = min(band[1], min(xs)); band[2] = max(band[2], max(xs))
            if lum(best) > lum(band[3]): band[3] = best
    elif band is not None:
        flush(band, y); band = None
flush(band, y1)
