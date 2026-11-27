#!/usr/bin/env python3
"""Turn the screens rendered by `cargo test ... render_screenshots` into PNGs.

usage: [SCALE=2] render_screenshots.py SHOTS_DIR COVER_PNG OUT_DIR
       render_screenshots.py --gif FRAMES_DIR OUT_GIF

Each SHOTS_DIR/<name>.json (cells + cover position) becomes OUT_DIR/<name>.png:
the terminal drawn cell by cell with JetBrains Mono, box and block characters
drawn as shapes so they join up, the cover pasted where the terminal would
show it, in a rounded window with a soft shadow. --gif joins rendered frames into an
animation (the window only: GIFs can't hold a soft shadow).
"""

import json
import os
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFilter, ImageFont

SCALE = int(os.environ.get("SCALE", "2"))  # pixels per point
FONT_SIZE = 14 * SCALE
LINE = 1.32  # line height, as a multiple of the font size
PAD = 22 * SCALE  # inside the window
MARGIN = 36 * SCALE  # around the window, for the shadow
RADIUS = 14 * SCALE

FONTS = {
    "regular": "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
    "bold": "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Bold.ttf",
    "fallback": "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "fallback2": "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
}

# Rosé Pine
BASE = "#191724"
TEXT = "#e0def4"
NAMED = {
    "Black": "#26233a", "Red": "#eb6f92", "Green": "#31748f", "Yellow": "#f6c177",
    "Blue": "#9ccfd8", "Magenta": "#c4a7e7", "Cyan": "#ebbcba", "Gray": "#908caa",
    "DarkGray": "#6e6a86", "LightRed": "#eb6f92", "LightGreen": "#31748f",
    "LightYellow": "#f6c177", "LightBlue": "#9ccfd8", "LightMagenta": "#c4a7e7",
    "LightCyan": "#ebbcba", "White": "#e0def4",
}

# box drawing: which arms (up, right, down, left) each character has
BOX = {
    "─": "0101", "━": "0101", "│": "1010", "┃": "1010",
    "┌": "0110", "┐": "0011", "└": "1100", "┘": "1001",
    "├": "1110", "┤": "1011", "┬": "0111", "┴": "1101", "┼": "1111",
}
ROUND = {"╭": "0110", "╮": "0011", "╰": "1100", "╯": "1001"}
EIGHTHS = "▁▂▃▄▅▆▇█"


def rgb(c, default):
    if c is None:
        return default
    if c.startswith("#"):
        return c
    return NAMED.get(c, default)


def make_gif(frames_dir, target):
    paths = sorted(Path(frames_dir).glob("*.png"))
    m = int(os.environ.get("MARGIN", 36))  # MARGIN at SCALE=1
    frames = []
    for p in paths:
        im = Image.open(p).convert("RGBA")
        frames.append(im.crop((m, m, im.width - m, im.height - m)))
    w, h = frames[0].size
    # one palette for every frame, from frames spread over the animation
    picks = frames[:: max(1, len(frames) // 4)][:4]
    sample = Image.new("RGB", (w, h * len(picks)))
    for i, f in enumerate(picks):
        sample.paste(f.convert("RGB"), (0, h * i))
    palette = sample.quantize(colors=255, method=0)
    out = []
    for f in frames:
        q = f.convert("RGB").quantize(palette=palette, dither=Image.NONE)
        q.paste(255, mask=f.getchannel("A").point(lambda v: 255 if v < 128 else 0))
        out.append(q)
    out[0].save(target, save_all=True, append_images=out[1:], duration=83, loop=0,
                optimize=True, transparency=255, disposal=1)
    print(target)


def main():
    if sys.argv[1] == "--gif":
        make_gif(sys.argv[2], sys.argv[3])
        return
    shots, cover_path, out = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    out.mkdir(parents=True, exist_ok=True)
    fonts = {k: ImageFont.truetype(v, FONT_SIZE) for k, v in FONTS.items()}
    cmaps = {k: TTFont(v).getBestCmap() for k, v in FONTS.items()}
    cw = round(fonts["regular"].getlength("M"))
    ch = round(FONT_SIZE * LINE)
    ascent = fonts["regular"].getmetrics()[0]
    cover = Image.open(cover_path).convert("RGB")

    def font_for(sym, bold):
        o = ord(sym[0])
        if o in cmaps["regular"]:
            return fonts["bold" if bold else "regular"]
        if o in cmaps["fallback"]:
            return fonts["fallback"]
        return fonts["fallback2"]

    for path in sorted(shots.glob("*.json")):
        d = json.loads(path.read_text())
        w, h = d["width"], d["height"]
        term = Image.new("RGB", (w * cw, h * ch), BASE)
        draw = ImageDraw.Draw(term)
        thin = max(1, SCALE)
        thick = 2 * SCALE

        for y, row in enumerate(d["rows"]):
            for x, (sym, fg, bg, bold, italic, reverse, dim) in enumerate(row):
                fg, bg = rgb(fg, TEXT), rgb(bg, BASE)
                if reverse:
                    fg, bg = bg, fg
                x0, y0 = x * cw, y * ch
                if bg != BASE:
                    draw.rectangle([x0, y0, x0 + cw - 1, y0 + ch - 1], fill=bg)
                if sym in ("", " "):
                    continue
                cx, cy = x0 + cw // 2, y0 + ch // 2
                if sym in BOX or sym in ROUND:
                    arms = BOX.get(sym) or ROUND[sym]
                    lw = thick if sym in "━┃" else thin
                    if sym in ROUND:
                        # a quarter circle joining the two arms
                        r = cw // 2
                        up, right, down, left = (a == "1" for a in arms)
                        bx = x0 + cw if right else x0 - cw
                        by = y0 + ch if down else y0 - ch
                        ox = cx + (r if right else -r)
                        oy = cy + (r if down else -r)
                        start = {(True, True): 180, (False, True): 270,
                                 (True, False): 90, (False, False): 0}[(right, down)]
                        draw.arc([ox - r, oy - r, ox + r, oy + r], start, start + 90, fill=fg, width=lw)
                        if right:
                            draw.line([ox, cy, x0 + cw, cy], fill=fg, width=lw)
                        else:
                            draw.line([x0, cy, ox, cy], fill=fg, width=lw)
                        if down:
                            draw.line([cx, oy, cx, y0 + ch], fill=fg, width=lw)
                        else:
                            draw.line([cx, y0, cx, oy], fill=fg, width=lw)
                        continue
                    if arms[0] == "1":
                        draw.line([cx, y0, cx, cy], fill=fg, width=lw)
                    if arms[2] == "1":
                        draw.line([cx, cy, cx, y0 + ch], fill=fg, width=lw)
                    if arms[1] == "1":
                        draw.line([cx, cy, x0 + cw, cy], fill=fg, width=lw)
                    if arms[3] == "1":
                        draw.line([x0, cy, cx, cy], fill=fg, width=lw)
                    continue
                if sym == "▀":
                    draw.rectangle([x0, y0, x0 + cw - 1, y0 + ch // 2 - 1], fill=fg)
                    continue
                if sym == "▄":
                    draw.rectangle([x0, y0 + ch // 2, x0 + cw - 1, y0 + ch - 1], fill=fg)
                    continue
                if sym == "▌":
                    draw.rectangle([x0, y0, x0 + cw // 2 - 1, y0 + ch - 1], fill=fg)
                    continue
                if sym in EIGHTHS:
                    n = EIGHTHS.index(sym) + 1
                    top = y0 + ch - round(ch * n / 8)
                    draw.rectangle([x0, top, x0 + cw - 1, y0 + ch - 1], fill=fg)
                    continue
                f = font_for(sym, bold)
                top = y0 + (ch - FONT_SIZE) // 2 + (ascent - f.getmetrics()[0]) // 2
                draw.text((x0, top), sym, font=f, fill=fg)

        if d.get("cover"):
            c = d["cover"]
            box = (c["x"] * cw, c["y"] * ch, (c["x"] + c["w"]) * cw, (c["y"] + c["h"]) * ch)
            bw, bh = box[2] - box[0], box[3] - box[1]
            side = min(bw, bh)
            img = cover.resize((side, side), Image.LANCZOS)
            mask = Image.new("L", (side, side), 0)
            ImageDraw.Draw(mask).rounded_rectangle([0, 0, side - 1, side - 1], radius=max(4, side // 28), fill=255)
            draw.rectangle(box, fill=BASE)  # the cells under it hold a low-res fallback
            term.paste(img, (box[0] + (bw - side) // 2, box[1] + (bh - side) // 2), mask)

        # the window: rounded, padded, with a soft shadow on a clear background
        ww, wh = term.width + 2 * PAD, term.height + 2 * PAD
        canvas = Image.new("RGBA", (ww + 2 * MARGIN, wh + 2 * MARGIN), (0, 0, 0, 0))
        shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
        ImageDraw.Draw(shadow).rounded_rectangle(
            [MARGIN, MARGIN + 8 * SCALE, MARGIN + ww, MARGIN + wh + 8 * SCALE], RADIUS, fill=(0, 0, 0, 130))
        canvas = Image.alpha_composite(canvas, shadow.filter(ImageFilter.GaussianBlur(14 * SCALE)))
        window = Image.new("RGBA", (ww, wh), BASE)
        window.paste(term, (PAD, PAD))
        mask = Image.new("L", (ww, wh), 0)
        ImageDraw.Draw(mask).rounded_rectangle([0, 0, ww - 1, wh - 1], RADIUS, fill=255)
        canvas.paste(window, (MARGIN, MARGIN), mask)
        ImageDraw.Draw(canvas).rounded_rectangle(
            [MARGIN, MARGIN, MARGIN + ww - 1, MARGIN + wh - 1], RADIUS, outline="#26233a", width=SCALE)
        target = out / (path.stem + ".png")
        canvas.save(target, optimize=True)
        print(target)


if __name__ == "__main__":
    main()
