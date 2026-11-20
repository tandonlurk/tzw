# Cut Adam's and God's hands out of the fresco crop. Usage: python3 -I cutout.py <crop.jpg> <outdir>
import sys
from PIL import Image, ImageDraw, ImageFilter
import numpy as np
src, out = sys.argv[1], sys.argv[2]
c = Image.open(src).convert("RGB"); W, H = c.size
a = np.asarray(c).astype(float); d = a[..., 0] - a[..., 2]
ADAM = [(0,238),(60,234),(130,228),(200,208),(290,178),(360,146),(400,139),(450,143),(500,158),(540,178),(572,200),(598,230),(592,242),(576,246),(572,268),(556,272),(542,262),(527,270),(506,268),(492,244),(470,232),(447,226),(420,217),(392,214),(352,238),(300,282),(250,318),(205,328),(150,333),(80,348),(0,362)]
GOD = [(603,224),(618,213),(660,204),(700,197),(742,184),(792,177),(850,150),(930,128),(1000,116),(1000,236),(930,246),(860,250),(820,256),(792,266),(772,292),(748,318),(733,321),(720,302),(712,324),(690,324),(682,300),(678,276),(666,268),(648,260),(638,240),(618,233)]
SEEDS = {"hand-adam": (100, 290), "hand-god": (900, 190)}  # points inside each forearm (crop coords)
def cut(poly, name, x0, x1):
    m = Image.new("L", (W, H), 0); ImageDraw.Draw(m).polygon(poly, fill=255)
    m = m.filter(ImageFilter.MaxFilter(5)).filter(ImageFilter.GaussianBlur(1.5))
    poly_a = np.asarray(m).astype(float) / 255
    col = np.clip((d - 37) / 12, 0, 1)            # plaster has r-b ~ 20-35, skin above
    col = np.asarray(Image.fromarray((col * 255).astype(np.uint8)).filter(ImageFilter.MedianFilter(5)).filter(ImageFilter.MinFilter(5)).filter(ImageFilter.MaxFilter(5))).astype(float) / 255
    alpha = poly_a * col
    # keep only the arm itself: flood-fill from a seed inside it and drop every other island
    binm = Image.fromarray(((alpha > 0.15) * 255).astype(np.uint8)).copy()  # copy: arrays come in read-only
    ys, xs = np.nonzero(np.asarray(binm) == 255)
    hx, hy = SEEDS[name]
    i = int(np.argmin((xs - hx) ** 2 + (ys - hy) ** 2))   # snap the hint to the nearest solid pixel
    ImageDraw.floodfill(binm, (int(xs[i]), int(ys[i])), 128)
    keep = Image.fromarray(((np.asarray(binm) == 128) * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(5))
    alpha = alpha * (np.asarray(keep).astype(float) / 255)
    alpha = np.asarray(Image.fromarray((alpha * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.8)))
    rgba = np.dstack([a.astype(np.uint8), alpha])[:, x0:x1]
    Image.fromarray(rgba, "RGBA").save(f"{out}/{name}.png")
    # preview on dark
    bg = Image.new("RGBA", (x1 - x0, H), (40, 40, 40, 255)); bg.alpha_composite(Image.fromarray(rgba, "RGBA")); bg.convert("RGB").save(f"{out}/{name}-preview.jpg")
cut(ADAM, "hand-adam", 0, 605)
cut(GOD, "hand-god", 600, 1005)

# Long version of God's arm (up to the sleeve), cut straight from the full scan, for final-logo.py.
# Usage: python3 -I cutout-hands.py <crop.jpg> <outdir> <scan.jpg>
if len(sys.argv) > 3:
    BOX = (1320, 430, 2000, 840)                      # full-scan region; fingertip at local (20, 289)
    s = Image.open(sys.argv[3]).convert("RGB").crop(BOX)
    W, H = s.size
    a = np.asarray(s).astype(float); d = a[..., 0] - a[..., 2]
    LONG = [(1337,723),(1352,712),(1394,703),(1434,696),(1476,683),(1526,676),(1584,649),(1664,627),(1734,615),
            (1760,622),(1800,614),(1850,594),(1900,566),(1950,538),(1985,520),(1985,596),(1950,626),(1900,663),(1850,696),
            (1800,720),(1750,736),(1700,743),(1664,745),(1594,749),(1554,755),(1526,765),(1506,791),(1482,817),
            (1467,820),(1454,801),(1446,823),(1424,823),(1416,799),(1412,775),(1400,767),(1382,759),(1372,739),(1352,732)]
    GOD_LONG = [(x - BOX[0], y - BOX[1]) for x, y in LONG]
    SEEDS["hand-god-long"] = (380, 250)
    a_full = a
    def cut_long():
        global a, d
        m = Image.new("L", (W, H), 0); ImageDraw.Draw(m).polygon(GOD_LONG, fill=255)
        m = m.filter(ImageFilter.GaussianBlur(1.5))     # no dilation: the red mantle sits right outside
        poly_a = np.asarray(m).astype(float) / 255
        col = np.clip((d - 37) / 12, 0, 1)
        col = np.asarray(Image.fromarray((col * 255).astype(np.uint8)).filter(ImageFilter.MedianFilter(5)).filter(ImageFilter.MinFilter(5)).filter(ImageFilter.MaxFilter(5))).astype(float) / 255
        alpha = poly_a * col
        binm = Image.fromarray(((alpha > 0.15) * 255).astype(np.uint8)).copy()
        ys, xs = np.nonzero(np.asarray(binm) == 255)
        hx, hy = SEEDS["hand-god-long"]
        i = int(np.argmin((xs - hx) ** 2 + (ys - hy) ** 2))
        ImageDraw.floodfill(binm, (int(xs[i]), int(ys[i])), 128)
        keep = Image.fromarray(((np.asarray(binm) == 128) * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(5))
        alpha = alpha * (np.asarray(keep).astype(float) / 255)
        alpha = np.asarray(Image.fromarray((alpha * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.8)))
        Image.fromarray(np.dstack([a.astype(np.uint8), alpha]), "RGBA").save(f"{out}/hand-god-long.png")
        bg = Image.new("RGBA", (W, H), (40, 40, 40, 255)); bg.alpha_composite(Image.open(f"{out}/hand-god-long.png")); bg.convert("RGB").save(f"{out}/hand-god-long-preview.jpg")
    cut_long()
