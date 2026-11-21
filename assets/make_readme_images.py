"""Картинки для README: кнопка DOWNLOAD, карта меню и обложка со скриншотом.

Запуск (нужен Pillow, шрифты Segoe UI из Windows):  python assets/make_readme_images.py
Пишет в docs/images/.
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "images"
SHOTS = ROOT / "docs" / "screenshots"
FONTS = Path(r"C:\Windows\Fonts")

BG = (17, 19, 24)
CARD = (27, 31, 39)
LINE = (44, 50, 62)
TEXT = (232, 236, 243)
MUTED = (150, 158, 172)
ACCENT = (72, 140, 255)
ORANGE = (255, 168, 48)


def font(name, size):
    return ImageFont.truetype(str(FONTS / name), size)


def emoji(draw, xy, ch, size):
    draw.text(xy, ch, font=font("seguiemj.ttf", size), embedded_color=True)


def gradient(size, top, bottom, radius):
    w, h = size
    g = Image.new("RGBA", size)
    px = g.load()
    for y in range(h):
        t = y / max(1, h - 1)
        c = tuple(int(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,)
        for x in range(w):
            px[x, y] = c
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, w - 1, h - 1], radius=radius, fill=255)
    out = Image.new("RGBA", size, (0, 0, 0, 0))
    out.paste(g, (0, 0), mask)
    return out


def with_shadow(img, pad=40, blur=22, alpha=150):
    w, h = img.size
    canvas = Image.new("RGBA", (w + 2 * pad, h + 2 * pad), (0, 0, 0, 0))
    shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle([pad, pad + 10, pad + w, pad + h + 10], radius=26, fill=(0, 0, 0, alpha))
    canvas = Image.alpha_composite(canvas, shadow.filter(ImageFilter.GaussianBlur(blur)))
    canvas.alpha_composite(img, (pad, pad))
    return canvas


# ---------------------------------------------------------------------------
# Кнопка DOWNLOAD (рисуется в 2× для чётких краёв)
# ---------------------------------------------------------------------------
def download_button():
    k = 2
    w, h = 460 * k, 112 * k
    img = gradient((w, h), (90, 160, 255), (40, 92, 220), 28 * k)
    d = ImageDraw.Draw(img)
    # Значок «скачать» в кружке
    cx, cy, r = 62 * k, h // 2, 34 * k
    ring = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ImageDraw.Draw(ring).ellipse([cx - r, cy - r, cx + r, cy + r], fill=(255, 255, 255, 46))
    img.alpha_composite(ring)
    d = ImageDraw.Draw(img)
    d.line([(cx, cy - 18 * k), (cx, cy + 10 * k)], fill="white", width=7 * k)
    d.polygon([(cx - 15 * k, cy - 2 * k), (cx + 15 * k, cy - 2 * k), (cx, cy + 16 * k)], fill="white")
    d.line([(cx - 18 * k, cy + 22 * k), (cx + 18 * k, cy + 22 * k)], fill="white", width=6 * k)
    d.text((118 * k, 18 * k), "DOWNLOAD", font=font("segoeuib.ttf", 46 * k), fill="white")
    d.text((120 * k, 72 * k), "WinForge.exe  ·  Windows 10 / 11  ·  x64", font=font("segoeui.ttf", 19 * k), fill=(225, 236, 255))
    img = img.resize((w // k, h // k), Image.LANCZOS)
    with_shadow(img, pad=18, blur=12, alpha=110).save(OUT / "download.png")


# ---------------------------------------------------------------------------
# Карта меню: два режима крупным текстом
# ---------------------------------------------------------------------------
BUILD = [
    ("💿", "Образ Windows", "Офіційний ISO, редакція, мова, акаунт, Wi-Fi, ключ"),
    ("📦", "Програми", "80 програм через winget і свої інсталятори"),
    ("🛡", "Приватність", "Телеметрія, реклама, Copilot, Recall, ШІ"),
    ("⚙", "Оптимізація", "Швидкодія, Провідник, панель завдань, оновлення"),
    ("🗑", "Видалення", "69 вбудованих застосунків і старих компонентів"),
    ("🔨", "Збірка", "Флешка або ISO — з прогресом просто у вікні"),
]
LIVE = [
    ("⚡", "Застосувати", "Ті самі твіки — до цієї системи, з відкатом"),
    ("✨", "Очищення", "Temp, кеш оновлень, браузери — реальний розмір"),
    ("🚀", "Автозапуск", "Що стартує разом з Windows"),
    ("📶", "Мережа", "DNS з шифруванням (DoH), пінг"),
    ("🔑", "Активація", "Свій ключ або KMS-сервер організації"),
    ("🧰", "Ще 6 інструментів", "hosts, реєстр, меню, PATH, диски, залізо"),
]


def menu_map():
    w, h = 1660, 980
    img = Image.new("RGBA", (w, h), BG)
    d = ImageDraw.Draw(img)
    title = font("segoeuib.ttf", 34)
    item = font("seguisb.ttf", 30)
    sub = font("segoeui.ttf", 21)
    col_w, top = 730, 60
    for i, (head, ico, items, color) in enumerate([
        ("Зібрати нову Windows", "💿", BUILD, ACCENT),
        ("Налаштувати цю Windows", "💻", LIVE, ORANGE),
    ]):
        x = 60 + i * (col_w + 80)
        card_h = h - 2 * top
        card = Image.new("RGBA", (col_w, card_h), (0, 0, 0, 0))
        ImageDraw.Draw(card).rounded_rectangle([0, 0, col_w - 1, card_h - 1], radius=28, fill=CARD, outline=LINE, width=2)
        img.alpha_composite(card, (x, top))
        # Шапка режима
        d.rounded_rectangle([x + 28, top + 28, x + col_w - 28, top + 108], radius=18, fill=color)
        emoji(d, (x + 50, top + 42), ico, 38)
        d.text((x + 110, top + 44), head, font=title, fill="white")
        y = top + 140
        for n, (e, name, desc) in enumerate(items, 1):
            d.rounded_rectangle([x + 28, y, x + col_w - 28, y + 104], radius=16, fill=(33, 38, 48))
            emoji(d, (x + 50, y + 26), e, 40)
            d.text((x + 116, y + 16), name, font=item, fill=TEXT)
            d.text((x + 117, y + 60), desc, font=sub, fill=MUTED)
            if i == 0:
                d.text((x + col_w - 70, y + 30), str(n), font=font("segoeuib.ttf", 32), fill=(70, 80, 100))
            y += 116
    # Стрелка-подсказка между колонками
    d.text((60 + col_w + 20, h // 2 - 16), "або", font=font("segoeuii.ttf", 26), fill=MUTED)
    img.save(OUT / "menu.png")


# ---------------------------------------------------------------------------
# Обложка: главный экран в рамке окна с тенью
# ---------------------------------------------------------------------------
def framed(name, out_name, scale=1.0):
    shot = Image.open(SHOTS / name).convert("RGBA")
    w, h = shot.size
    bar = 40
    win = Image.new("RGBA", (w, h + bar), (0, 0, 0, 0))
    mask = Image.new("L", win.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, w - 1, h + bar - 1], radius=16, fill=255)
    body = Image.new("RGBA", win.size, (32, 36, 44, 255))
    bd = ImageDraw.Draw(body)
    icon = Image.open(ROOT / "assets" / "winforge.png").convert("RGBA").resize((22, 22), Image.LANCZOS)
    body.alpha_composite(icon, (14, 9))
    bd.text((44, 9), "WinForge", font=font("segoeui.ttf", 16), fill=(200, 206, 216))
    c = (170, 176, 188)
    x0 = w - 132
    bd.line([(x0, 20), (x0 + 12, 20)], fill=c, width=1)                       # свернуть
    bd.rectangle([x0 + 44, 14, x0 + 54, 24], outline=c, width=1)            # развернуть
    bd.line([(x0 + 88, 14), (x0 + 98, 24)], fill=c, width=1)                # закрыть
    bd.line([(x0 + 98, 14), (x0 + 88, 24)], fill=c, width=1)
    body.alpha_composite(shot, (0, bar))
    win.paste(body, (0, 0), mask)
    if scale != 1.0:
        win = win.resize((int(win.width * scale), int(win.height * scale)), Image.LANCZOS)
    with_shadow(win, pad=36, blur=20, alpha=140).save(OUT / out_name)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    download_button()
    menu_map()
    framed("home.png", "hero.png")
    print("ok")


if __name__ == "__main__":
    main()
