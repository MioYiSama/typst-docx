#!/usr/bin/env python3
"""Rasterize two PDFs and report per-page pixel difference + overlay images."""
import sys

import pypdfium2 as pdfium
from PIL import Image, ImageChops, ImageFilter

REF, GOT, OUTDIR, DPI = sys.argv[1], sys.argv[2], sys.argv[3], 150


def pages(path):
    doc = pdfium.PdfDocument(path)
    out = []
    for page in doc:
        bmp = page.render(scale=DPI / 72)
        out.append(bmp.to_pil().convert("RGB"))
    return out


ref_pages = pages(REF)
got_pages = pages(GOT)
if len(ref_pages) != len(got_pages):
    print(f"PAGE COUNT MISMATCH: ref={len(ref_pages)} got={len(got_pages)}")

worst = 0.0
for i, (a, b) in enumerate(zip(ref_pages, got_pages), 1):
    if a.size != b.size:
        b = b.resize(a.size)
    blur = ImageFilter.GaussianBlur(1)
    diff = ImageChops.difference(a.filter(blur), b.filter(blur)).convert("L")
    hist = diff.histogram()
    bad = sum(hist[25:])
    total = a.size[0] * a.size[1]
    pct = 100.0 * bad / total
    worst = max(worst, pct)

    # Overlay: ref in red channel, got in green channel -> matches are yellow/gray
    overlay = Image.merge("RGB", (a.convert("L"), b.convert("L"), a.convert("L").point(lambda p: 255)))
    overlay = Image.blend(a, b, 0.5)
    heat = diff.point(lambda p: 255 if p > 24 else 0)
    red = Image.new("RGB", a.size, (255, 0, 0))
    marked = Image.composite(red, overlay, heat)
    marked.save(f"{OUTDIR}/diff-{i}.png")
    print(f"page {i}: {pct:.3f}% pixels differ (|d|>24)")

print(f"WORST {worst:.3f}%  {'PASS' if worst < 0.5 else 'CHECK'}")
