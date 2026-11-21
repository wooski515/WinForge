"""Рисует иконку WinForge: наковальня, на которой «куются» четыре окна Windows.

Запуск (нужен Pillow):  python assets/make_icon.py
Создаёт рядом:
  winforge.ico        — иконка exe и ярлыков (16…256 px)
  winforge.png        — 256 px, для README и проверки
  icon_64.rgba / icon_128.rgba — сырые RGBA-пиксели для окна и логотипа в программе
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

HERE = Path(__file__).resolve().parent
S = 1024  # рисуем крупно, потом уменьшаем — так края гладкие


def lerp(a, b, t):
    return tuple(int(round(x + (y - x) * t)) for x, y in zip(a, b))


def background():
    top, bottom = (78, 146, 255), (28, 58, 150)
    grad = Image.new("RGBA", (S, S))
    px = grad.load()
    for y in range(S):
        c = lerp(top, bottom, y / (S - 1)) + (255,)
        for x in range(S):
            px[x, y] = c
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([24, 24, S - 24, S - 24], radius=230, fill=255)
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(grad, (0, 0), mask)
    # Лёгкий блик сверху.
    shine = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(shine).ellipse([-200, -620, S + 200, 380], fill=(255, 255, 255, 34))
    shine_mask = Image.new("L", (S, S), 0)
    shine_mask.paste(mask)
    out = Image.alpha_composite(out, Image.composite(shine, Image.new("RGBA", (S, S)), shine_mask))
    return out


def anvil(draw, color):
    # Верхняя плоскость с рогом слева, талия и основание.
    top = [
        (128, 600),   # кончик рога
        (300, 572),
        (840, 572),
        (840, 668),
        (720, 668),
        (650, 712),   # переход в талию
        (650, 788),
        (760, 840),
        (760, 896),
        (264, 896),
        (264, 840),
        (374, 788),
        (374, 712),
        (300, 668),
        (250, 650),
    ]
    draw.polygon(top, fill=color)


def panes(draw, color):
    # Четыре окна Windows над наковальней.
    size, gap = 176, 30
    left = (S - (2 * size + gap)) // 2 + 20
    top = 150
    for row in range(2):
        for col in range(2):
            x = left + col * (size + gap)
            y = top + row * (size + gap)
            draw.rounded_rectangle([x, y, x + size, y + size], radius=22, fill=color)


def spark(draw, cx, cy, r, color):
    w = r * 0.28
    draw.polygon([(cx, cy - r), (cx + w, cy - w), (cx + r, cy), (cx + w, cy + w),
                  (cx, cy + r), (cx - w, cy + w), (cx - r, cy), (cx - w, cy - w)], fill=color)


def render():
    img = background()
    # Мягкая тень под фигурами.
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shadow)
    anvil(sd, (8, 20, 60, 120))
    panes(sd, (8, 20, 60, 90))
    shadow = shadow.transform((S, S), Image.AFFINE, (1, 0, -10, 0, 1, -16)).filter(ImageFilter.GaussianBlur(14))
    img = Image.alpha_composite(img, shadow)

    fg = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(fg)
    panes(d, (236, 244, 255, 255))
    anvil(d, (250, 252, 255, 255))
    # Тонкая тёмная линия — грань наковальни.
    d.line([(300, 610), (840, 610)], fill=(176, 196, 232, 255), width=10)
    spark(d, 836, 498, 64, (255, 168, 48, 255))
    spark(d, 912, 404, 34, (255, 200, 90, 255))
    spark(d, 760, 430, 26, (255, 200, 90, 255))
    return Image.alpha_composite(img, fg)


def main():
    big = render()
    sizes = [256, 128, 64, 48, 40, 32, 24, 20, 16]
    frames = {n: big.resize((n, n), Image.LANCZOS) for n in sizes}
    frames[256].save(HERE / "winforge.png")
    frames[256].save(HERE / "winforge.ico", sizes=[(n, n) for n in sizes],
                     append_images=[frames[n] for n in sizes[1:]])
    for n in (64, 128):
        (HERE / f"icon_{n}.rgba").write_bytes(frames[n].tobytes())
    # Лист для проверки на тёмном и светлом фоне.
    sheet = Image.new("RGBA", (40 + sum(s + 20 for s in sizes), 2 * 300), (17, 19, 24, 255))
    ImageDraw.Draw(sheet).rectangle([0, 300, sheet.width, 600], fill=(240, 240, 240, 255))
    x = 20
    for n in sizes:
        for y0 in (0, 300):
            sheet.alpha_composite(frames[n], (x, y0 + 150 - n // 2))
        x += n + 20
    sheet.save(HERE / "preview.png")
    print("ok")


if __name__ == "__main__":
    main()
