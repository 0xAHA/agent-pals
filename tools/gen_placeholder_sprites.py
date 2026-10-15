"""Generates placeholder sprite sheets for themes/office and themes/construction.
Swap-key colours must stay exact (they're palette-swapped per agent at runtime).
Replace sheet.png with real art (Aseprite/LibreSprite) using the same layout:
16x20 frames, 4 frames per row, rows = idle, walk, think, work, wait, talk, done. Facing right."""
from PIL import Image
from pathlib import Path

FW, FH, FRAMES = 16, 20, 4
ANIMS = ["idle", "walk", "think", "work", "wait", "talk", "done"]
Y0 = 1
OUTLINE = (34, 32, 52, 255)
DARK = (30, 30, 36)

THEMES = {
    "office": dict(head=(90, 58, 34), skin=(241, 194, 125), shirt=(58, 110, 165),
                   pants=(47, 53, 66), shoes=DARK, hat=False),
    "construction": dict(head=(255, 200, 40), skin=(241, 194, 125), shirt=(255, 120, 30),
                         pants=(60, 70, 110), shoes=(80, 50, 30), hat=True),
}


def frame(t, anim, f):
    img = Image.new("RGBA", (FW, FH), (0, 0, 0, 0))
    px = img.load()
    jump = [0, -2, -3, -1][f] if anim == "done" else 0
    bob = {"idle": [0, 0, 1, 0], "walk": [0, 1, 0, 1], "think": [0, 0, 0, 1]}.get(anim, [0] * 4)[f]

    def put(x, y, c, upper=True):
        yy = y + Y0 + jump + (bob if upper else 0)
        if 0 <= x < FW and 0 <= yy < FH:
            px[x, yy] = (*c, 255)

    # legs + shoes (not bobbed)
    step = anim == "walk" and f % 2 == 0
    for x in range(6, 10): put(x, 13, t["pants"], False)
    legs = [(5, 6), (9, 10)] if step else [(6, 7), (8, 9)]
    for y in (14, 15):
        for a, b in legs:
            put(a, y, t["pants"], False); put(b, y, t["pants"], False)
    shoes = [(4, 6), (9, 11)] if step else [(5, 7), (8, 10)]
    for a, b in shoes:
        for x in range(a, b + 1): put(x, 16, t["shoes"], False)

    # torso
    for y in range(8, 13):
        for x in range(5, 11): put(x, y, t["shirt"])
    if t["hat"]:
        for x in range(5, 11): put(x, 10, (230, 230, 230))  # hi-vis stripe

    # head
    for y in range(4, 8):
        for x in range(6, 10): put(x, y, t["skin"])
    put(7, 5, DARK); put(9, 5, DARK)
    if anim in ("talk", "wait") and f % 2 == 0: put(8, 7, (150, 40, 40))
    for x in range(6, 10): put(x, 2, t["head"])
    for x in range(5, 11): put(x, 3, t["head"])
    if t["hat"]:
        for x in range(4, 12): put(x, 4, t["head"])  # brim
    else:
        put(5, 4, t["head"]); put(10, 4, t["head"])

    def arm(x, rows, hand):
        for y in rows: put(x, y, t["shirt"])
        put(hand[0], hand[1], t["skin"])

    L, R = 4, 11
    if anim == "think":
        arm(L, [9, 10, 11], (L, 12)); arm(R, [9], (10, 7))
    elif anim == "work":
        if t["hat"]:  # hammer swing
            arm(L, [9, 10, 11], (L, 12))
            if f < 2:
                arm(R, [8, 7], (12, 6)); put(12, 5, (140, 90, 50)); put(12, 4, (120, 120, 130)); put(13, 4, (120, 120, 130))
            else:
                arm(R, [9, 10], (12, 11)); put(13, 11, (140, 90, 50)); put(14, 11, (120, 120, 130)); put(14, 12, (120, 120, 130))
        else:  # laptop typing
            for x in range(4, 12):
                for y in (11, 12): put(x, y, (160, 166, 180))
            for x in range(5, 11): put(x, 11, (120, 220, 255) if (x + f) % 3 else (200, 245, 255))
            put(L, 10 + f % 2, t["skin"]); put(R, 11 - f % 2, t["skin"])
    elif anim == "wait":
        arm(L, [9, 10, 11], (L, 12)); arm(R, [7, 6, 5], (R + f % 2, 4))
    elif anim == "talk":
        arm(L, [9, 10], (L - (f % 2), 11)); arm(R, [9, 10], (R + ((f + 1) % 2), 11))
    elif anim == "done":
        arm(L, [7, 6, 5], (L, 4)); arm(R, [7, 6, 5], (R, 4))
    else:
        swing = (f % 2) if anim == "walk" else 0
        arm(L, [9, 10, 11], (L, 12 - swing)); arm(R, [9, 10, 11], (R, 11 + swing))

    # 1px outline so pals read on any wallpaper
    src = img.copy().load()
    for y in range(FH):
        for x in range(FW):
            if src[x, y][3]: continue
            if any(0 <= x + dx < FW and 0 <= y + dy < FH and src[x + dx, y + dy][3]
                   for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                px[x, y] = OUTLINE
    return img


def main():
    root = Path(__file__).resolve().parent.parent / "themes"
    for name, t in THEMES.items():
        sheet = Image.new("RGBA", (FW * FRAMES, FH * len(ANIMS)), (0, 0, 0, 0))
        for r, a in enumerate(ANIMS):
            for f in range(FRAMES):
                sheet.paste(frame(t, a, f), (f * FW, r * FH))
        (root / name).mkdir(parents=True, exist_ok=True)
        sheet.save(root / name / "sheet.png")


if __name__ == "__main__":
    main()
