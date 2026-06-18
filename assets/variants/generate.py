import random, math
from PIL import Image, ImageDraw

SRC = "/Users/erik/source/rust-game/tilemap.png"
BASE = (183, 75, 48, 255)
LIGHT = (214, 106, 55, 255)
DARK = (134, 59, 48, 255)
DARKER = (95, 42, 38, 255)
M_DARK = (58, 82, 38, 255)
M_MID = (92, 122, 52, 255)
M_LIGHT = (140, 170, 74, 255)


def load_tile():
    im = Image.open(SRC).convert("RGBA")
    return im.crop((0, 0, 40, 40)).copy()


def is_base(t, x, y):
    return 0 <= x < 40 and 0 <= y < 40 and t.getpixel((x, y)) == BASE


def put(t, x, y, c):
    if is_base(t, x, y):
        t.putpixel((x, y), c)


# ---- moss (the keeper algorithm, just reseeded) ----
def mossy(seed):
    t = load_tile()
    rnd = random.Random(seed)
    n = rnd.randint(3, 5)
    blobs = [(rnd.randint(6, 33), rnd.randint(4, 26), rnd.randint(4, 7)) for _ in range(n)]
    for x in range(40):
        for y in range(40):
            if not is_base(t, x, y):
                continue
            best = 99
            for cx, cy, r in blobs:
                d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5 - r + rnd.uniform(-1.4, 1.4)
                best = min(best, d)
            if best < -1.5:
                put(t, x, y, M_MID)
            elif best < 0.4:
                put(t, x, y, M_DARK)
    for x in range(40):
        for y in range(40):
            if t.getpixel((x, y)) == M_MID and rnd.random() < 0.16:
                t.putpixel((x, y), M_LIGHT)
    return t


# ---- beveled bricks: each brick embossed like the tile frame ----
def brick_beveled(seed=7, bh=9, bw=18):
    t = load_tile()
    for r, ytop in enumerate(range(0, 40, bh)):
        mortar_y = ytop + bh - 1            # mortar row below this course
        body_top, body_bot = ytop, mortar_y - 1
        # horizontal mortar
        for x in range(40):
            put(t, x, mortar_y, DARKER)
        offset = bw // 2 if r % 2 else 0
        # vertical mortar columns for this course
        verts = sorted(set([offset + k * bw for k in range(-1, 4)]))
        for vx in verts:
            for y in range(body_top, body_bot + 1):
                put(t, vx, y, DARKER)
        # emboss each brick body between consecutive vertical mortars
        for i in range(len(verts) - 1):
            L, R = verts[i] + 1, verts[i + 1] - 1
            if R < L:
                continue
            for x in range(L, R + 1):
                put(t, x, body_top, LIGHT)      # top edge highlight
                put(t, x, body_bot, DARK)       # bottom edge shadow
            for y in range(body_top, body_bot + 1):
                put(t, L, y, LIGHT)             # left edge highlight
                put(t, R, y, DARK)              # right edge shadow
    return t


def brick_flat(seed=7):  # the original flat version, for comparison
    t = load_tile()
    course_h = 8
    row = 0
    for y in range(2, 39, course_h):
        for x in range(40):
            put(t, x, y, DARK)
            put(t, x, y + 1, DARKER)
            put(t, x, y + 2, LIGHT)
        offset = course_h if row % 2 else 0
        for x in range(offset, 40, 16):
            for yy in range(y + 2, y + course_h):
                put(t, x, yy, DARK)
        row += 1
    return t


# generate
moss_seeds = [4, 11, 23, 37, 50, 64]
moss = {f"solid_mossy_s{s}": mossy(s) for s in moss_seeds}
bricks = {"solid_brick_flat": brick_flat(), "solid_brick_beveled": brick_beveled()}
for name, img in {**moss, **bricks}.items():
    img.save(f"/tmp/{name}.png")


def contact(items, path, scale=6, cols=None):
    cols = cols or len(items)
    rows = (len(items) + cols - 1) // cols
    pad = 12
    cw, ch = 40 * scale + pad, 40 * scale + pad + 16
    sheet = Image.new("RGBA", (cols * cw + pad, rows * ch + pad), (38, 38, 38, 255))
    d = ImageDraw.Draw(sheet)
    for i, (name, img) in enumerate(items):
        r, c = divmod(i, cols)
        x, y = pad + c * cw, pad + r * ch
        d.text((x + 2, y), name, fill=(235, 235, 235, 255))
        sheet.paste(img.resize((40 * scale, 40 * scale), Image.NEAREST), (x, y + 14),
                    img.resize((40 * scale, 40 * scale), Image.NEAREST))
    sheet.save(path)


contact([("original", load_tile())] + list(moss.items()), "/tmp/moss_contact.png", cols=4)
contact([("original", load_tile())] + list(bricks.items()), "/tmp/brick_contact.png")
print("done")
