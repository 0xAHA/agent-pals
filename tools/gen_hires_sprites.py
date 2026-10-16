"""Generates the 32x40 (density 2) character themes and writes themes/<name>/sheet.png + theme.json.

Same layout as gen_placeholder_sprites.py: 4 frames per row, rows = idle, walk, think, work, wait,
talk, done, facing right. At density 2 a frame fills the same 16x20 logical slot as the classic
themes, at twice the detail, so both kinds mix freely in random mode.

Swap-key colours are palette-swapped per agent at runtime: they must stay exact, and must not be
reused for anything that should keep its colour.

    python tools/gen_hires_sprites.py [--preview out.png]
"""
import json
import math
import sys
from pathlib import Path

from PIL import Image

W, H, FRAMES, DENSITY = 32, 40, 4, 2
ANIMS = ["idle", "walk", "think", "work", "wait", "talk", "done"]
# Alternates the app picks between per pal (theme.json "work_2" etc. group under "work").
VARIANTS = ["idle_2", "idle_3", "work_2", "work_3"]
FPS = {"idle": 3, "walk": 8, "think": 3, "work": 6, "wait": 5, "talk": 5, "done": 6}
Y0 = 3  # feet land on sheet row 37, just above the shadow the app draws on logical row 19
OUTLINE = (34, 32, 52)
DARK = (30, 30, 36)
WHITE = (245, 245, 250)
MOUTH = (150, 50, 55)
STEEL = (170, 172, 184)
WOOD = (130, 86, 50)

SKIN = (241, 194, 125)
HAIR = (90, 58, 34)
SKIN_SWAP = (SKIN, ["#f1c27d", "#e0ac69", "#c68642", "#8d5524", "#ffdbac"])
HAIR_SWAP = (HAIR, ["#5a3a22", "#1e1a1a", "#c8a050", "#a0522d", "#8c8c96", "#d26e2d"])


def hx(c):
    return "#%02x%02x%02x" % c


class Frame:
    def __init__(self, name, f):
        self.img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        self.px = self.img.load()
        anim, _, n = name.partition("_")
        self.anim, self.var, self.f = anim, int(n or 1), f
        self.speaking = False  # mouth animates as in talk, e.g. on a phone call
        self.look = 0  # eye offset: -1 glance back, +1 glance ahead
        self.down = 0  # eye offset downwards
        self.jump = [0, -4, -6, -2][f] if anim == "done" else 0
        self.bob = {"idle": [0, 0, 1, 0], "walk": [0, 1, 0, 1], "think": [0, 0, 0, 1]}.get(anim, [0] * 4)[f]
        self.stride = anim == "walk" and f % 2 == 0
        self.upper()

    def upper(self):
        self.dy = Y0 + self.jump + self.bob

    def lower(self):
        self.dy = Y0 + self.jump

    def p(self, x, y, c):
        y += self.dy
        if 0 <= x < W and 0 <= y < H:
            self.px[x, y] = (*c, 255)

    def r(self, x0, y0, x1, y1, c):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                self.p(x, y, c)

    def clear(self, x0, y0, x1, y1):
        for y in range(y0 + self.dy, y1 + 1 + self.dy):
            for x in range(x0, x1 + 1):
                if 0 <= x < W and 0 <= y < H:
                    self.px[x, y] = (0, 0, 0, 0)

    def rr(self, x0, y0, x1, y1, c):
        """Rect with its four corner pixels left out."""
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                if x in (x0, x1) and y in (y0, y1):
                    continue
                self.p(x, y, c)

    def line(self, x0, y0, x1, y1, c, thick=2):
        n = max(abs(x1 - x0), abs(y1 - y0), 1)
        for i in range(n + 1):
            x = round(x0 + (x1 - x0) * i / n)
            y = round(y0 + (y1 - y0) * i / n)
            self.r(x, y, x + thick - 1, y + thick - 1, c)

    def poly(self, pts, c, thick=2):
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            self.line(x0, y0, x1, y1, c, thick)

    def ellipse(self, cx, cy, rx, ry, c):
        for y in range(cy - ry, cy + ry + 1):
            for x in range(cx - rx, cx + rx + 1):
                if ((x - cx) / (rx + 0.5)) ** 2 + ((y - cy) / (ry + 0.5)) ** 2 <= 1:
                    self.p(x, y, c)

    def outlined(self):
        """1px outline so pals read on any wallpaper."""
        src = self.img.copy().load()
        for y in range(H):
            for x in range(W):
                if src[x, y][3]:
                    continue
                if any(0 <= x + dx < W and 0 <= y + dy < H and src[x + dx, y + dy][3]
                       for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                    self.px[x, y] = (*OUTLINE, 255)
        return self.img


class Humanoid:
    """Chibi figure facing right. Subclasses dress it by overriding the hooks."""
    skin, hair = SKIN, HAIR
    top = (58, 110, 165)
    sleeve = None  # defaults to top
    hand = None  # defaults to skin
    legs_c = (47, 53, 66)
    shoes = DARK
    swaps = [SKIN_SWAP, HAIR_SWAP]

    def draw(self, F):
        self.stance(F)
        F.upper(); self.back(F)
        F.upper(); self.back_arm(F)
        F.lower(); self.legs(F)
        F.upper(); self.torso(F)
        self.head(F)
        self.front_arm(F)
        return F.outlined()

    def stance(self, F):
        """Set per-frame flags before drawing. Generic idle variants live here."""
        a, v, f = F.anim, F.var, F.f
        if a == "idle" and v == 2:  # glance down at a watch
            F.down = 1 if f in (1, 2) else 0
        elif a == "idle" and v == 3:  # look back, then ahead
            F.look = -1 if f < 2 else 1

    def back(self, F):
        pass

    def arm(self, F, x, y0, y1, hand):
        F.r(x, y0, x + 1, y1, self.sleeve or self.top)
        self.paw(F, *hand)

    def paw(self, F, x, y):
        F.r(x, y, x + 1, y + 1, self.hand or self.skin)

    def reach(self, F, hand):
        """Arm from the front shoulder to `hand`, any angle."""
        F.line(20, 16, hand[0], hand[1], self.sleeve or self.top)
        self.paw(F, *hand)

    def back_arm(self, F):
        a, f = F.anim, F.f
        if a == "work":
            self.work_back(F)
        elif a == "done":
            self.arm(F, 10, 9, 15, (10, 7))
        elif a == "talk":
            self.arm(F, 10, 16, 21, (9 - f % 2, 22))
        else:
            swing = f % 2 if a == "walk" else 0
            self.arm(F, 10, 16, 22, (10, 23 - swing))

    def work_back(self, F):
        self.arm(F, 10, 16, 22, (10, 23))

    def front_arm(self, F):
        a, f = F.anim, F.f
        if a == "done":
            self.arm(F, 20, 9, 15, (20, 7))
        elif a == "idle" and F.var == 2:
            self.arm(F, 20, 16, 18, (18, 19))
        elif a == "think":
            self.arm(F, 20, 16, 17, (19, 14))
        elif a == "wait":
            self.arm(F, 20, 9, 16, (20 + f % 2, 7))
        elif a == "talk":
            self.arm(F, 20, 16, 21, (21 + (f + 1) % 2, 22))
        elif a == "work":
            self.work(F)
        else:
            swing = f % 2 if a == "walk" else 0
            self.arm(F, 20, 16, 22, (20, 22 + swing))

    def work(self, F):
        getattr(self, f"work{F.var}")(F)

    # Families override these; a family without its own variants repeats work1.
    def work2(self, F):
        self.work1(F)

    def work3(self, F):
        self.work1(F)

    def work1(self, F):
        """Default: typing on a laptop held at the waist."""
        f = F.f
        F.r(9, 22, 23, 24, (160, 166, 180))
        for x in range(10, 23):
            F.p(x, 22, (120, 220, 255) if (x + f) % 3 else (200, 245, 255))
        self.arm(F, 20, 16, 19, (20, 20 + f % 2))
        self.paw(F, 13, 21 - f % 2)

    def legs(self, F):
        c, s = self.legs_c, self.shoes
        F.r(12, 25, 19, 26, c)
        if F.stride:
            F.r(11, 27, 13, 32, c); F.r(18, 27, 20, 32, c)
            F.r(10, 33, 13, 34, s); F.r(18, 33, 21, 34, s)
        else:
            F.r(12, 27, 14, 32, c); F.r(17, 27, 19, 32, c)
            F.r(12, 33, 15, 34, s); F.r(17, 33, 20, 34, s)
            if F.anim == "idle" and F.var == 3 and F.f % 2:  # foot tap
                F.clear(17, 32, 20, 34)
                F.r(17, 31, 19, 31, c); F.r(17, 32, 20, 33, s)

    def torso(self, F):
        F.r(12, 15, 19, 24, self.top)
        self.torso_detail(F)

    def torso_detail(self, F):
        pass

    def head(self, F):
        F.rr(11, 5, 20, 14, self.skin)
        self.face(F)
        self.headwear(F)

    def eyes(self, F, c=DARK, y=9):
        x, y = 16 + F.look, y + F.down
        if F.anim == "idle" and F.var == 1 and F.f == 3:  # blink
            F.p(x, y + 1, c); F.p(x + 3, y + 1, c)
        else:
            F.r(x, y, x, y + 1, c); F.r(x + 3, y, x + 3, y + 1, c)

    def chatty(self, F):
        return (F.anim in ("talk", "wait") or F.speaking) and F.f % 2 == 0

    def mouth(self, F):
        if self.chatty(F):
            F.r(17, 12, 18, 13, MOUTH)
        elif F.anim == "done":
            F.r(17, 13, 18, 13, MOUTH); F.p(16, 12, MOUTH); F.p(19, 12, MOUTH)
        else:
            F.r(17, 12, 18, 12, MOUTH)

    def face(self, F):
        self.eyes(F)
        self.mouth(F)

    def headwear(self, F):
        h = self.hair
        F.r(12, 3, 19, 3, h); F.r(11, 4, 20, 6, h)
        F.r(11, 7, 12, 10, h); F.p(20, 7, h)


class Heroes(Humanoid):
    """Caped hero: one-colour suit, cape, gloves and boots, domino mask."""
    SUIT = (200, 50, 50)
    CAPE = (40, 60, 150)
    top = legs_c = SUIT
    shoes = hand = CAPE
    BELT = (230, 190, 40)
    CHEST = (255, 232, 96)
    swaps = [
        (SUIT, ["#c83232", "#2850c8", "#2d9646", "#7832b4", "#2a2a32", "#e6e6ee", "#e67828"]),
        (CAPE, ["#283c96", "#b42828", "#e6c828", "#1e1e24", "#d2d2dc", "#28a0a0"]),
        SKIN_SWAP, HAIR_SWAP,
    ]

    def back(self, F):
        a, f, c = F.anim, F.f, self.CAPE
        if a == "done":
            F.r(8, 15, 12, 26, c); F.r(7, 17, 7, 24, c)
            return
        F.r(9, 15, 12, 31, c)
        if a == "work":
            F.r(6, 18, 8, 29, c); F.r(5, 21, 5, 27, c)
        elif a == "walk":
            if f % 2 == 0:
                F.r(7, 21, 8, 31, c)
            else:
                F.r(8, 24, 8, 31, c)

    def torso_detail(self, F):
        F.r(12, 24, 19, 24, self.BELT)
        F.r(15, 17, 16, 17, self.CHEST); F.r(14, 18, 17, 18, self.CHEST); F.r(15, 19, 16, 19, self.CHEST)

    def face(self, F):
        F.r(13, 8, 20, 10, self.CAPE)
        self.eyes(F, WHITE)
        self.mouth(F)

    def stance(self, F):
        super().stance(F)
        if F.anim == "work" and F.var == 3:
            F.stride = True

    def work_back(self, F):
        if F.var == 3:
            self.arm(F, 10, 5, 15, (10, 3))
        else:
            super().work_back(F)

    def work2(self, F):
        """Punch combo: jab, recoil, uppercut, guard."""
        f, spark = F.f, (255, 235, 110)
        if f == 0:
            F.r(20, 17, 25, 18, self.SUIT); F.r(26, 17, 27, 18, self.CAPE)
            F.p(29, 16, spark); F.p(30, 18, spark); F.p(29, 20, spark)
        elif f == 1:
            self.arm(F, 20, 16, 20, (20, 21))
        elif f == 2:
            self.reach(F, (23, 9))
            F.p(25, 7, spark); F.p(26, 9, spark); F.p(24, 5, spark)
        else:
            self.arm(F, 20, 15, 17, (20, 13))

    def work3(self, F):
        """Holding a boulder overhead, straining."""
        sag = [0, 1, 0, 1][F.f]
        self.arm(F, 20, 5, 15, (20, 3))
        F.ellipse(15, -1 + sag, 8, 3, (128, 126, 136))
        F.r(10, -2 + sag, 12, -2 + sag, (160, 158, 168))

    def work1(self, F):
        """Energy blast from an outstretched fist."""
        f = F.f
        F.r(20, 17, 25, 18, self.SUIT)
        F.r(26, 17, 27, 18, self.CAPE)
        glow = (255, 245, 150)
        size = [1, 2, 3, 2][f]
        F.r(28, 18 - size, 28 + size, 17 + size, glow)
        if size > 1:
            F.r(29, 18 - size + 1, 28 + size - 1, 17 + size - 1, WHITE)


class SpaceKnights(Humanoid):
    """Hooded robe, belt, and an energy blade drawn when working."""
    ROBE = (120, 86, 52)
    BLADE = (70, 150, 255)
    top = legs_c = ROBE
    BELT = (60, 40, 25)
    SHADOW = (45, 35, 35)
    swaps = [
        (ROBE, ["#785634", "#2a2426", "#d8ccb0", "#5a5a64", "#3c2a46"]),
        (BLADE, ["#4696ff", "#46d25a", "#e63c3c", "#a05ae6", "#f0c83c"]),
        SKIN_SWAP,
    ]

    def legs(self, F):
        R = self.ROBE
        F.r(11, 25, 20, 31, R)
        if F.stride:
            F.r(10, 32, 13, 33, R); F.r(17, 32, 21, 33, R)
            F.r(10, 34, 12, 34, DARK); F.r(19, 34, 21, 34, DARK)
        else:
            F.r(10, 32, 21, 33, R)
            F.r(12, 34, 14, 34, DARK); F.r(17, 34, 19, 34, DARK)

    def torso_detail(self, F):
        F.r(16, 16, 16, 21, self.SHADOW)
        F.r(12, 22, 19, 22, self.BELT)
        F.r(15, 22, 16, 22, STEEL)
        if F.anim != "work":
            F.r(13, 23, 14, 26, STEEL)

    def headwear(self, F):
        R = self.ROBE
        F.r(12, 2, 19, 2, R); F.r(10, 3, 21, 6, R)
        F.r(10, 7, 12, 16, R); F.r(21, 5, 21, 10, R)
        F.r(13, 7, 20, 7, self.SHADOW)

    def stance(self, F):
        super().stance(F)
        if F.anim == "work" and F.var in (2, 3):
            F.stride = True

    def blade(self, F, hand, start, end, glow=True):
        self.reach(F, hand)
        F.line(*start, *end, self.BLADE)
        if glow:
            F.line(*start, *end, WHITE, 1)
        F.r(hand[0] + 1, hand[1], hand[0] + 2, hand[1] + 1, STEEL)

    def work2(self, F):
        """Guard stance: blade upright, humming."""
        self.blade(F, (21, 17), (22, 15), (22, 2 - F.f % 2), glow=F.f % 2 == 0)

    def work3(self, F):
        """Lunge: draw back, thrust, hold, recover."""
        hx = [19, 23, 23, 21][F.f]
        self.blade(F, (hx, 18), (hx + 3, 18), (hx + 9 + (F.f in (1, 2)), 18))

    def work1(self, F):
        """Blade swing: overhead, diagonal, level, low."""
        hand, start, end = [
            ((20, 10), (21, 8), (21, -1)),
            ((21, 12), (23, 10), (30, 3)),
            ((21, 17), (24, 17), (31, 17)),
            ((21, 19), (23, 21), (29, 27)),
        ][F.f]
        self.reach(F, hand)
        F.line(*start, *end, self.BLADE)
        F.line(*start, *end, WHITE, 1)
        F.r(hand[0] + 1, hand[1], hand[0] + 2, hand[1] + 1, STEEL)


class StarshipCrew(Humanoid):
    """Duty-coloured tunic with a dark yoke, and a handheld scanner."""
    TUNIC = (212, 168, 42)
    YOKE = (28, 28, 34)
    top = TUNIC
    legs_c = YOKE
    shoes = (14, 14, 18)
    swaps = [
        (TUNIC, ["#d4a82a", "#2a6ed4", "#c8282d", "#2a9a8a", "#7a4fb4"]),
        SKIN_SWAP, HAIR_SWAP,
    ]

    def torso_detail(self, F):
        F.r(12, 15, 19, 16, self.YOKE)
        F.r(17, 18, 18, 19, (205, 205, 215))

    def stance(self, F):
        super().stance(F)
        if F.anim == "work" and F.var == 3:
            F.speaking = True

    def work2(self, F):
        """Tapping at a wall console with blinking lights."""
        f = F.f
        F.lower()
        F.r(24, 11, 29, 27, (60, 62, 70))
        F.r(25, 12, 28, 15, [(80, 180, 255), (80, 220, 140)][f % 2])
        lights = [(230, 70, 70), (240, 200, 70), (80, 220, 140), (80, 180, 255)]
        for i, (x, y) in enumerate([(25, 18), (27, 18), (25, 21), (27, 21)]):
            F.r(x, y, x + 1, y + 1, lights[(i + f) % 4])
        F.upper()
        self.reach(F, (22, 17 + (f % 2) * 3))

    def work3(self, F):
        """Calling in on the badge communicator."""
        self.reach(F, (17, 17))
        if F.f % 2 == 0:
            wave = (140, 210, 255)
            for x, y in ((22, 8), (23, 9), (23, 10), (22, 11), (24, 7), (25, 9), (25, 10), (24, 12)):
                F.p(x, y, wave)

    def work1(self, F):
        f = F.f
        F.r(22, 17, 26, 21, (70, 72, 80))
        F.r(23, 18, 25, 19, [(80, 220, 140), (80, 180, 255), (80, 220, 140), (255, 200, 80)][f])
        F.p(26, 16, (230, 60, 60) if f % 2 else (90, 30, 30))
        self.reach(F, (21, 20))


class RescuePups(Humanoid):
    """Upright pup in a coloured vest and helmet, with a wagging tail and a shovel."""
    FUR = (200, 150, 90)
    VEST = (210, 50, 50)
    top = VEST
    sleeve = hand = legs_c = shoes = FUR
    MUZZLE = (240, 226, 200)
    EAR = (100, 66, 40)
    NOSE = (25, 22, 25)
    BADGE = (240, 200, 60)
    TONGUE = (230, 110, 130)
    swaps = [
        (FUR, ["#c8965a", "#7a5032", "#ebe1d2", "#323032", "#dcaa46", "#9a9aa0"]),
        (VEST, ["#d23232", "#2d64d2", "#32a046", "#f0821e", "#e65aa0", "#f0c828", "#7a46c8"]),
    ]

    def back(self, F):
        c = self.FUR
        if F.f % 2:
            F.r(9, 22, 11, 23, c); F.r(7, 21, 8, 22, c)
        else:
            F.r(9, 21, 11, 22, c); F.r(8, 18, 9, 20, c)

    def torso_detail(self, F):
        F.r(15, 18, 16, 19, self.BADGE)

    def head(self, F):
        F.rr(11, 5, 20, 15, self.FUR)
        F.r(19, 10, 23, 14, self.MUZZLE)
        F.r(23, 10, 24, 11, self.NOSE)
        F.r(11, 6, 13, 13, self.EAR)
        self.eyes(F, y=8)
        if self.chatty(F) or (F.anim == "done" and F.f % 2 == 0):
            F.r(20, 14, 22, 14, self.NOSE); F.p(21, 15, self.TONGUE)
        F.rr(12, 1, 20, 5, self.VEST)
        F.r(18, 5, 22, 5, self.VEST)

    def stance(self, F):
        super().stance(F)
        if F.anim == "work" and F.var == 3:
            F.speaking = True

    def work_back(self, F):
        if F.var == 2:
            self.arm(F, 10, 11, 15, (12, 10))
        else:
            super().work_back(F)

    def work2(self, F):
        """Scanning the horizon through binoculars."""
        self.reach(F, (20, 10))
        F.r(21, 7, 24, 10, (40, 40, 48))
        if F.f % 2:
            F.p(24, 8, (200, 230, 255))

    def work3(self, F):
        """Radioing in on a walkie-talkie."""
        dark = (40, 40, 48)
        F.r(24, 7, 26, 13, dark)
        F.r(25, 4, 25, 6, dark)
        F.p(25, 8, (240, 60, 60) if F.f % 2 else (110, 30, 30))
        self.reach(F, (23, 12))

    def work1(self, F):
        """Digging: shovel up, up, into the ground, flick dirt."""
        f = F.f
        if f < 2:
            top = 2 + 2 * f
            F.line(22, top + 3, 22, top + 12, WOOD, 1)
            F.r(21, top, 23, top + 2, STEEL)
            self.reach(F, (21, top + 7))
        else:
            F.line(23, 18, 27, 27, WOOD, 1)
            F.r(26, 28, 29, 31, STEEL)
            self.reach(F, (22, 20))
            if f == 3:
                dirt = (110, 80, 50)
                F.p(29, 26, dirt); F.p(30, 24, dirt); F.p(28, 24, dirt); F.p(31, 22, dirt)


class PaperCo(Humanoid):
    """Shirt and tie office worker, stamping paperwork."""
    SHIRT = (232, 232, 240)
    TIE = (180, 40, 45)
    top = SHIRT
    legs_c = (62, 64, 74)
    shoes = (40, 30, 26)
    COLLAR = (252, 252, 255)
    PAPER = (250, 248, 238)
    INK = (200, 40, 40)
    swaps = [
        (SHIRT, ["#e8e8f0", "#a8c8e8", "#f0e6a8", "#f0c0c8", "#b8e0c8"]),
        (TIE, ["#b4282d", "#28327a", "#2d7a3c", "#6e2d82", "#c89628"]),
        SKIN_SWAP, HAIR_SWAP,
    ]

    def torso_detail(self, F):
        F.r(15, 15, 16, 15, self.TIE)
        F.r(15, 16, 16, 23, self.TIE)
        F.p(14, 15, self.COLLAR); F.p(17, 15, self.COLLAR)
        F.r(12, 24, 19, 24, self.shoes)

    def headwear(self, F):
        h = self.hair
        F.r(12, 3, 19, 3, h); F.r(11, 4, 20, 5, h)
        F.r(11, 6, 12, 9, h)

    def stance(self, F):
        super().stance(F)
        if F.anim == "work" and F.var == 3:
            F.speaking = True

    def work_back(self, F):
        if F.var == 3:  # gesturing while on the phone
            self.arm(F, 10, 16, 21, (9 - F.f % 2, 22))
        else:
            super().work_back(F)

    def work2(self, F):
        Humanoid.work1(self, F)

    def work3(self, F):
        """On the desk phone."""
        F.r(20, 8, 21, 13, (35, 35, 40))
        self.reach(F, (20, 12))

    def work1(self, F):
        f = F.f
        F.r(9, 21, 23, 22, self.PAPER)
        for x in range(11, 20, 2):
            F.p(x, 21, (170, 170, 180))
        if f == 3:
            F.r(19, 21, 21, 21, self.INK)
        hand_y = [12, 15, 18, 13][f]
        F.r(20, hand_y - 3, 23, hand_y - 2, (60, 40, 40))
        F.r(21, hand_y - 5, 22, hand_y - 4, WOOD)
        self.reach(F, (21, hand_y))


class Undersea:
    """Round-bodied octopus pal. Not a Humanoid: tentacles for legs and arms."""
    BODY = (230, 110, 160)
    swaps = [(BODY, ["#e66ea0", "#965adc", "#f08c3c", "#3cb4b4", "#dc4646", "#6ec85a"])]

    def draw(self, F):
        a, f, B = F.anim, F.f, self.BODY
        F.lower()
        amp = 2 if a == "walk" else 1
        for i, bx in enumerate((10, 13, 16, 19, 22)):
            for y in range(24, 35):
                dx = round(amp * math.sin(y * 0.7 + f * 1.6 + i * 1.3) * (y - 24) / 10)
                F.r(bx + dx, y, bx + dx + 1, y, B)
        F.upper()
        self.arms(F)
        F.ellipse(16, 16, 8, 10, B)
        F.r(11, 9, 12, 10, WHITE)
        if a == "idle" and F.var == 1 and f == 3:
            F.r(15, 15, 17, 15, DARK); F.r(19, 15, 21, 15, DARK)
        else:
            # Pupils look ahead, except down at the book and up at the juggling.
            py = 15 if (a == "work" and F.var == 3) else 13 if (a == "work" and F.var == 2) else 14
            F.r(15, 13, 17, 16, WHITE); F.r(19, 13, 21, 16, WHITE)
            F.r(16, py, 17, py + 1, DARK); F.r(20, py, 21, py + 1, DARK)
        v, bub = F.var, (190, 235, 255)
        if (a in ("talk", "wait") and f % 2 == 0) or (a == "idle" and v == 2):
            F.r(18, 19, 20, 20, MOUTH)
        elif a == "done":
            F.r(18, 20, 20, 20, MOUTH); F.p(17, 19, MOUTH); F.p(21, 19, MOUTH)
        else:
            F.r(18, 19, 20, 19, MOUTH)
        if a == "idle" and v == 2:  # blowing a bubble that grows and floats off
            s = [0, 1, 1, 2][f]
            F.r(21, 17 - 3 * f, 21 + s, 17 - 3 * f + s, bub)
        elif a == "work" and v == 1:  # laptop
            F.r(12, 24, 28, 26, (160, 166, 180))
            F.r(13, 24, 27, 24, (120, 220, 255) if f % 2 else (200, 245, 255))
            F.poly([(9, 20), (12, 23 - f % 2)], B)
            F.poly([(23, 20), (24, 22 + f % 2)], B)
        elif a == "work" and v == 2:  # juggling
            arc = [(4, 7), (9, 1), (21, 1), (26, 7)]
            for i, c in enumerate([(255, 200, 90), (120, 220, 255), (255, 120, 160)]):
                x, y = arc[(f + i) % 4]
                F.r(x, y, x + 2, y + 2, c)
        elif a == "work" and v == 3:  # reading, turning a page
            F.r(19, 19, 28, 24, (90, 60, 140))
            F.r(20, 19, 27, 20, (250, 248, 238))
            if f == 2:
                F.r(23, 16, 24, 19, (250, 248, 238))
            F.poly([(23, 20), (24, 23)], B)
        if (a == "work" and v == 1) or a == "think":
            F.r(26, 10 - 2 * f, 27, 11 - 2 * f, bub)
            F.p(28, 5 - f, bub)
        return F.outlined()

    def arms(self, F):
        a, f, v, B = F.anim, F.f, F.var, self.BODY
        if a == "work" and v in (1, 3):
            return
        if a == "work" and v == 2:
            F.poly([(9, 16), (6, 11), (5 + f % 2, 9)], B)
            F.poly([(23, 16), (26, 11), (27 - f % 2, 9)], B)
            return
        if a == "idle" and v == 3:  # little wave
            F.poly([(9, 20), (6, 25), (7, 28)], B)
            F.poly([(23, 18), (26, 14), (27 - f % 2, 11)], B)
            return
        if a == "done":
            F.poly([(9, 14), (6, 9), (5, 5)], B)
            F.poly([(23, 14), (26, 9), (27, 5)], B)
            return
        sway = f % 2
        F.poly([(9, 20), (6, 25), (7 - sway, 28)], B)
        if a == "think":
            F.poly([(23, 20), (25, 23), (21, 23)], B)
        elif a == "wait":
            F.poly([(23, 16), (26, 10), (27 - sway, 6)], B)
        elif a == "talk":
            F.poly([(23, 20), (27, 22 - 2 * sway)], B)
        else:
            F.poly([(23, 20), (26, 25), (25 + sway, 28)], B)


THEMES = {
    "heroes": Heroes(),
    "space-knights": SpaceKnights(),
    "starship-crew": StarshipCrew(),
    "undersea": Undersea(),
    "rescue-pups": RescuePups(),
    "paper-co": PaperCo(),
}


ROWS = ANIMS + VARIANTS


def sheet_for(character):
    sheet = Image.new("RGBA", (W * FRAMES, H * len(ROWS)), (0, 0, 0, 0))
    for r, a in enumerate(ROWS):
        for f in range(FRAMES):
            sheet.paste(character.draw(Frame(a, f)), (f * W, r * H))
    return sheet


def theme_json(name, character):
    return {
        "name": name,
        "frame_w": W,
        "frame_h": H,
        "density": DENSITY,
        "sheet": "sheet.png",
        "anims": {a: {"row": r, "frames": FRAMES, "fps": FPS[a.partition("_")[0]]} for r, a in enumerate(ROWS)},
        "swaps": [{"key": hx(k), "choices": c} for k, c in character.swaps],
    }


def main():
    root = Path(__file__).resolve().parent.parent / "themes"
    sheets = []
    for name, ch in THEMES.items():
        sheet = sheet_for(ch)
        out = root / name
        out.mkdir(parents=True, exist_ok=True)
        sheet.save(out / "sheet.png")
        (out / "theme.json").write_text(json.dumps(theme_json(name, ch), indent=2) + "\n", encoding="utf-8")
        sheets.append(sheet)
    if "--preview" in sys.argv:
        # All sheets side by side on a mid-grey card, scaled up for eyeballing.
        gap, z = 8, 3
        card = Image.new("RGBA", (sum(s.width for s in sheets) + gap * (len(sheets) + 1), sheets[0].height + 2 * gap),
                         (128, 132, 140, 255))
        x = gap
        for s in sheets:
            card.alpha_composite(s, (x, gap))
            x += s.width + gap
        card.resize((card.width * z, card.height * z), Image.NEAREST).save(sys.argv[sys.argv.index("--preview") + 1])


if __name__ == "__main__":
    main()
