# Final TZW logo (v2): the fresco hands (Michelangelo, "The Creation of Adam", public domain scan),
# re-posed on a diagonal around a centred gold Z coin, on a clean plate of the same fresco plaster.
# Adam's hand reaches up from the lower left at ~45 deg, God's comes down from the upper right.
# Output: 1500 x 1000 (3:2, Devpost thumbnail).
#
# Usage: python3 -I final-logo.py <scan.jpg> <hand-adam.png> <hand-god.png> <out.jpg>
#   scan: Wikimedia "Michelangelo - Creation of Adam (cropped).jpg" (3524 x 1599)
#   hands: public/logos/hand-*.png (made by cutout-hands.py)
# v1 (hands as painted, coin under the fingertips): final-logo-v1.py
import sys, math
from PIL import Image, ImageDraw, ImageFilter, ImageFont
import numpy as np

scan_p, adam_p, god_p, out_p = sys.argv[1:5]
W, H = 1500, 1000
CX, CY = W // 2, H // 2

# --- clean plate: an empty stretch of the same plaster (cracks included), scaled to fill ---
scan = Image.open(scan_p).convert("RGB")
plate = scan.crop((860, 40, 1720, 613)).resize((W, H), Image.LANCZOS)
base = np.asarray(plate).astype(float)

# --- hands: scale, rotate about the fingertip, place on the diagonal ---
S = 1.5
R = 74                       # coin radius
GAP = 16                     # spark gap between each fingertip and the coin rim
HANDS = [
    # file, fingertip in the cutout, rotation (deg, counter-clockwise), direction from coin centre to tip
    (adam_p, (596, 234), 40, 225),   # lower left, reaching up
    (god_p, (6, 220), 22, 32),       # upper right, reaching down
]

def place(path, tip, rot, dir_deg):
    h = Image.open(path).convert("RGBA")
    h = h.resize((int(h.width * S), int(h.height * S)), Image.LANCZOS)
    tx, ty = tip[0] * S, tip[1] * S
    big = Image.new("RGBA", (3200, 3200), (0, 0, 0, 0))
    bx, by = 1600 - int(tx), 1600 - int(ty)
    big.paste(h, (bx, by), h)
    big = big.rotate(rot, resample=Image.BICUBIC, center=(1600, 1600))
    a = math.radians(dir_deg)
    px = CX + (R + GAP) * math.cos(a)
    py = CY - (R + GAP) * math.sin(a)
    layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    layer.paste(big, (int(px - 1600), int(py - 1600)), big)
    return np.asarray(layer).astype(float)

layers = [place(*h) for h in HANDS]

# --- coin ---
SS = 4
def coin_layer():
    Sz = 2 * R * SS + 8 * SS
    yy, xx = np.mgrid[0:Sz, 0:Sz].astype(float)
    c0 = Sz / 2
    r = np.hypot(xx - c0, yy - c0) / (R * SS)
    lx, ly = (xx - c0) / (R * SS), (yy - c0) / (R * SS)
    light = np.clip(0.62 - 0.38 * lx - 0.46 * ly, 0, 1)
    hi, lo = np.array([246, 214, 136]), np.array([150, 104, 38])
    col = lo + (hi - lo) * light[..., None]
    col[(r > 0.86) & (r <= 1.0)] *= 0.78
    bev = (r > 0.80) & (r <= 0.86)
    col[bev] = col[bev] * 1.12 + 10
    spec = np.exp(-(((lx + 0.38) ** 2 + (ly + 0.42) ** 2) / 0.05))
    col = col + (np.array([255, 244, 214]) - col) * (spec[..., None] * 0.55)
    alpha = np.clip((1.0 - r) * R * SS / 1.5, 0, 1)
    c = Image.fromarray(np.dstack([np.clip(col, 0, 255), alpha * 255]).astype(np.uint8), "RGBA")
    font = ImageFont.truetype("/System/Library/Fonts/Supplemental/Times New Roman Bold.ttf", int(R * SS * 1.25))
    d = ImageDraw.Draw(c)
    box = d.textbbox((0, 0), "Z", font=font)
    tx, ty = c0 - (box[0] + box[2]) / 2, c0 - (box[1] + box[3]) / 2
    o = 2.2 * SS
    d.text((tx + o, ty + o), "Z", font=font, fill=(92, 60, 20, 235))
    d.text((tx - o * 0.6, ty - o * 0.6), "Z", font=font, fill=(246, 222, 160, 200))
    d.text((tx, ty), "Z", font=font, fill=(196, 150, 64, 255))
    return c.resize((Sz // SS, Sz // SS), Image.LANCZOS)

coin = coin_layer(); cw = coin.width
cx0, cy0 = CX - cw // 2, CY - cw // 2
yy, xx = np.mgrid[0:H, 0:W].astype(float)

# warm glow around the coin
g = np.exp(-(((xx - CX) ** 2 + (yy - CY) ** 2) / (2 * (R * 2.4) ** 2)))
base = base + (np.array([255, 214, 140]) - base) * (g[..., None] * 0.24)

# soft cast shadows on the wall: the coin and both arms (light from the upper left)
def shadow_from(alpha, dx, dy, blur, k):
    m = Image.fromarray((alpha * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(blur))
    m = np.roll(np.roll(np.asarray(m).astype(float) / 255, dy, axis=0), dx, axis=1)
    return m * k
cA = np.zeros((H, W)); cRGB = np.zeros((H, W, 3))
ca = np.asarray(coin).astype(float)
cRGB[cy0:cy0 + cw, cx0:cx0 + cw] = ca[..., :3]
cA[cy0:cy0 + cw, cx0:cx0 + cw] = ca[..., 3] / 255
sh = shadow_from(cA, 16, 18, 12, 0.30)
for L in layers:
    sh = np.maximum(sh, shadow_from(L[..., 3] / 255, 14, 18, 14, 0.22))
base = base * (1 - sh[..., None])

# coin textured by the plaster under it, softened and graded toward the fresco
pl = np.asarray(plate.convert("L").filter(ImageFilter.GaussianBlur(0.6))).astype(float)
local = np.asarray(plate.convert("L").filter(ImageFilter.GaussianBlur(14))).astype(float)
tex = np.clip(pl / np.maximum(local, 1), 0.7, 1.15)
cRGB = cRGB * (0.5 + 0.5 * tex[..., None])
cRGB = np.clip(cRGB + np.random.default_rng(5).normal(0, 5, (H, W, 1)), 0, 255)
cRGB = np.asarray(Image.fromarray(cRGB.astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.8))).astype(float)
grey = cRGB.mean(axis=2, keepdims=True)
cRGB = grey + (cRGB - grey) * 0.9
base = base * (1 - cA[..., None]) + cRGB * cA[..., None]
ring = Image.new("L", (W, H), 0)
ImageDraw.Draw(ring).ellipse((CX - R + 1, CY - R + 1, CX + R - 1, CY + R - 1), outline=255, width=3)
ring = np.asarray(ring.filter(ImageFilter.GaussianBlur(1.0))).astype(float) / 255
base = base + (np.array([112, 70, 40]) - base) * (ring[..., None] * 0.55)

# hands on top
for L in layers:
    a = L[..., 3:4] / 255
    base = base * (1 - a) + L[..., :3] * a

# one grain pass and a gentle vignette so it reads as one painted surface
base = base + np.random.default_rng(9).normal(0, 2.4, (H, W, 1))
v = np.hypot((xx - CX) / (W * 0.62), (yy - CY) / (H * 0.62))
base = base * (1 - 0.18 * np.clip(v - 0.45, 0, 1)[..., None])
Image.fromarray(np.clip(base, 0, 255).astype(np.uint8)).save(out_p, quality=94)
print("wrote", out_p, W, H)
