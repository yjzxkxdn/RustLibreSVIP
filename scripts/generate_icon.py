"""Generates the application icon.

    python scripts/generate_icon.py

Same composition as the original LibreSVIP mark - black rounded frame, the
"Libre"/"SVIP" wordmark in mint and magenta, an eighth note in the bottom-right
corner - except that the "r" of "Libre" is a Rust-style gear "R", so the port
is easy to tell apart from the original app.

Writes `crates/libresvip-gui/assets/icon.png` (1024x1024) and `icon.ico`
(16..256). Pure Pillow, no external logo file, so it is reproducible.
"""

from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

BASE = 1024
S = 2                      # supersampling factor
W = BASE * S

BLACK = (0, 0, 0, 255)
WHITE = (255, 255, 255, 255)
TEAL = (18, 214, 179, 255)        # "Libre"
MAGENTA = (196, 0, 226, 255)      # "SVIP"
FONT_PATH = r"C:\Windows\Fonts\arialbd.ttf"

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "crates" / "libresvip-gui" / "assets"


def px(value: float) -> int:
    return int(round(value * S))


def face(size: float) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(FONT_PATH, px(size))


def text_width(text: str, font: ImageFont.FreeTypeFont) -> float:
    left, _, right, _ = font.getbbox(text)
    return right - left


def text_height(text: str, font: ImageFont.FreeTypeFont) -> float:
    left, top, right, bottom = font.getbbox(text)
    return bottom - top


def gear_r(diameter: float, letter_size: float) -> Image.Image:
    """A gear ring with the "R" sitting in its hole, in the wordmark colour."""
    canvas = px(diameter)
    layer = Image.new("L", (canvas, canvas), 0)
    draw = ImageDraw.Draw(layer)
    centre = canvas / 2
    outer = centre - px(1)
    teeth = 9
    body = outer - px(diameter * 0.09)
    hole = body * 0.88

    draw.ellipse([centre - body, centre - body, centre + body, centre + body], fill=255)
    half_angle = math.pi / teeth * 0.5
    for index in range(teeth):
        angle = index * 2 * math.pi / teeth
        draw.polygon(
            [
                (
                    centre + body * math.cos(angle - half_angle),
                    centre + body * math.sin(angle - half_angle),
                ),
                (
                    centre + outer * math.cos(angle - half_angle * 0.62),
                    centre + outer * math.sin(angle - half_angle * 0.62),
                ),
                (
                    centre + outer * math.cos(angle + half_angle * 0.62),
                    centre + outer * math.sin(angle + half_angle * 0.62),
                ),
                (
                    centre + body * math.cos(angle + half_angle),
                    centre + body * math.sin(angle + half_angle),
                ),
            ],
            fill=255,
        )
    draw.ellipse([centre - hole, centre - hole, centre + hole, centre + hole], fill=0)

    mask = layer.resize((px(diameter), px(diameter)), Image.LANCZOS)
    mark = Image.new("RGBA", (px(diameter), px(diameter)), TEAL)
    mark.putalpha(mask)

    letter_font = ImageFont.truetype(FONT_PATH, px(letter_size))
    ink = Image.new("L", (canvas, canvas), 0)
    ink_draw = ImageDraw.Draw(ink)
    left, top, right, bottom = ink_draw.textbbox((0, 0), "R", font=letter_font)
    ink_draw.text(
        ((canvas - (right - left)) / 2 - left, (canvas - (bottom - top)) / 2 - top),
        "R",
        font=letter_font,
        fill=255,
    )
    hole_mask = Image.new("L", (canvas, canvas), 0)
    ImageDraw.Draw(hole_mask).ellipse(
        [centre - hole, centre - hole, centre + hole, centre + hole], fill=255
    )
    ink = ink.resize((px(diameter), px(diameter)), Image.LANCZOS)
    hole_mask = hole_mask.resize((px(diameter), px(diameter)), Image.LANCZOS)
    ink = Image.composite(ink, Image.new("L", ink.size, 0), hole_mask)
    mark.paste(Image.new("RGBA", mark.size, TEAL), (0, 0), ink)
    return mark


def draw_frame(draw: ImageDraw.ImageDraw) -> None:
    """Rounded square outline with the bottom-right corner left open."""
    draw.rounded_rectangle(
        [px(176), px(86), px(854), px(876)],
        radius=px(112),
        outline=BLACK,
        width=px(72),
    )
    # The note sits in the gap: clear the ends of the right and bottom edges.
    draw.rectangle([px(770), px(470), px(880), px(890)], fill=WHITE)
    draw.rectangle([px(540), px(790), px(880), px(890)], fill=WHITE)


def draw_note(image: Image.Image) -> None:
    layer = Image.new("RGBA", image.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    head_x, head_y, radius = 706, 840, 84
    draw.ellipse(
        [
            px(head_x - radius),
            px(head_y - radius),
            px(head_x + radius),
            px(head_y + radius),
        ],
        fill=BLACK,
    )
    draw.rectangle(
        [px(head_x + radius - 26), px(566), px(head_x + radius + 4), px(head_y)],
        fill=BLACK,
    )
    draw.polygon(
        [
            (px(head_x + radius + 4), px(566)),
            (px(846), px(512)),
            (px(846), px(596)),
            (px(head_x + radius + 4), px(634)),
        ],
        fill=BLACK,
    )
    image.alpha_composite(layer)


def draw_wordmark(image: Image.Image) -> None:
    """`Libre` (its "r" a gear R) above `SVIP`, centred like the original."""
    size = 180.0
    font = face(size)
    e_height = text_height("e", font) / S
    r_size = e_height / 0.72 * 0.92     # the R: almost as tall as "e"
    gear = 1.70 * e_height            # the gear is a little taller than "e"
    gap_before, gap_after = 6.0, 10.0
    left_edge, right_edge = 248, 782
    inner = px(right_edge - left_edge)

    def layout(scale: float) -> float:
        return (
            text_width("Lib", font)
            + px(gap_before * scale)
            + px(gear * scale)
            + px(gap_after * scale)
            + text_width("e", font)
        )

    scale = 1.0
    width = layout(scale)
    if width > inner:
        scale = inner / width
        size *= scale
        font = face(size)
        width = layout(scale)

    x = px(left_edge) + (inner - width) / 2
    baseline = px(432)
    draw = ImageDraw.Draw(image)
    draw.text((x, baseline), "Lib", font=font, fill=TEAL, anchor="ls")
    x += text_width("Lib", font) + px(gap_before * scale)
    mark = gear_r(gear * scale, r_size * scale)
    image.alpha_composite(mark, (int(x), int(baseline - mark.height)))
    x += mark.width + px(gap_after * scale)
    draw.text((x, baseline), "e", font=font, fill=TEAL, anchor="ls")

    svip_width = text_width("SVIP", font)
    draw.text(
        (px(left_edge) + (inner - svip_width) / 2, px(602)),
        "SVIP",
        font=font,
        fill=MAGENTA,
        anchor="ls",
    )


def main() -> None:
    image = Image.new("RGBA", (W, W), WHITE)
    draw_frame(ImageDraw.Draw(image))
    draw_wordmark(image)
    draw_note(image)
    icon = image.resize((BASE, BASE), Image.LANCZOS)

    ASSETS.mkdir(parents=True, exist_ok=True)
    icon.save(ASSETS / "icon.png")
    icon.save(
        ASSETS / "icon.ico",
        sizes=[(value, value) for value in (16, 24, 32, 48, 64, 128, 256)],
    )
    print(f"wrote {ASSETS / 'icon.png'} and icon.ico")


if __name__ == "__main__":
    main()
