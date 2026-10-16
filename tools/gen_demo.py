"""Builds assets/demo.gif for the README from the shipped sprite sheets, so it never drifts from
what the app actually draws. Each family plays two of its animations side by side.

    python tools/gen_demo.py
"""
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
CELL_W, CELL_H = 32, 40  # sheet pixels at density 2; density-1 themes are doubled to match
GAP, PAD, ZOOM = 6, 8, 3
BG = (31, 36, 48)
SHADOW = (20, 23, 31)
FRAME_MS = 160

# (theme, first anim, second anim): each plays its 4 frames twice.
CAST = [
    ("construction", "work", "wait"),
    ("heroes", "work", "work_2"),
    ("space-knights", "work", "work_3"),
    ("starship-crew", "work_2", "talk"),
    ("undersea", "work_2", "idle_2"),
    ("rescue-pups", "work", "done"),
    ("paper-co", "work_3", "work"),
    ("office", "work", "think"),
]


def frames(theme, anim):
    meta = json.loads((ROOT / "themes" / theme / "theme.json").read_text(encoding="utf-8"))
    sheet = Image.open(ROOT / "themes" / theme / meta.get("sheet", "sheet.png")).convert("RGBA")
    a = meta["anims"][anim]
    fw, fh, up = meta["frame_w"], meta["frame_h"], 2 // meta.get("density", 1)
    for f in range(a["frames"]):
        img = sheet.crop((f * fw, a["row"] * fh, (f + 1) * fw, (a["row"] + 1) * fh))
        yield img.resize((fw * up, fh * up), Image.NEAREST)


def main():
    loops = []
    for theme, a, b in CAST:
        fa, fb = list(frames(theme, a)), list(frames(theme, b))
        loops.append(fa + fa + fb + fb)
    n = max(len(l) for l in loops)
    w = PAD * 2 + len(CAST) * CELL_W + (len(CAST) - 1) * GAP
    h = PAD * 2 + CELL_H
    out = []
    for t in range(n):
        canvas = Image.new("RGBA", (w, h), (*BG, 255))
        for i, loop in enumerate(loops):
            x = PAD + i * (CELL_W + GAP)
            for dx in range(8, CELL_W - 8):
                canvas.putpixel((x + dx, PAD + CELL_H - 2), (*SHADOW, 255))
            canvas.alpha_composite(loop[t % len(loop)], (x, PAD))
        out.append(canvas.resize((w * ZOOM, h * ZOOM), Image.NEAREST).convert("RGB"))
    # One shared palette keeps colours steady from frame to frame.
    pal = out[0].quantize(colors=255, method=Image.Quantize.MEDIANCUT)
    gif = [f.quantize(palette=pal, dither=Image.Dither.NONE) for f in out]
    dest = ROOT / "assets" / "demo.gif"
    gif[0].save(dest, save_all=True, append_images=gif[1:], duration=FRAME_MS, loop=0, optimize=True)
    print(f"wrote {dest} ({len(gif)} frames, {w * ZOOM}x{h * ZOOM})")


if __name__ == "__main__":
    main()
