#!/usr/bin/env python3
"""Generate the installable-web-app (PWA) icons from the mobile app icon sources.

    python3 webapp/scripts/pwa-icons.py

Sources: mobile/shared/assets/{diner,partner}-icon*.png (1024px). Output:
webapp/public/icons/. Re-run after changing the app icons.
"""
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "mobile" / "shared" / "assets"
OUT = ROOT / "webapp" / "public" / "icons"
OUT.mkdir(parents=True, exist_ok=True)

for app in ("diner", "partner"):
    full = Image.open(SRC / f"{app}-icon.png").convert("RGBA")
    # "any": the full-bleed icon. Maskable: the adaptive-icon layers, whose
    # foreground already sits inside the safe zone.
    bg = Image.open(SRC / f"{app}-icon-background.png").convert("RGBA")
    fg = Image.open(SRC / f"{app}-icon-foreground.png").convert("RGBA")
    maskable = Image.alpha_composite(bg, fg)
    for size in (192, 512):
        full.resize((size, size), Image.LANCZOS).save(OUT / f"{app}-{size}.png", optimize=True)
        maskable.resize((size, size), Image.LANCZOS).save(OUT / f"{app}-maskable-{size}.png", optimize=True)
    # iOS ignores transparency and applies its own corner mask: flatten to RGB.
    full.convert("RGB").resize((180, 180), Image.LANCZOS).save(OUT / f"{app}-apple-touch-icon.png", optimize=True)
    # Shortcut icons (monochrome-ish small glyph on brand colour).
    full.resize((96, 96), Image.LANCZOS).save(OUT / f"{app}-96.png", optimize=True)
print("icons written to", OUT)
