#!/usr/bin/env python3
"""
Generate a minimal 8x16 bitmap font for Maya's framebuffer console.

The script prefers Pillow for readable glyph rendering and falls back to
simple placeholder box glyphs if Pillow is unavailable.
"""

from pathlib import Path

GLYPH_WIDTH = 8
GLYPH_HEIGHT = 16
FIRST = 32
LAST = 127


def generate_with_pillow() -> bytearray:
    from PIL import Image, ImageDraw, ImageFont

    font = ImageFont.load_default()
    data = bytearray()

    for code in range(FIRST, LAST + 1):
        image = Image.new("1", (GLYPH_WIDTH, GLYPH_HEIGHT), 0)
        draw = ImageDraw.Draw(image)
        ch = chr(code)
        bbox = draw.textbbox((0, 0), ch, font=font)
        text_w = bbox[2] - bbox[0]
        text_h = bbox[3] - bbox[1]
        x = max((GLYPH_WIDTH - text_w) // 2 - bbox[0], 0)
        y = max((GLYPH_HEIGHT - text_h) // 2 - bbox[1], 0)
        draw.text((x, y), ch, fill=1, font=font)

        for row in range(GLYPH_HEIGHT):
            byte = 0
            for col in range(GLYPH_WIDTH):
                if image.getpixel((col, row)):
                    byte |= 1 << (7 - col)
            data.append(byte)

    return data


def generate_fallback() -> bytearray:
    data = bytearray()
    for code in range(FIRST, LAST + 1):
        if code == 32:
            data.extend([0x00] * GLYPH_HEIGHT)
            continue

        glyph = [0x00] * GLYPH_HEIGHT
        glyph[0] = 0x7E
        glyph[-1] = 0x7E
        for row in range(1, GLYPH_HEIGHT - 1):
            glyph[row] = 0x42
        data.extend(glyph)
    return data


def main() -> None:
    try:
        out = generate_with_pillow()
    except Exception:
        out = generate_fallback()

    target = Path(__file__).resolve().parent.parent / "crates/kernel/src/fb/font8x16.bin"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(out)
    print(f"Font written: {len(out)} bytes ({len(out) // GLYPH_HEIGHT} glyphs)")


if __name__ == "__main__":
    main()
