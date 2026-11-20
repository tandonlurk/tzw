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
    binm = Image.fromarray(((alpha > 0.15) * 255).astype(np.uint8))
    ImageDraw.floodfill(binm, SEEDS[name], 128)
    keep = Image.fromarray(((np.asarray(binm) == 128) * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(5))
    alpha = alpha * (np.asarray(keep).astype(float) / 255)
    alpha = np.asarray(Image.fromarray((alpha * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.8)))
    rgba = np.dstack([a.astype(np.uint8), alpha])[:, x0:x1]
    Image.fromarray(rgba, "RGBA").save(f"{out}/{name}.png")
    # preview on dark
    bg = Image.new("RGBA", (x1 - x0, H), (40, 40, 40, 255)); bg.alpha_composite(Image.fromarray(rgba, "RGBA")); bg.convert("RGB").save(f"{out}/{name}-preview.jpg")
cut(ADAM, "hand-adam", 0, 605)
cut(GOD, "hand-god", 600, 1005)
