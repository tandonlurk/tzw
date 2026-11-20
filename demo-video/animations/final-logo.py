# Final TZW logo: the fresco itself (Michelangelo, "The Creation of Adam", public domain scan) with a
# gold Z coin painted in between the fingertips. The coin takes on the plaster's cracks and grain, a
# fresco colour grade, a cast shadow and a warm glow; the real fingers are laid back over it.
#
# Usage: python3 -I final-logo.py <scan.jpg> <hand-adam.png> <hand-god.png> <out.jpg>
#   scan: Wikimedia "Michelangelo - Creation of Adam (cropped).jpg" (3524 x 1599)
#   hands: public/logos/hand-*.png (cutout-hands.py; crop box below)
import sys
from PIL import Image, ImageDraw, ImageFilter, ImageFont, ImageChops
import numpy as np

scan_p, adam_p, god_p, out_p = sys.argv[1:5]
scan = Image.open(scan_p).convert("RGB")

CUT_BOX = (734, 499)                 # where the hand cutouts were taken from (cutout-hands.py crop)
GOD_X = 600                          # god cutout starts at this x inside that crop
# Final frame, 3:2, framed like the reference (forearms to the edges, hands slightly above centre)
W, H = 1170, 780
X0, Y0 = 718, 352
img = scan.crop((X0, Y0, X0 + W, Y0 + H))
ox, oy = CUT_BOX[0] - X0, CUT_BOX[1] - Y0          # cutout -> frame offset

# Coin: hangs just under the near-touching fingertips (Adam tip ~ (596,234), God tip ~ (606,220) in crop)
R = 50
CX, CY = ox + 601, oy + 227 + R - 6
SS = 4                                            # supersample for clean edges

def coin_layer():
    S = 2 * R * SS + 8 * SS
    c = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    yy, xx = np.mgrid[0:S, 0:S].astype(float)
    cx = cy = S / 2
    r = np.hypot(xx - cx, yy - cy) / (R * SS)
    # lit from the upper left, like the fresco
    lx, ly = (xx - cx) / (R * SS), (yy - cy) / (R * SS)
    light = np.clip(0.62 - 0.38 * lx - 0.46 * ly, 0, 1)
    base_hi, base_lo = np.array([246, 214, 136]), np.array([150, 104, 38])   # fresco gold
    col = base_lo + (base_hi - base_lo) * light[..., None]
    # rim: darker band with a bright inner bevel
    rim = (r > 0.86) & (r <= 1.0)
    col[rim] *= 0.78
    bevel = (r > 0.80) & (r <= 0.86)
    col[bevel] = col[bevel] * 1.12 + 10
    # soft specular glint upper-left, like light on worn metal
    spec = np.exp(-(((lx + 0.38) ** 2 + (ly + 0.42) ** 2) / 0.05))
    col = col + (np.array([255, 244, 214]) - col) * (spec[..., None] * 0.55)
    alpha = np.clip((1.0 - r) * R * SS / 1.5, 0, 1)                          # anti-aliased edge
    rgba = np.dstack([np.clip(col, 0, 255), alpha * 255]).astype(np.uint8)
    c = Image.fromarray(rgba, "RGBA")
    # embossed Z: shadow down-right, highlight up-left, then the face
    font = ImageFont.truetype("/System/Library/Fonts/Supplemental/Times New Roman Bold.ttf", int(R * SS * 1.25))
    d = ImageDraw.Draw(c)
    box = d.textbbox((0, 0), "Z", font=font)
    tx, ty = cx - (box[0] + box[2]) / 2, cy - (box[1] + box[3]) / 2
    o = 2.2 * SS
    d.text((tx + o, ty + o), "Z", font=font, fill=(92, 60, 20, 235))
    d.text((tx - o * 0.6, ty - o * 0.6), "Z", font=font, fill=(246, 222, 160, 200))
    d.text((tx, ty), "Z", font=font, fill=(196, 150, 64, 255))
    return c.resize((S // SS, S // SS), Image.LANCZOS)

coin = coin_layer()
cw = coin.width
cx0, cy0 = int(CX - cw / 2), int(CY - cw / 2)

base = np.asarray(img).astype(float)

# 1) warm glow on the wall around the coin (soft light, low)
yy, xx = np.mgrid[0:H, 0:W].astype(float)
g = np.exp(-(((xx - CX) ** 2 + (yy - CY) ** 2) / (2 * (R * 2.3) ** 2)))
glow = np.array([255, 214, 140])
base = base + (glow - base) * (g[..., None] * 0.22)

# 2) cast shadow on the plaster, down-right, soft
sh = Image.new("L", (W, H), 0)
ImageDraw.Draw(sh).ellipse((CX - R + 10, CY - R * 0.85 + 16, CX + R + 18, CY + R * 0.95 + 18), fill=255)
sh = np.asarray(sh.filter(ImageFilter.GaussianBlur(12))).astype(float) / 255
base = base * (1 - 0.32 * sh[..., None])

# 3) the coin itself, textured by the plaster underneath (cracks and grain carry through)
cA = np.zeros((H, W)); cRGB = np.zeros((H, W, 3))
ca = np.asarray(coin).astype(float)
cRGB[cy0:cy0 + cw, cx0:cx0 + cw] = ca[..., :3]
cA[cy0:cy0 + cw, cx0:cx0 + cw] = ca[..., 3] / 255
plaster_l = np.asarray(img.convert("L").filter(ImageFilter.GaussianBlur(0.6))).astype(float)
local = np.asarray(img.convert("L").filter(ImageFilter.GaussianBlur(14))).astype(float)
tex = np.clip(plaster_l / np.maximum(local, 1), 0.7, 1.15)              # cracks < 1, grain ~ 1
cRGB = cRGB * (0.5 + 0.5 * tex[..., None])
noise = np.random.default_rng(5).normal(0, 5, (H, W, 1))
cRGB = np.clip(cRGB + noise, 0, 255)
# soften like brushwork and grade toward the fresco (slight desaturation)
cImg = Image.fromarray(cRGB.astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.7))
cRGB = np.asarray(cImg).astype(float)
grey = cRGB.mean(axis=2, keepdims=True)
cRGB = grey + (cRGB - grey) * 0.9
base = base * (1 - cA[..., None]) + cRGB * cA[..., None]
# painted contour, like the sienna outlines on the hands
ring = Image.new("L", (W, H), 0)
ImageDraw.Draw(ring).ellipse((CX - R + 1, CY - R + 1, CX + R - 1, CY + R - 1), outline=255, width=2)
ring = np.asarray(ring.filter(ImageFilter.GaussianBlur(0.9))).astype(float) / 255
sienna = np.array([112, 70, 40])
base = base + (sienna - base) * (ring[..., None] * 0.55)

# 4) the real fingers back on top, with a soft contact shadow on the coin
hands = np.zeros((H, W))
for path, dx in ((adam_p, 0), (god_p, GOD_X)):
    h = Image.open(path)
    a = np.asarray(h).astype(float)[..., 3] / 255
    x, y = ox + dx, oy
    xs0, ys0 = max(0, x), max(0, y)
    xs1, ys1 = min(W, x + h.width), min(H, y + h.height)
    hands[ys0:ys1, xs0:xs1] = np.maximum(hands[ys0:ys1, xs0:xs1], a[ys0 - y:ys1 - y, xs0 - x:xs1 - x])
contact = np.asarray(Image.fromarray((hands * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(5))).astype(float) / 255
contact = np.roll(np.roll(contact, 4, axis=0), 3, axis=1)
base = base * (1 - 0.35 * (contact * cA)[..., None])
orig = np.asarray(img).astype(float)
base = base * (1 - hands[..., None]) + orig * hands[..., None]

# 5) one light grain pass over everything so nothing sits on top of the paint
base = np.clip(base + np.random.default_rng(9).normal(0, 2.2, (H, W, 1)), 0, 255)
Image.fromarray(base.astype(np.uint8)).save(out_p, quality=94)
print("wrote", out_p, W, H)
