"""Builds assets/agent-pals.ico from the construction worker's first idle frame.

Pixel art blurs under smooth resampling, so each icon size is drawn separately: the sprite is
cropped to its bounding box and scaled by the largest whole number that fits, nearest-neighbour.
Where even 1x does not fit (the 16 px size), it is scaled down nearest-neighbour instead.

    python tools/gen_icon.py [--preview out.png]
"""
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SHEET = ROOT / "themes" / "construction" / "sheet.png"
FRAME_W, FRAME_H = 16, 20
SIZES = [16, 24, 32, 48, 64, 128, 256]
MARGIN = 1 / 16  # breathing room around the sprite, as a share of the icon


def sprite():
    frame = Image.open(SHEET).convert("RGBA").crop((0, 0, FRAME_W, FRAME_H))  # idle, frame 0
    return frame.crop(frame.getbbox())


def icon(src, size):
    room = max(1, round(size * (1 - 2 * MARGIN)))
    side = max(src.width, src.height)
    if side <= room:
        k = room // side
        img = src.resize((src.width * k, src.height * k), Image.NEAREST)
    else:
        img = src.resize((max(1, src.width * size // side), max(1, src.height * size // side)), Image.NEAREST)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas.alpha_composite(img, ((size - img.width) // 2, (size - img.height) // 2))
    return canvas


def main():
    src = sprite()
    images = [icon(src, s) for s in SIZES]
    out = ROOT / "assets" / "agent-pals.ico"
    out.parent.mkdir(exist_ok=True)
    biggest = images[-1]
    biggest.save(out, format="ICO", sizes=[(s, s) for s in SIZES], append_images=images[:-1])
    if "--preview" in sys.argv:
        gap = 8
        sheet = Image.new("RGBA", (sum(SIZES) + gap * (len(SIZES) + 1), SIZES[-1] + 2 * gap), (128, 132, 140, 255))
        x = gap
        for im in images:
            sheet.alpha_composite(im, (x, gap + SIZES[-1] - im.height))
            x += im.width + gap
        sheet.save(sys.argv[sys.argv.index("--preview") + 1])
    print(f"wrote {out} ({', '.join(map(str, SIZES))} px)")


if __name__ == "__main__":
    main()
