"""Render the Tidemail logo (concept B, "Rising tide") to PNG and ICO.

Geometry matches docs/design/logo/tidemail.svg (64-unit viewBox). Drawn at
16x and scaled down, so no SVG renderer is needed. Small sizes use heavier
strokes so the envelope survives at 16 px.

    python scripts/logo/render.py

Writes crates/app/assets/logo/tidemail.ico and docs/design/logo/png/*.png.
"""

from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
ICO = ROOT / "crates/app/assets/logo/tidemail.ico"
PNG_DIR = ROOT / "docs/design/logo/png"
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]

K = 16  # supersampling: 64 units -> 1024 px
TEAL_A, TEAL_B = (0x2F, 0xB5, 0xA5), (0x0B, 0x7F, 0x74)
DEEP_A, DEEP_B = (0x16, 0x26, 0x2A), (0x0C, 0x11, 0x13)
LINE = (0xE3, 0xEB, 0xED)


def gradient(size, a, b, diagonal):
    w, h = size
    img = Image.new("RGB", size)
    px = img.load()
    for y in range(h):
        for x in range(w):
            t = (x + y) / (w + h - 2) if diagonal else y / (h - 1)
            px[x, y] = tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))
    return img


def wave_points(x0, y0, step, amp, count, n=24):
    """Quadratic segments alternating up/down, like SVG `q` then `t`."""
    pts = []
    for i in range(count):
        sx, ex = x0 + i * step, x0 + (i + 1) * step
        cx, cy = sx + step / 2, y0 - amp if i % 2 == 0 else y0 + amp
        for j in range(n + 1):
            t = j / n
            x = (1 - t) ** 2 * sx + 2 * (1 - t) * t * cx + t**2 * ex
            y = (1 - t) ** 2 * y0 + 2 * (1 - t) * t * cy + t**2 * y0
            pts.append((x, y))
    return pts


def s(v):
    return v * K


def draw(stroke, zoom=1.0):
    """`zoom` enlarges the envelope (not the tile) about its centre."""
    N = 64 * K

    def ex(x):
        return s(32 + (x - 32) * zoom)

    def ey(y):
        return s(33 + (y - 33) * zoom)

    out = Image.new("RGBA", (N, N), (0, 0, 0, 0))

    # Tile: rounded square, vertical deep gradient.
    tile_mask = Image.new("L", (N, N), 0)
    ImageDraw.Draw(tile_mask).rounded_rectangle([0, 0, N - 1, N - 1], radius=s(14), fill=255)
    out.paste(gradient((N, N), DEEP_A, DEEP_B, diagonal=False), (0, 0), tile_mask)

    env = [ex(13), ey(19), ex(51), ey(47)]
    env_mask = Image.new("L", (N, N), 0)
    ImageDraw.Draw(env_mask).rounded_rectangle(env, radius=s(3.5 * zoom), fill=255)

    # Faint paper inside the envelope.
    paper = Image.new("RGBA", (N, N), (255, 255, 255, 0))
    paper.putalpha(env_mask.point(lambda v: v * 20 // 255))
    out = Image.alpha_composite(out, paper)

    # Water: wave line at y=36 filled down, clipped to the envelope.
    pts = [(ex(x), ey(y)) for x, y in wave_points(10, 36, 9.5, 4, 5)]
    water_mask = Image.new("L", (N, N), 0)
    ImageDraw.Draw(water_mask).polygon(pts + [(ex(57.5), ey(50)), (ex(10), ey(50))], fill=255)
    water_mask = ImageChops.multiply(water_mask, env_mask)
    out.paste(gradient((N, N), TEAL_A, TEAL_B, diagonal=True), (0, 0), water_mask)

    # Envelope outline and V flap.
    d = ImageDraw.Draw(out)
    w = round(s(stroke * zoom))
    # PIL draws an outline inside its box; SVG centres the stroke on the edge.
    half = stroke / 2
    d.rounded_rectangle(
        [ex(13 - half), ey(19 - half), ex(51 + half), ey(47 + half)],
        radius=s((3.5 + half) * zoom),
        outline=LINE + (255,),
        width=w,
    )
    flap = [(ex(14.5), ey(21)), (ex(32), ey(33)), (ex(49.5), ey(21))]
    d.line(flap, fill=LINE + (255,), width=w, joint="curve")
    r = w / 2
    for x, y in (flap[0], flap[2]):
        d.ellipse([x - r, y - r, x + r, y + r], fill=LINE + (255,))
    return out


def main():
    small, medium, large = draw(4.0, zoom=1.25), draw(4.5), draw(3.5)
    images = []
    PNG_DIR.mkdir(parents=True, exist_ok=True)
    for size in SIZES:
        src = small if size <= 24 else medium if size <= 32 else large
        img = src.resize((size, size), Image.LANCZOS)
        img.save(PNG_DIR / f"tidemail-{size}.png")
        images.append(img)
    large.resize((512, 512), Image.LANCZOS).save(PNG_DIR / "tidemail-512.png")
    ICO.parent.mkdir(parents=True, exist_ok=True)
    images[-1].save(ICO, format="ICO", sizes=[(n, n) for n in SIZES], append_images=images[:-1])
    print(f"wrote {ICO.relative_to(ROOT)} and {len(SIZES) + 1} PNGs")


if __name__ == "__main__":
    main()
